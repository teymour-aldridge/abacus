use std::collections::HashMap;

use axum::{
    Form,
    extract::{
        Extension, Path, Query,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{IntoResponse, Redirect},
};
use diesel::{dsl::case_when, prelude::*};

use hypertext::prelude::*;
use hypertext::{Renderable, maud};
use itertools::Either;
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
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
    tournaments::{
        Tournament,
        participants::{Judge, TournamentParticipants},
        rounds::Round,
    },
    util_resp::{StandardResponse, bad_request, err_not_found},
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
                  "ws-connect"=(format!("/tournaments/{}/rounds/{}/availability/judges/ws", self.tournament_id, self.rounds[0].seq)) {
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

pub async fn view_judge_availability(
    Path((tournament_id, round_seq)): Path<(String, i64)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
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

    let tournament_id_clone = tournament_id.clone();
    let table = JudgeAvailabilityTable {
        tournament_id: &tournament_id_clone,
        judges: &tournament_judges,
        rounds: &current_rounds.clone(),
        judge_availability: &judge_availability,
    };

    let nav_current_rounds = Round::current_rounds(&tournament_id, &mut *conn);

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .current_rounds(nav_current_rounds)
            .extra_head(
                maud! {
                    script src="https://cdn.jsdelivr.net/npm/htmx-ext-ws@2.0.2" crossorigin="anonymous" {
                    }
                }
                .render()
                .into_inner()
            )
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) active_page=(Some(crate::tournaments::manage::sidebar::SidebarPage::Setup)) selected_seq=(Some(round_seq)) {
                    h1 {
                        "Manage judge availability for rounds "
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
                                form method="post" action=(format!("/tournaments/{tournament_id}/rounds/{}/availability/judges/all?check=in", round.id)) {
                                    button type="submit" class="btn btn-primary" {
                                        "Check in all for " (round.name)
                                    }
                                }
                            }
                            div class="col-md-auto" {
                                form method="post" action=(format!("/tournaments/{tournament_id}/rounds/{}/availability/judges/all?check=out", round.id)) {
                                    button type="submit" class="btn btn-primary" {
                                        "Check out all for " (round.name)
                                    }
                                }
                            }
                        }
                    }
                    (table)
                }
            })
            .render(),
    )
}

pub async fn judge_availability_updates(
    Path((tournament_id, round_seq)): Path<(String, i64)>,
    ws: WebSocketUpgrade,
    Extension(pool): Extension<DbPool>,
    Extension(tx): Extension<Sender<Msg>>,
    user: User<false>,
) -> impl IntoResponse {
    let pool: DbPool = pool.clone();

    let pool1 = pool.clone();
    let mut conn = spawn_blocking(move || pool1.get().unwrap()).await.unwrap();

    let tournament = match Tournament::fetch(&tournament_id, &mut conn) {
        Ok(t) => t,
        Err(_) => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };

    if tournament
        .check_user_is_superuser(&user.id, &mut conn)
        .is_err()
    {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    let rounds_exist = diesel::dsl::select(diesel::dsl::exists(
        tournament_rounds::table.filter(
            tournament_rounds::tournament_id
                .eq(tournament_id.to_string())
                .and(tournament_rounds::seq.eq(round_seq)),
        ),
    ))
    .get_result::<bool>(&mut conn)
    .unwrap();

    if !rounds_exist {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    }

    ws.on_upgrade(move |socket| {
        handle_socket(socket, pool, tx, tournament_id, round_seq, tournament)
    })
}

async fn handle_socket(
    mut socket: WebSocket,
    pool: DbPool,
    tx: Sender<Msg>,
    tournament_id: String,
    round_seq: i64,
    tournament: Tournament,
) {
    let mut rx = tx.subscribe();

    loop {
        let msg = tokio::select! {
            msg = rx.recv() => Either::Left(msg),
            msg = socket.recv() => Either::Right(msg),
        };

        match msg {
            Either::Left(Ok(msg)) => {
                if msg.tournament.id == tournament.id
                    && matches!(msg.inner, MsgContents::JudgeAvailabilityUpdate)
                {
                    let pool1 = pool.clone();
                    let tournament_id = tournament_id.clone();
                    let tournament = tournament.clone();

                    let table_html = spawn_blocking(move || {
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

                    if socket.send(Message::Text(table_html)).await.is_err() {
                        break;
                    }
                }
            }
            Either::Right(Some(Ok(Message::Close(_))))
            | Either::Right(None) => {
                break;
            }
            Either::Right(Some(Err(_))) => {
                break;
            }
            _ => {}
        }
    }
}

#[derive(Deserialize)]
pub struct JudgeAvailabilityForm {
    #[serde(default)]
    #[allow(dead_code)]
    available: bool,
}

// Re-defining for correct checkbox handling
#[derive(Deserialize)]
pub struct JudgeAvailabilityFormRaw {
    judge: String,
    available: Option<String>,
}

pub async fn update_judge_availability(
    Path((tournament_id, round_id)): Path<(String, String)>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    Form(form): Form<JudgeAvailabilityFormRaw>,
) -> StandardResponse {
    let available = form.available.as_deref() == Some("on");

    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = match tournament_rounds::table
        .filter(
            tournament_rounds::tournament_id
                .eq(&tournament.id)
                .and(tournament_rounds::id.eq(&round_id)),
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
            tournament_judge_availability::available.eq(!available),
        ))
        .on_conflict((
            tournament_judge_availability::round_id,
            tournament_judge_availability::judge_id,
        ))
        .do_update()
        .set(tournament_judge_availability::available.eq(!available))
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

    return see_other_ok(Redirect::to(&format!(
        "/tournaments/{tournament_id}/rounds/{}/availability/judges",
        round.seq
    )));
}

#[derive(Deserialize)]
pub struct CheckQuery {
    check: String,
}

pub async fn update_judge_availability_for_all(
    Path((tournament_id, round_id)): Path<(String, String)>,
    Query(query): Query<CheckQuery>,
    user: User<true>,
    mut conn: Conn<true>,
    Extension(tx): Extension<Sender<Msg>>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = Round::fetch(&round_id, &mut *conn)?;

    match query.check.as_str() {
        "in" => {
            // todo: there is a more efficient way to do this
            let participants =
                TournamentParticipants::load(&tournament.id, &mut *conn);

            for (_, judge) in participants.judges {
                let n =
                    diesel::insert_into(tournament_judge_availability::table)
                        .values((
                            tournament_judge_availability::id
                                .eq(Uuid::now_v7().to_string()),
                            tournament_judge_availability::round_id
                                .eq(&round.id),
                            tournament_judge_availability::judge_id
                                .eq(&judge.id),
                            tournament_judge_availability::available.eq(true),
                        ))
                        .on_conflict((
                            tournament_judge_availability::round_id,
                            tournament_judge_availability::judge_id,
                        ))
                        .do_update()
                        .set(tournament_judge_availability::available.eq(true))
                        .execute(&mut *conn)
                        .unwrap();
                assert_eq!(n, 1);
            }

            diesel::update(
                tournament_judge_availability::table.filter(
                    tournament_judge_availability::round_id.eq_any(
                        tournament_rounds::table
                            .filter(
                                tournament_rounds::tournament_id
                                    .eq(&round.tournament_id)
                                    .and(tournament_rounds::seq.eq(round.seq))
                                    // don't want to mark unavailable for
                                    // current round
                                    .and(tournament_rounds::id.ne(&round.id)),
                            )
                            .select(tournament_rounds::id),
                    ),
                ),
            )
            .set(tournament_judge_availability::available.eq(false))
            .execute(&mut *conn)
            .unwrap();
        }
        "out" => {
            diesel::update(
                tournament_judge_availability::table.filter(
                    tournament_judge_availability::round_id.eq(&round.id),
                ),
            )
            .set(tournament_judge_availability::available.eq(false))
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
        inner: MsgContents::JudgeAvailabilityUpdate,
    });

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/rounds/{}/availability/judges",
        tournament_id, round.seq
    )))
}
