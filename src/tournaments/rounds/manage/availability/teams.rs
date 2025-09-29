use std::collections::HashMap;

use diesel::{
    connection::LoadConnection, dsl::case_when, prelude::*, sqlite::Sqlite,
};
use hypertext::prelude::*;
use itertools::Either;
use rocket::{
    FromForm, State,
    form::Form,
    futures::{SinkExt, StreamExt},
    get, post,
    response::Redirect,
};
use tokio::{
    sync::broadcast::{Receiver, Sender},
    task::spawn_blocking,
};
use uuid::Uuid;

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{
        tournament_rounds, tournament_team_availability, tournament_teams,
    },
    state::{Conn, DbPool},
    template::Page,
    tournaments::{Tournament, rounds::Round, teams::Team},
    util_resp::{StandardResponse, err_not_found, see_other_ok, success},
};

pub struct ManageAvailabilityTable<'r> {
    tournament_id: &'r str,
    rounds: &'r [Round],
    teams: &'r [Team],
    teams_and_availability: &'r HashMap<(String, String), bool>,
}

impl<'r> Renderable for ManageAvailabilityTable<'r> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            table class="table table-striped table-bordered" id="participantsTable"
                  hx-ext="ws" hx-swap-oob="morphdom"
                  "ws-connect"=(format!("/tournaments/{}/rounds/{}/availability/teams?channel", self.tournament_id, self.rounds[0].seq)) {
                thead {
                    tr {
                        th scope="col" {
                            "#"
                        }
                        th scope="col" {
                            "Team name"
                        }
                        @for round in self.rounds {
                            th scope="col" {
                                (round.name) "?"
                            }
                        }

                    }
                }
                tbody {
                    @for (i, team) in self.teams.iter().enumerate() {
                        tr {
                            th scope="row" {
                                (i)
                            }
                            td {
                                (team.name)
                            }
                            @for round in self.rounds {
                                @let available = self.teams_and_availability.get(&(team.id.clone(), round.id.clone())).unwrap_or(&false);
                                td {
                                    form method="post"
                                         action=(format!("/tournaments/{}/rounds/{}/update_team_eligibility", self.tournament_id, round.id)) {
                                        input type="text" hidden value=(team.id) name="team";
                                        @if *available {
                                        } @else {
                                            input type="text" hidden value="true" name="available";
                                        }
                                        div style="display: inline-block; position: relative;" {
                                            @if *available {
                                                input type="checkbox" checked;
                                            } @else {
                                                input type="checkbox";
                                            }
                                            input type="submit" value=""
                                                style="left: 0; height: 100%; opacity: 0; position: absolute; top: 0; width: 100%";
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }.render_to(buffer);
    }
}

fn get_teams_and_availability(
    tournament_id: &str,
    round_seq: usize,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> HashMap<(std::string::String, std::string::String), bool> {
    tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(tournament_id))
        .order_by(tournament_teams::id.asc())
        .inner_join(
            tournament_rounds::table
                .on(tournament_rounds::seq.eq(round_seq as i64)),
        )
        .left_outer_join(tournament_team_availability::table)
        .filter(
            tournament_team_availability::round_id.eq(tournament_rounds::id),
        )
        .select((
            tournament_teams::id,
            tournament_rounds::id,
            case_when(
                tournament_team_availability::available
                    .nullable()
                    .is_not_null(),
                tournament_team_availability::available
                    .nullable()
                    .assume_not_null(),
            )
            .otherwise(false),
        ))
        .load::<(String, String, bool)>(&mut *conn)
        .unwrap()
        .into_iter()
        .map(|(team, round, bool)| ((team, round), bool))
        .collect::<HashMap<_, _>>()
}

#[get("/tournaments/<tournament_id>/rounds/<round_seq>/availability/teams")]
pub async fn view_team_availability(
    tournament_id: &str,
    round_seq: usize,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = tournament_rounds::table
        .filter(
            tournament_rounds::tournament_id
                .eq(&tournament_id)
                .and(tournament_rounds::seq.eq(round_seq as i64)),
        )
        .order_by(tournament_rounds::id.asc())
        .load::<Round>(&mut *conn)
        .unwrap();

    let teams = tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(tournament_id))
        .order_by(tournament_teams::id.asc())
        .load::<Team>(&mut *conn)
        .unwrap();

    let teams_and_availability =
        get_teams_and_availability(tournament_id, round_seq, &mut *conn);

    let table = ManageAvailabilityTable {
        tournament_id,
        rounds: &rounds.clone(),
        teams: &teams,
        teams_and_availability: &teams_and_availability,
    };

    success(
        Page::default()
            // todo: add option `with_htmx_ws()` and remove `extra_head` (?)
            .extra_head(
                maud! {
                    script src="https://cdn.jsdelivr.net/npm/htmx-ext-ws@2.0.2" crossorigin="anonymous" {
                    }
                }
            )
            .user(user)
            .tournament(tournament)
            .body(maud! {
                h1 {
                    "Manage availabilities for rounds "
                    @for (i, round) in rounds.iter().enumerate() {
                        @if i > 0 {
                            ", "
                        }
                        (round.name)
                    }
                }
                (table)
            })
            .render(),
    )
}

#[get(
    "/tournaments/<tournament_id>/rounds/<round_seq>/availability/teams?channel"
)]
pub async fn team_availability_updates(
    tournament_id: &str,
    round_seq: i64,
    ws: rocket_ws::WebSocket,
    pool: &State<DbPool>,
    rx: &State<Receiver<Msg>>,
    user: User<false>,
) -> Option<rocket_ws::Channel<'static>> {
    let pool: DbPool = pool.inner().clone();

    let pool1 = pool.clone();
    let mut conn = spawn_blocking(move || pool1.get().unwrap()).await.unwrap();

    let tournament = Tournament::fetch(&tournament_id, &mut conn).ok()?;
    tournament
        .check_user_is_superuser(&user.id, &mut conn)
        .ok()?;

    let rounds = diesel::dsl::select(diesel::dsl::exists(
        tournament_rounds::table.filter(
            tournament_rounds::tournament_id
                .eq(tournament_id.to_string())
                .and(tournament_rounds::seq.eq(round_seq)),
        ),
    ))
    .get_result::<bool>(&mut conn)
    .unwrap();

    if !rounds {
        return None;
    }

    let mut rx: Receiver<Msg> = rx.inner().resubscribe();

    let tournament_id = tournament_id.to_string();

    Some(ws.channel(move |mut stream| {
        Box::pin(async move {
            loop {
                let msg = {
                    let msg = tokio::select! {
                        msg = rx.recv() => Either::Left(msg.unwrap()),
                        msg = stream.next() => Either::Right(msg.unwrap()),
                    };
                    let msg = match msg {
                        Either::Left(left) => left,
                        Either::Right(close) => {
                            let close = close?;
                            if matches!(close, rocket_ws::Message::Close(_)) {
                                return Ok(());
                            } else {
                                continue;
                            }
                        }
                    };

                    if msg.tournament.id == tournament.id
                        && matches!(
                            msg.inner,
                            MsgContents::TeamAvailabilityUpdate
                        )
                    {
                        msg
                    } else {
                        continue;
                    }
                };

                match msg.inner {
                    MsgContents::TeamAvailabilityUpdate => {
                        let pool1 = pool.clone();
                        let tournament_id = tournament_id.to_string();

                        let table = spawn_blocking(move || {
                            let mut conn = pool1.get().unwrap();

                            let rounds = tournament_rounds::table
                                .filter(
                                    tournament_rounds::tournament_id
                                        .eq(&tournament_id)
                                        .and(
                                            tournament_rounds::seq
                                                .eq(round_seq as i64),
                                        ),
                                )
                                .order_by(tournament_rounds::id.asc())
                                .load::<Round>(&mut *conn)
                                .unwrap();

                            let teams = tournament_teams::table
                                .filter(
                                    tournament_teams::tournament_id
                                        .eq(&tournament_id),
                                )
                                .order_by(tournament_teams::id.asc())
                                .load::<Team>(&mut *conn)
                                .unwrap();

                            let teams_and_availability =
                                get_teams_and_availability(
                                    &tournament_id,
                                    round_seq as usize,
                                    &mut *conn,
                                );

                            let table = ManageAvailabilityTable {
                                tournament_id: &tournament_id,
                                rounds: &rounds.clone(),
                                teams: &teams,
                                teams_and_availability: &teams_and_availability,
                            };

                            table.render().into_inner()
                        })
                        .await
                        .unwrap();

                        let _ =
                            stream.send(rocket_ws::Message::Text(table)).await;
                    }
                    _ => unreachable!(),
                };
            }
        })
    }))
}

#[derive(FromForm)]
pub struct UpdateTeamAvailabilityForm {
    available: bool,
    team: String,
}

#[post(
    "/tournaments/<tournament_id>/rounds/<round_id>/update_team_eligibility",
    data = "<form>"
)]
pub async fn update_team_eligibility(
    tournament_id: &str,
    round_id: &str,
    user: User<true>,
    mut conn: Conn<true>,
    form: Form<UpdateTeamAvailabilityForm>,
    tx: &rocket::State<Sender<Msg>>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let team = match tournament_teams::table
        .filter(
            tournament_teams::tournament_id
                .eq(tournament_id)
                .and(tournament_teams::id.eq(&form.team)),
        )
        .first::<Team>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(team) => team,
        None => return err_not_found(),
    };

    let round = match tournament_rounds::table
        .filter(
            tournament_rounds::id
                .eq(round_id)
                .and(tournament_rounds::tournament_id.eq(tournament_id)),
        )
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(round) => round,
        None => return err_not_found(),
    };

    // TODO: check that a team can't be allocated to multiple concurrent rounds

    let n = diesel::insert_into(tournament_team_availability::table)
        .values((
            tournament_team_availability::id.eq(Uuid::now_v7().to_string()),
            tournament_team_availability::round_id.eq(&round.id),
            tournament_team_availability::team_id.eq(&team.id),
            tournament_team_availability::available.eq(form.available),
        ))
        .on_conflict((
            tournament_team_availability::round_id,
            tournament_team_availability::team_id,
        ))
        .do_update()
        .set(tournament_team_availability::available.eq(form.available))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    diesel::update(
        tournament_team_availability::table.filter(
            tournament_team_availability::team_id.eq(&team.id).and(
                diesel::dsl::exists(
                    tournament_rounds::table.filter(
                        tournament_rounds::tournament_id
                            .eq(&tournament.id)
                            .and(
                                tournament_rounds::seq
                                    .eq(round.seq)
                                    .and(tournament_rounds::id.ne(&round.id)),
                            )
                            .and(
                                tournament_team_availability::round_id
                                    .eq(tournament_rounds::id),
                            ),
                    ),
                ),
            ),
        ),
    )
    .set(tournament_team_availability::available.eq(false))
    .execute(&mut *conn)
    .unwrap();

    let _ = tx.send(Msg {
        tournament,
        inner: MsgContents::TeamAvailabilityUpdate,
    });

    return see_other_ok(Redirect::to(format!(
        "/tournaments/{tournament_id}/rounds/{}/availability/teams",
        round.seq
    )));
}
