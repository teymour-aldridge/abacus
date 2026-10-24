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
use crate::tournaments::manage::sidebar::SidebarWrapper;
use crate::tournaments::rounds::TournamentRounds;
use crate::{
    auth::User,
    schema::{judge_availability, judge_stated_eligibility, judges, rounds},
    state::Conn,
    tournaments::{
        Tournament,
        participants::{Judge, TournamentParticipants},
        rounds::Round,
    },
    util_resp::{StandardResponse, err_not_found},
};

pub struct JudgeAvailabilityTable<'r> {
    tournament_id: &'r str,
    judges: &'r [Judge],
    rounds: &'r [Round],
    /// Vec<(judge_id, round_id, stated_availability, actual_eligibility)>
    judge_availability: &'r HashMap<(String, String), (bool, bool)>,
    editing: bool,
    oob: bool,
}

impl Renderable for JudgeAvailabilityTable<'_> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            table class="table table-striped table-bordered" id=(if self.editing { "availability-judges-editor-table" } else { "availability-judges-read" })
                  hx-ext="ws" hx-swap-oob=[self.oob.then_some("outerHTML")]
                  "ws-connect"=(format!("/tournaments/{}/rounds/{}/availability/judges/ws?edit={}", self.tournament_id, self.rounds[0].seq, self.editing)) {
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
                                    @if self.editing {
                                    form method="post"
                                         action=(format!("/tournaments/{}/rounds/{}/update_judge_availability", self.tournament_id, round.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/rounds/{}/update_judge_availability", self.tournament_id, round.id), if format!("/tournaments/{}/rounds/{}/update_judge_availability", self.tournament_id, round.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" {
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
                                    } @else { input type="checkbox" disabled checked[actual]; }
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
    axum::extract::Query(query): axum::extract::Query<
        crate::tournaments::manage::workspace::WorkspaceQuery,
    >,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let access = crate::permission::TournamentAccess::resolve(
        &tournament_id,
        user.as_ref().map(|u| u.id.as_str()),
        &mut *conn,
    )?;
    if !access.is_superuser() {
        if query.editing() {
            return Err(crate::util_resp::FailureResponse::Unauthorized(()));
        }
        return crate::tournaments::manage::workspace::unavailable(
            &tournament,
            user,
            "Availability",
        );
    }
    let user = user.expect("superuser is authenticated");

    let current_rounds = Round::of_seq(round_seq, &tournament.id, &mut *conn);
    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    if current_rounds.is_empty() {
        return err_not_found();
    }

    let judges_of_tournament =
        judges::table.filter(judges::tournament_id.eq(&tournament_id));

    let judges = judges_of_tournament
        .order_by(judges::id.asc())
        .load::<Judge>(&mut *conn)
        .unwrap();

    let judge_availability = judges_of_tournament
        .inner_join(
            rounds::table.on(rounds::tournament_id
                .eq(tournament_id.clone())
                .and(rounds::seq.eq(round_seq))),
        )
        .left_outer_join(
            judge_availability::table.on(judge_availability::judge_id
                .eq(judges::id)
                .and(judge_availability::round_id.eq(rounds::id))),
        )
        .left_outer_join(
            judge_stated_eligibility::table.on(
                judge_stated_eligibility::judge_id
                    .eq(judges::id)
                    .and(judge_stated_eligibility::round_id.eq(rounds::id)),
            ),
        )
        .select((
            judges::id,
            rounds::id,
            case_when(
                judge_stated_eligibility::available.nullable().is_not_null(),
                judge_stated_eligibility::available
                    .nullable()
                    .assume_not_null(),
            )
            .otherwise(false),
            case_when(
                judge_availability::available.nullable().is_not_null(),
                judge_availability::available.nullable().assume_not_null(),
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
        judges: &judges,
        rounds: &current_rounds.clone(),
        judge_availability: &judge_availability,
        editing: true,
        oob: false,
    };

    let canonical = format!(
        "/tournaments/{tournament_id}/rounds/{round_seq}/availability/judges"
    );
    if !query.editing() {
        let table = JudgeAvailabilityTable {
            tournament_id: &tournament_id,
            judges: &judges,
            rounds: &current_rounds,
            judge_availability: &judge_availability,
            editing: false,
            oob: false,
        };
        return crate::tournaments::manage::workspace::read_page(
            &tournament,
            user,
            "Availability",
            &canonical,
            &access,
            table,
        );
    }
    crate::tournaments::manage::workspace::editor_page(
        query.form(),
        "Availability",
        &canonical,
        &tournament.clone(),
        user,
        maud! {
            div id="availability-judges-editor" {
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
                            form method="post" action=(format!("/tournaments/{tournament_id}/rounds/{}/availability/judges/all?check=in", round.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{tournament_id}/rounds/{}/availability/judges/all?check=in", round.id), if format!("/tournaments/{tournament_id}/rounds/{}/availability/judges/all?check=in", round.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" {
                                button type="submit" class="btn btn-primary" {
                                    "Check in all for " (round.name)
                                }
                            }
                        }
                        div class="col-md-auto" {
                            form method="post" action=(format!("/tournaments/{tournament_id}/rounds/{}/availability/judges/all?check=out", round.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{tournament_id}/rounds/{}/availability/judges/all?check=out", round.id), if format!("/tournaments/{tournament_id}/rounds/{}/availability/judges/all?check=out", round.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" {
                                button type="submit" class="btn btn-primary" {
                                    "Check out all for " (round.name)
                                }
                            }
                        }
                    }
                }
                (table)
            }
            }
        },
        false,
    )
}

pub async fn judge_availability_updates(
    Path((tournament_id, round_seq)): Path<(String, i64)>,
    ws: WebSocketUpgrade,
    Extension(pool): Extension<DbPool>,
    Extension(tx): Extension<Sender<Msg>>,
    axum::extract::Query(query): axum::extract::Query<
        crate::tournaments::manage::workspace::WorkspaceQuery,
    >,
    user: User<false>,
) -> impl IntoResponse {
    let pool: DbPool = pool.clone();

    let pool1 = pool.clone();
    let mut conn = match spawn_blocking(move || pool1.get()).await {
        Ok(Ok(conn)) => conn,
        _ => {
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR
                .into_response();
        }
    };

    let tournament = match Tournament::fetch(&tournament_id, &mut conn) {
        Ok(t) => t,
        Err(error) => return error.into_response(),
    };

    if let Err(error) = tournament.check_user_is_superuser(&user.id, &mut conn)
    {
        return error.into_response();
    }

    let rounds_exist = match diesel::dsl::select(diesel::dsl::exists(
        rounds::table.filter(
            rounds::tournament_id
                .eq(tournament_id.to_string())
                .and(rounds::seq.eq(round_seq)),
        ),
    ))
    .get_result::<bool>(&mut conn)
    {
        Ok(exists) => exists,
        Err(error) => {
            return crate::util_resp::FailureResponse::from(error)
                .into_response();
        }
    };

    if !rounds_exist {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    }

    ws.on_upgrade(move |socket| {
        handle_socket(
            socket,
            pool,
            tx,
            tournament_id,
            round_seq,
            tournament,
            user.id,
            query.editing(),
        )
    })
}

async fn handle_socket(
    mut socket: WebSocket,
    pool: DbPool,
    tx: Sender<Msg>,
    tournament_id: String,
    round_seq: i64,
    tournament: Tournament,
    viewer_id: String,
    editing: bool,
) {
    let mut rx = tx.subscribe();

    let mut recheck = tokio::time::interval(std::time::Duration::from_secs(15));
    loop {
        if let Some(removal) = super::stream_removal(
            super::stream_access(&pool, &tournament_id, &viewer_id).await,
        ) {
            let _ = socket.send(Message::Text(removal.into())).await;
            break;
        }
        let msg = tokio::select! {
            _ = recheck.tick() => continue,
            msg = rx.recv() => Either::Left(msg),
            msg = socket.recv() => Either::Right(msg),
        };

        if let Some(removal) = super::stream_removal(
            super::stream_access(&pool, &tournament_id, &viewer_id).await,
        ) {
            let _ = socket.send(Message::Text(removal.into())).await;
            break;
        }
        match msg {
            Either::Left(Ok(msg)) => {
                if msg.tournament.id == tournament.id
                    && matches!(msg.inner, MsgContents::JudgeAvailabilityUpdate)
                {
                    let pool1 = pool.clone();
                    let tournament_id = tournament_id.clone();

                    let recheck_tid = tournament_id.clone();
                    let loaded = spawn_blocking(move || -> Result<String, crate::util_resp::FailureResponse> {
                        let mut conn = pool1.get().map_err(|_|crate::util_resp::FailureResponse::ServerError(()))?;

                        let rounds=rounds::table.filter(rounds::tournament_id.eq(&tournament_id)).filter(rounds::seq.eq(round_seq)).load::<Round>(&mut *conn)?;
                        if rounds.is_empty(){return Err(crate::util_resp::FailureResponse::NotFound(()));}

                        let judges_of_tournament = judges::table
                            .filter(judges::tournament_id.eq(&tournament_id));

                        let judges = judges_of_tournament
                            .order_by(judges::id.asc())
                            .load::<Judge>(&mut *conn)
                            ?;

                        let judge_availability = judges_of_tournament
                            .inner_join(
                                rounds::table.on(rounds::tournament_id
                                    .eq(tournament_id.clone())
                                    .and(rounds::seq.eq(round_seq))),
                            )
                            .left_outer_join(
                                judge_availability::table.on(
                                    judge_availability::judge_id
                                        .eq(judges::id)
                                        .and(
                                            judge_availability::round_id
                                                .eq(rounds::id),
                                        ),
                                ),
                            )
                            .left_outer_join(
                                judge_stated_eligibility::table.on(
                                    judge_stated_eligibility::judge_id
                                        .eq(judges::id)
                                        .and(
                                            judge_stated_eligibility::round_id
                                                .eq(rounds::id),
                                        ),
                                ),
                            )
                            .select((
                                judges::id,
                                rounds::id,
                                case_when(
                                    judge_stated_eligibility::available
                                        .nullable()
                                        .is_not_null(),
                                    judge_stated_eligibility::available
                                        .nullable()
                                        .assume_not_null(),
                                )
                                .otherwise(false),
                                case_when(
                                    judge_availability::available
                                        .nullable()
                                        .is_not_null(),
                                    judge_availability::available
                                        .nullable()
                                        .assume_not_null(),
                                )
                                .otherwise(false),
                            ))
                            .load::<(String, String, bool, bool)>(&mut *conn)
                            ?
                            .into_iter()
                            .map(|(judge, team, indicated, actual)| {
                                ((judge, team), (indicated, actual))
                            })
                            .collect();

                        let table = JudgeAvailabilityTable {
                            tournament_id: &tournament_id,
                            judges: &judges,
                            rounds: &rounds.clone(),
                            judge_availability: &judge_availability,
                            editing,
                        oob: true,
                        };
                        Ok(table.render().into_inner())
                    })
                    .await;
                    let table_html = match loaded {
                        Ok(Ok(html)) => html,
                        _ => {
                            let _=socket.send(Message::Text(super::removal("Availability is temporarily unavailable. Refresh this page."))).await;
                            break;
                        }
                    };
                    if let Some(removal) = super::stream_removal(
                        super::stream_access(&pool, &recheck_tid, &viewer_id)
                            .await,
                    ) {
                        let _ = socket.send(Message::Text(removal)).await;
                        break;
                    }

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

#[derive(Deserialize, serde::Serialize)]
pub struct JudgeAvailabilityForm {
    #[serde(default)]
    #[allow(dead_code)]
    available: bool,
}

// Re-defining for correct checkbox handling
#[derive(Deserialize, serde::Serialize)]
pub struct JudgeAvailabilityFormRaw {
    judge: String,
    available: Option<String>,
}

pub async fn update_judge_availability(
    Path((tournament_id, round_id)): Path<(String, String)>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    Form(form): Form<JudgeAvailabilityFormRaw>,
) -> StandardResponse {
    let available = form.available.as_deref() == Some("on");

    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = match rounds::table
        .filter(
            rounds::tournament_id
                .eq(&tournament.id)
                .and(rounds::id.eq(&round_id)),
        )
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(round) => round,
        None => return err_not_found(),
    };

    if round.completed {
        return super::availability_error(
            modal_query,
            &tournament,
            user,
            round.seq,
            "judges",
            &format!(
                "/tournaments/{tournament_id}/rounds/{round_id}/update_judge_availability"
            ),
            &form,
            "This round is complete; its availability can no longer be changed.",
        );
    }

    if let Some(_prev) =
        round.preceeding_incomplete_round_of_lowest_seq(&mut *conn)
    {
        return super::availability_error(
            modal_query,
            &tournament,
            user,
            round.seq,
            "judges",
            &format!(
                "/tournaments/{tournament_id}/rounds/{round_id}/update_judge_availability"
            ),
            &form,
            "Complete preceding rounds before changing this round’s availability.",
        );
    }

    let judge = match judges::table
        .filter(
            judges::id
                .eq(&form.judge)
                .and(judges::tournament_id.eq(&tournament.id)),
        )
        .first::<Judge>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(judge) => judge,
        None => return err_not_found(),
    };

    let n = diesel::insert_into(judge_availability::table)
        .values((
            judge_availability::id.eq(Uuid::now_v7().to_string()),
            judge_availability::round_id.eq(&round.id),
            judge_availability::judge_id.eq(&judge.id),
            judge_availability::available.eq(!available),
            judge_availability::tournament_id.eq(&tournament.id),
        ))
        .on_conflict((
            judge_availability::round_id,
            judge_availability::judge_id,
        ))
        .do_update()
        .set(judge_availability::available.eq(!available))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    if !available {
        diesel::update(
            judge_availability::table.filter(
                judge_availability::judge_id.eq(&judge.id).and(
                    diesel::dsl::exists(
                        rounds::table.filter(
                            rounds::tournament_id
                                .eq(&tournament.id)
                                .and(
                                    rounds::seq
                                        .eq(round.seq)
                                        .and(rounds::id.ne(&round.id)),
                                )
                                .and(
                                    judge_availability::round_id.eq(rounds::id),
                                ),
                        ),
                    ),
                ),
            ),
        )
        .set(judge_availability::available.eq(false))
        .execute(&mut *conn)
        .unwrap();
    }

    let _ = tx.send(Msg {
        tournament,
        inner: MsgContents::JudgeAvailabilityUpdate,
    });

    return crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!(
            "/tournaments/{tournament_id}/rounds/{}/availability/judges",
            round.seq
        )),
    );
}

#[derive(Deserialize, serde::Serialize)]
pub struct CheckQuery {
    check: String,
}

pub async fn update_judge_availability_for_all(
    Path((tournament_id, round_id)): Path<(String, String)>,
    Query(query): Query<CheckQuery>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    Extension(tx): Extension<Sender<Msg>>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = Round::fetch(&tournament_id, &round_id, &mut *conn)?;

    if round.completed {
        return super::availability_error(
            modal_query,
            &tournament,
            user,
            round.seq,
            "judges",
            &format!(
                "/tournaments/{tournament_id}/rounds/{round_id}/availability/judges/all?check={}",
                query.check
            ),
            &query,
            "This round is complete; its availability can no longer be changed.",
        );
    }
    if let Some(_prev) =
        round.preceeding_incomplete_round_of_lowest_seq(&mut *conn)
    {
        return super::availability_error(
            modal_query,
            &tournament,
            user,
            round.seq,
            "judges",
            &format!(
                "/tournaments/{tournament_id}/rounds/{round_id}/availability/judges/all?check={}",
                query.check
            ),
            &query,
            "Complete preceding rounds before changing this round’s availability.",
        );
    }
    match query.check.as_str() {
        "in" => {
            // todo: there is a more efficient way to do this
            let participants =
                TournamentParticipants::load(&tournament.id, &mut *conn);

            for (_, judge) in participants.judges {
                let n = diesel::insert_into(judge_availability::table)
                    .values((
                        judge_availability::id.eq(Uuid::now_v7().to_string()),
                        judge_availability::round_id.eq(&round.id),
                        judge_availability::judge_id.eq(&judge.id),
                        judge_availability::available.eq(true),
                        judge_availability::tournament_id
                            .eq(tournament.id.clone()),
                    ))
                    .on_conflict((
                        judge_availability::round_id,
                        judge_availability::judge_id,
                    ))
                    .do_update()
                    .set(judge_availability::available.eq(true))
                    .execute(&mut *conn)
                    .unwrap();
                assert_eq!(n, 1);
            }

            diesel::update(
                judge_availability::table.filter(
                    judge_availability::round_id.eq_any(
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
            .set(judge_availability::available.eq(false))
            .execute(&mut *conn)
            .unwrap();
        }
        "out" => {
            diesel::update(
                judge_availability::table
                    .filter(judge_availability::round_id.eq(&round.id)),
            )
            .set(judge_availability::available.eq(false))
            .execute(&mut *conn)
            .unwrap();
        }
        _ => {
            // todo: proper page (but should not be encountered in standard use)
            return super::availability_error(
                modal_query,
                &tournament,
                user,
                round.seq,
                "judges",
                &format!(
                    "/tournaments/{tournament_id}/rounds/{round_id}/availability/judges/all?check={}",
                    query.check
                ),
                &query,
                "Choose check in or check out.",
            );
        }
    }

    let _ = tx.send(Msg {
        tournament,
        inner: MsgContents::JudgeAvailabilityUpdate,
    });

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!(
            "/tournaments/{}/rounds/{}/availability/judges",
            tournament_id, round.seq
        )),
    )
}
