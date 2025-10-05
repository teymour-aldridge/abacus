use std::collections::HashMap;

use diesel::{dsl::case_when, prelude::*};
use hypertext::prelude::*;
use hypertext::{Renderable, maud};
use itertools::Either;
use rocket::form::Form;
use rocket::futures::{SinkExt, StreamExt};
use rocket::response::Redirect;
use rocket::{FromForm, State, get, post};
use tokio::sync::broadcast::{Receiver, Sender};
use tokio::task::spawn_blocking;
use uuid::Uuid;

use crate::msg::{Msg, MsgContents};
use crate::state::DbPool;
use crate::template::Page;
use crate::tournaments::manage::sidebar::SidebarWrapper;
use crate::tournaments::rounds::TournamentRounds;
use crate::util_resp::{see_other_ok, success};
use crate::{
    auth::User,
    schema::{
        tournament_judge_availability, tournament_judge_stated_eligibility,
        tournament_judges, tournament_rounds,
    },
    state::Conn,
    tournaments::{Tournament, participants::Judge, rounds::Round},
    util_resp::{StandardResponse, err_not_found},
};

pub struct JudgeAvailabilityTable<'r> {
    tournament_id: &'r str,
    judges: &'r [Judge],
    rounds: &'r [Round],
    /// Vec<(judge_id, round_id, stated_availability, actual_eligibility)>
    judge_availability: &'r HashMap<(String, String), (bool, bool)>,
}

impl Renderable for JudgeAvailabilityTable<'_> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            table class="table table-striped table-bordered" id="participantsTable"
                  hx-ext="ws" hx-swap-oob="morphdom"
                  "ws-connect"=(format!("/tournaments/{}/rounds/{}/availability/judges?channel", self.tournament_id, self.rounds[0].seq)) {
                thead {
                    tr {
                        th scope="col" {
                            "#"
                        }
                        th scope="col" {
                            "Judge name"
                        }
                        @for round in self.rounds {
                            th scope="col" {
                                "Indicated availability for " (round.name)
                            }
                            th scope="col" {
                                "Actual availability for " (round.name)
                            }
                        }
                    }
                }
                tbody {
                    @for (i, judge) in self.judges.iter().enumerate() {
                        tr {
                            th scope="col" {
                                (i)
                            }
                            td {
                                (judge.name)
                            }
                            @for round in self.rounds {
                                @let (indicated, actual): (bool, bool) = self.judge_availability[&(judge.id.clone(), round.id.clone())];
                                td {
                                    @if indicated {
                                        input type="checkbox" checked disabled;
                                    } @else {
                                        input type="checkbox" disabled;
                                    }
                                }
                                td {
                                    form method="post"
                                         action=(format!("/tournaments/{}/rounds/{}/update_judge_availability", self.tournament_id, round.id)) {
                                        input type="text" hidden value=(judge.id) name="judge";
                                        div style="display: inline-block; position: relative;" {
                                            @if actual {
                                                input type="checkbox" checked name="available";
                                            } @else {
                                                input type="checkbox" name="available";
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

#[get("/tournaments/<tournament_id>/rounds/<round_seq>/availability/judges")]
pub async fn view_judge_availability(
    tournament_id: &str,
    round_seq: i64,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament_id = tournament_id.to_string();

    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let current_rounds = Round::of_seq(round_seq, &tournament.id, &mut *conn);
    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    if current_rounds.is_empty() {
        return err_not_found();
    }

    let judges_of_tournament = tournament_judges::table
        .filter(tournament_judges::tournament_id.eq(&tournament_id));

    let tournament_judges = judges_of_tournament
        .order_by(tournament_judges::id.asc())
        .load::<Judge>(&mut *conn)
        .unwrap();

    let judge_availability = judges_of_tournament
        .inner_join(
            tournament_rounds::table.on(tournament_rounds::tournament_id
                .eq(tournament_id.clone())
                .and(tournament_rounds::seq.eq(round_seq))),
        )
        .left_outer_join(
            tournament_judge_availability::table.on(
                tournament_judge_availability::judge_id
                    .eq(tournament_judges::id)
                    .and(
                        tournament_judge_availability::round_id
                            .eq(tournament_rounds::id),
                    ),
            ),
        )
        .left_outer_join(
            tournament_judge_stated_eligibility::table.on(
                tournament_judge_stated_eligibility::judge_id
                    .eq(tournament_judges::id)
                    .and(
                        tournament_judge_stated_eligibility::round_id
                            .eq(tournament_rounds::id),
                    ),
            ),
        )
        .select((
            tournament_judges::id,
            tournament_rounds::id,
            case_when(
                tournament_judge_stated_eligibility::available
                    .nullable()
                    .is_not_null(),
                tournament_judge_stated_eligibility::available
                    .nullable()
                    .assume_not_null(),
            )
            .otherwise(false),
            case_when(
                tournament_judge_availability::available
                    .nullable()
                    .is_not_null(),
                tournament_judge_availability::available
                    .nullable()
                    .assume_not_null(),
            )
            .otherwise(false),
        ))
        .load::<(String, String, bool, bool)>(&mut *conn)
        .unwrap()
        .into_iter()
        .map(|(judge, team, indicated, actual)| {
            ((judge, team), (indicated, actual))
        })
        .collect();

    let table = JudgeAvailabilityTable {
        tournament_id: &tournament_id,
        judges: &tournament_judges,
        rounds: &current_rounds.clone(),
        judge_availability: &judge_availability,
    };

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) {
                    h1 {
                        "Manage judge availability for rounds "
                        @for (i, round) in current_rounds.iter().enumerate() {
                            @if i > 0 {
                                ", "
                            }
                            (round.name)
                        }
                    }
                    (table)
                }
            })
            .render(),
    )
}

#[get(
    "/tournaments/<tournament_id>/rounds/<round_seq>/availability/judges?channel"
)]
pub async fn judge_availability_updates(
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
                            MsgContents::JudgeAvailabilityUpdate
                        )
                    {
                        msg
                    } else {
                        continue;
                    }
                };

                match msg.inner {
                    MsgContents::JudgeAvailabilityUpdate => {
                        let pool1 = pool.clone();
                        let tournament_id = tournament_id.clone();
                        let tournament = tournament.clone();

                        let table = spawn_blocking(move || {
                            let mut conn = pool1.get().unwrap();

                            let rounds = Round::of_seq(round_seq, &tournament.id, &mut *conn);

                            let judges_of_tournament = tournament_judges::table
                                .filter(tournament_judges::tournament_id.eq(&tournament_id));

                            let tournament_judges = judges_of_tournament
                                .order_by(tournament_judges::id.asc())
                                .load::<Judge>(&mut *conn)
                                .unwrap();

                            let judge_availability = judges_of_tournament
                                .inner_join(
                                    tournament_rounds::table.on(tournament_rounds::tournament_id
                                        .eq(tournament_id.clone())
                                        .and(tournament_rounds::seq.eq(round_seq))),
                                )
                                .left_outer_join(
                                    tournament_judge_availability::table.on(
                                        tournament_judge_availability::judge_id
                                            .eq(tournament_judges::id)
                                            .and(
                                                tournament_judge_availability::round_id
                                                    .eq(tournament_rounds::id),
                                            ),
                                    ),
                                )
                                .left_outer_join(
                                    tournament_judge_stated_eligibility::table.on(
                                        tournament_judge_stated_eligibility::judge_id
                                            .eq(tournament_judges::id)
                                            .and(
                                                tournament_judge_stated_eligibility::round_id
                                                    .eq(tournament_rounds::id),
                                            ),
                                    ),
                                )
                                .select((
                                    tournament_judges::id,
                                    tournament_rounds::id,
                                    case_when(
                                        tournament_judge_stated_eligibility::available
                                            .nullable()
                                            .is_null(),
                                        tournament_judge_stated_eligibility::available
                                            .nullable()
                                            .assume_not_null(),
                                    )
                                    .otherwise(false),
                                    case_when(
                                        tournament_judge_availability::available
                                            .nullable()
                                            .is_null(),
                                        tournament_judge_availability::available
                                            .nullable()
                                            .assume_not_null(),
                                    )
                                    .otherwise(false),
                                ))
                                .load::<(String, String, bool, bool)>(&mut *conn)
                                .unwrap()
                                .into_iter()
                                .map(|(judge, team, indicated, actual)| {
                                    ((judge, team), (indicated, actual))
                                })
                                .collect();

                            let table = JudgeAvailabilityTable {
                                tournament_id: &tournament_id,
                                judges: &tournament_judges,
                                rounds: &rounds.clone(),
                                judge_availability: &judge_availability,
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
pub struct JudgeAvailabilityForm {
    available: bool,
    judge: String,
}

#[post(
    "/tournaments/<tournament_id>/rounds/<round_id>/update_judge_availability",
    data = "<form>"
)]
pub async fn update_judge_availability(
    tournament_id: &str,
    round_id: &str,
    user: User<true>,
    tx: &rocket::State<Sender<Msg>>,
    mut conn: Conn<true>,
    form: Form<JudgeAvailabilityForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = match tournament_rounds::table
        .filter(
            tournament_rounds::tournament_id
                .eq(&tournament.id)
                .and(tournament_rounds::id.eq(round_id)),
        )
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(round) => round,
        None => return err_not_found(),
    };

    let judge = match tournament_judges::table
        .filter(
            tournament_judges::id
                .eq(&form.judge)
                .and(tournament_judges::tournament_id.eq(&tournament.id)),
        )
        .first::<Judge>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(judge) => judge,
        None => return err_not_found(),
    };

    let n = diesel::insert_into(tournament_judge_availability::table)
        .values((
            tournament_judge_availability::id.eq(Uuid::now_v7().to_string()),
            tournament_judge_availability::round_id.eq(&round.id),
            tournament_judge_availability::judge_id.eq(&judge.id),
            tournament_judge_availability::available.eq(!form.available),
        ))
        .on_conflict((
            tournament_judge_availability::round_id,
            tournament_judge_availability::judge_id,
        ))
        .do_update()
        .set(tournament_judge_availability::available.eq(!form.available))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    diesel::update(
        tournament_judge_availability::table.filter(
            tournament_judge_availability::judge_id.eq(&judge.id).and(
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
                                tournament_judge_availability::round_id
                                    .eq(tournament_rounds::id),
                            ),
                    ),
                ),
            ),
        ),
    )
    .set(tournament_judge_availability::available.eq(false))
    .execute(&mut *conn)
    .unwrap();

    let _ = tx.send(Msg {
        tournament,
        inner: MsgContents::JudgeAvailabilityUpdate,
    });

    return see_other_ok(Redirect::to(format!(
        "/tournaments/{tournament_id}/rounds/{}/availability/judges",
        round.seq
    )));
}
