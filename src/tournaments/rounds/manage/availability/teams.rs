use std::collections::HashMap;

use axum::{
    Extension, Form,
    extract::{Path, Query, WebSocketUpgrade, ws},
    response::{IntoResponse, Redirect},
};
use diesel::{
    connection::LoadConnection, dsl::case_when, prelude::*, sqlite::Sqlite,
};
use futures::{sink::SinkExt, stream::StreamExt};
use hypertext::prelude::*;
use serde::Deserialize;
use tokio::{
    sync::broadcast::{Receiver, Sender},
    task::spawn_blocking,
};
use uuid::Uuid;

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{rounds, team_availability, teams},
    state::{Conn, DbPool},
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        participants::TournamentParticipants,
        rounds::{Round, TournamentRounds},
        teams::Team,
    },
    util_resp::{
        StandardResponse, bad_request, err_not_found, see_other_ok, success,
    },
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
                  "ws-connect"=(format!("/tournaments/{}/rounds/{}/availability/teams/ws", self.tournament_id, self.rounds[0].seq)) {
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
    teams::table
        .filter(teams::tournament_id.eq(tournament_id))
        .order_by(teams::id.asc())
        .inner_join(rounds::table.on(rounds::seq.eq(round_seq as i64)))
        .left_outer_join(team_availability::table)
        .filter(team_availability::round_id.eq(rounds::id))
        .select((
            teams::id,
            rounds::id,
            case_when(
                team_availability::available.nullable().is_not_null(),
                team_availability::available.nullable().assume_not_null(),
            )
            .otherwise(false),
        ))
        .load::<(String, String, bool)>(&mut *conn)
        .unwrap()
        .into_iter()
        .map(|(team, round, bool)| ((team, round), bool))
        .collect::<HashMap<_, _>>()
}

pub async fn view_team_availability(
    Path((tournament_id, round_seq)): Path<(String, usize)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let current_rounds = Round::current_rounds(&tournament.id, &mut *conn);

    let teams = teams::table
        .filter(teams::tournament_id.eq(&tournament_id))
        .order_by(teams::id.asc())
        .load::<Team>(&mut *conn)
        .unwrap();

    let teams_and_availability =
        get_teams_and_availability(&tournament_id, round_seq, &mut *conn);

    success(
        Page::new()
            .extra_head(
                maud! {
                    script src="https://cdn.jsdelivr.net/npm/htmx-ext-ws@2.0.2" crossorigin="anonymous" {
                    }
                }
            )
            .user(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds.clone())
            .body(maud! {
                SidebarWrapper rounds=(&rounds) tournament=(&tournament) active_page=(Some(crate::tournaments::manage::sidebar::SidebarPage::Setup)) selected_seq=(Some(round_seq as i64)) {
                    h1 {
                        "Manage availabilities for rounds "
                        @for (i, round) in current_rounds.iter().enumerate() {
                            @if i > 0 {
                                ", "
                            }
                            (round.name)
                        }
                    }
                    div class = "row mt-3 mb-3" {
                        @for round in &current_rounds {
                            div class="col-md-auto" {
                                form method="post" action=(format!("/tournaments/{tournament_id}/rounds/{}/availability/teams/all?check=in", round.id)) {
                                    button type="submit" class="btn btn-primary" {
                                        "Check in all for " (round.name)
                                    }
                                }
                            }
                            div class="col-md-auto" {
                                form method="post" action=(format!("/tournaments/{tournament_id}/rounds/{}/availability/teams/all?check=out", round.id)) {
                                    button type="submit" class="btn btn-primary" {
                                        "Check out all for " (round.name)
                                    }
                                }
                            }
                        }
                    }
                    ManageAvailabilityTable
                        tournament_id=(&tournament_id)
                        rounds=(&current_rounds)
                        teams=(&teams)
                        teams_and_availability=(&teams_and_availability);
                }
            })
            .render(),
    )
}

pub async fn team_availability_updates(
    ws: WebSocketUpgrade,
    Path((tournament_id, round_seq)): Path<(String, i64)>,
    Extension(pool): Extension<DbPool>,
    Extension(tx): Extension<Sender<Msg>>,
    user: User<false>,
) -> impl IntoResponse {
    let pool: DbPool = pool.clone();

    let pool1 = pool.clone();
    let has_rounds_result = spawn_blocking(move || {
        let mut conn = pool1.get().unwrap();

        let tournament = Tournament::fetch(&tournament_id, &mut conn).ok()?;
        tournament
            .check_user_is_superuser(&user.id, &mut conn)
            .ok()?;

        let rounds = diesel::dsl::select(diesel::dsl::exists(
            rounds::table.filter(
                rounds::tournament_id
                    .eq(tournament_id.to_string())
                    .and(rounds::seq.eq(round_seq)),
            ),
        ))
        .get_result::<bool>(&mut conn)
        .unwrap();

        Some((tournament, rounds))
    })
    .await
    .unwrap();

    let (tournament, rounds_exist) = match has_rounds_result {
        Some(res) => res,
        None => {
            return (axum::http::StatusCode::FORBIDDEN, "Access denied")
                .into_response();
        }
    };

    if !rounds_exist {
        return (axum::http::StatusCode::NOT_FOUND, "Round not found")
            .into_response();
    }

    let rx = tx.subscribe();
    let tournament_id = tournament.id.clone();

    ws.on_upgrade(move |socket| {
        handle_socket(socket, rx, pool, tournament_id, round_seq)
    })
}

async fn handle_socket(
    socket: ws::WebSocket,
    mut rx: Receiver<Msg>,
    pool: DbPool,
    tournament_id: String,
    round_seq: i64,
) {
    let (mut sender, mut receiver) = socket.split();

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if msg.tournament.id == tournament_id
                && matches!(msg.inner, MsgContents::TeamAvailabilityUpdate)
            {
                let pool1 = pool.clone();
                let tournament_id = tournament_id.clone();

                let table_html = spawn_blocking(move || {
                    let mut conn = pool1.get().unwrap();

                    let rounds = rounds::table
                        .filter(
                            rounds::tournament_id
                                .eq(&tournament_id)
                                .and(rounds::seq.eq(round_seq as i64)),
                        )
                        .order_by(rounds::id.asc())
                        .load::<Round>(&mut *conn)
                        .unwrap();

                    let teams = teams::table
                        .filter(teams::tournament_id.eq(&tournament_id))
                        .order_by(teams::id.asc())
                        .load::<Team>(&mut *conn)
                        .unwrap();

                    let teams_and_availability = get_teams_and_availability(
                        &tournament_id,
                        round_seq as usize,
                        &mut *conn,
                    );

                    let table = ManageAvailabilityTable {
                        tournament_id: &tournament_id,
                        rounds: &rounds,
                        teams: &teams,
                        teams_and_availability: &teams_and_availability,
                    };

                    table.render().into_inner()
                })
                .await
                .unwrap();

                if sender
                    .send(ws::Message::Text(table_html.into()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(_msg)) = receiver.next().await {
            // client logic if any, currently we just listen for close
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}

#[derive(Deserialize)]
pub struct UpdateTeamAvailabilityForm {
    available: Option<String>,
    team: String,
}

#[derive(Deserialize)]
pub struct CheckQuery {
    check: String,
}

pub async fn update_eligibility_for_all(
    Path((tournament_id, round_id)): Path<(String, String)>,
    Query(query): Query<CheckQuery>,
    user: User<true>,
    mut conn: Conn<true>,
    Extension(tx): Extension<Sender<Msg>>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = Round::fetch(&round_id, &mut *conn)?;

    if round.completed {
        return bad_request(maud! {
            "Note: the current round has been completed, so judge availability"
            " can no longer be updated!"
        }.render());
    }

    if let Some(prev) = round.find_first_preceding_incomplete_round(&mut *conn)
    {
        return bad_request(
            maud! {
                "Note: " (prev.name) " should be marked as complete first (it "
                "precedes the current round)"
            }
            .render(),
        );
    }

    match query.check.as_str() {
        "in" => {
            // todo: there is a more efficient way to do this
            let participants =
                TournamentParticipants::load(&tournament.id, &mut *conn);

            for (_, team) in participants.teams {
                let n = diesel::insert_into(team_availability::table)
                    .values((
                        team_availability::id.eq(Uuid::now_v7().to_string()),
                        team_availability::round_id.eq(&round.id),
                        team_availability::team_id.eq(&team.id),
                        team_availability::available.eq(true),
                        team_availability::tournament_id
                            .eq(tournament.id.clone()),
                    ))
                    .on_conflict((
                        team_availability::round_id,
                        team_availability::team_id,
                    ))
                    .do_update()
                    .set(team_availability::available.eq(true))
                    .execute(&mut *conn)
                    .unwrap();
                assert_eq!(n, 1);
            }

            diesel::update(
                team_availability::table.filter(
                    team_availability::round_id.eq_any(
                        rounds::table
                            .filter(
                                rounds::tournament_id
                                    .eq(&round.tournament_id)
                                    .and(rounds::seq.eq(round.seq))
                                    // don't want to mark unavailable for
                                    // current round
                                    .and(rounds::id.ne(&round.id)),
                            )
                            .select(rounds::id),
                    ),
                ),
            )
            .set(team_availability::available.eq(false))
            .execute(&mut *conn)
            .unwrap();
        }
        "out" => {
            diesel::update(
                team_availability::table
                    .filter(team_availability::round_id.eq(&round.id)),
            )
            .set(team_availability::available.eq(false))
            .execute(&mut *conn)
            .unwrap();
        }
        _ => {
            // todo: proper page (but should not be encountered in standard use)
            return bad_request(
                maud! {
                    "Invalid check-in option."
                }
                .render(),
            );
        }
    }

    let _ = tx.send(Msg {
        tournament,
        inner: MsgContents::TeamAvailabilityUpdate,
    });

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/rounds/{}/availability/teams",
        tournament_id, round.seq
    )))
}

pub async fn update_team_eligibility(
    Path((tournament_id, round_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Extension(tx): Extension<Sender<Msg>>,
    Form(form): Form<UpdateTeamAvailabilityForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let available_bool = form.available.as_deref() == Some("true");

    let team = match teams::table
        .filter(
            teams::tournament_id
                .eq(&tournament_id)
                .and(teams::id.eq(&form.team)),
        )
        .first::<Team>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(team) => team,
        None => return err_not_found(),
    };

    let round = match rounds::table
        .filter(
            rounds::id
                .eq(&round_id)
                .and(rounds::tournament_id.eq(&tournament_id)),
        )
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(round) => round,
        None => return err_not_found(),
    };

    if round.completed {
        return bad_request(maud! {
            "Note: the current round has been completed, so judge availability"
            " can no longer be updated!"
        }.render());
    }

    if let Some(prev) = round.find_first_preceding_incomplete_round(&mut *conn)
    {
        return bad_request(
            maud! {
                "Note: " (prev.name) " should be marked as complete first (it "
                "precedes the current round)"
            }
            .render(),
        );
    }

    // TODO: check that a team can't be allocated to multiple concurrent rounds

    let n = diesel::insert_into(team_availability::table)
        .values((
            team_availability::id.eq(Uuid::now_v7().to_string()),
            team_availability::round_id.eq(&round.id),
            team_availability::team_id.eq(&team.id),
            team_availability::available.eq(available_bool),
        ))
        .on_conflict((team_availability::round_id, team_availability::team_id))
        .do_update()
        .set(team_availability::available.eq(available_bool))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    diesel::update(
        team_availability::table.filter(
            team_availability::team_id
                .eq(&team.id)
                .and(diesel::dsl::exists(
                    rounds::table.filter(
                        rounds::tournament_id
                            .eq(&tournament.id)
                            .and(
                                rounds::seq
                                    .eq(round.seq)
                                    .and(rounds::id.ne(&round.id)),
                            )
                            .and(team_availability::round_id.eq(rounds::id)),
                    ),
                )),
        ),
    )
    .set(team_availability::available.eq(false))
    .execute(&mut *conn)
    .unwrap();

    debug_assert!(
        // check that a team is only ever assigned to one round (where there
        // are multiple concurrent rounds)
        team_availability::table
            .filter(
                team_availability::team_id.eq(&team.id).and(
                    team_availability::round_id.eq_any(
                        rounds::table
                            .filter(rounds::seq.eq(round.seq))
                            .select(rounds::id)
                    )
                )
            )
            .count()
            .get_result::<i64>(&mut *conn)
            .unwrap()
            <= 1
    );

    let _ = tx.send(Msg {
        tournament,
        inner: MsgContents::TeamAvailabilityUpdate,
    });

    return see_other_ok(Redirect::to(&format!(
        "/tournaments/{tournament_id}/rounds/{}/availability/teams",
        round.seq
    )));
}
