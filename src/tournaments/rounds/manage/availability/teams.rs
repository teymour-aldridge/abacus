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
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        participants::TournamentParticipants,
        rounds::{Round, TournamentRounds},
        teams::Team,
    },
    util_resp::{StandardResponse, err_not_found},
};

pub struct ManageAvailabilityTable<'r> {
    tournament_id: &'r str,
    rounds: &'r [Round],
    teams: &'r [Team],
    teams_and_availability: &'r HashMap<(String, String), bool>,
    editing: bool,
    oob: bool,
}

impl<'r> Renderable for ManageAvailabilityTable<'r> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            table class="table table-striped table-bordered" id=(if self.editing { "availability-teams-editor-table" } else { "availability-teams-read" })
                  hx-ext="ws" hx-swap-oob=[self.oob.then_some("outerHTML")]
                  "ws-connect"=(format!("/tournaments/{}/rounds/{}/availability/teams/ws?edit={}", self.tournament_id, self.rounds[0].seq, self.editing)) {
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
                                    @if self.editing {
                                    form method="post"
                                         action=(format!("/tournaments/{}/rounds/{}/update_team_eligibility", self.tournament_id, round.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/rounds/{}/update_team_eligibility", self.tournament_id, round.id), if format!("/tournaments/{}/rounds/{}/update_team_eligibility", self.tournament_id, round.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" {
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
                                    } @else { input type="checkbox" disabled checked[*available]; }
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
) -> Result<
    HashMap<(std::string::String, std::string::String), bool>,
    crate::util_resp::FailureResponse,
> {
    Ok(teams::table
        .filter(teams::tournament_id.eq(tournament_id))
        .order_by(teams::id.asc())
        .inner_join(
            rounds::table.on(rounds::seq
                .eq(round_seq as i64)
                .and(rounds::tournament_id.eq(tournament_id.to_owned()))),
        )
        .left_outer_join(
            team_availability::table.on(team_availability::team_id
                .eq(teams::id)
                .and(team_availability::round_id.eq(rounds::id))),
        )
        .select((
            teams::id,
            rounds::id,
            case_when(
                team_availability::available.nullable().is_not_null(),
                team_availability::available.nullable().assume_not_null(),
            )
            .otherwise(false),
        ))
        .load::<(String, String, bool)>(&mut *conn)?
        .into_iter()
        .map(|(team, round, bool)| ((team, round), bool))
        .collect::<HashMap<_, _>>())
}

pub async fn view_team_availability(
    Path((tournament_id, round_seq)): Path<(String, usize)>,
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

    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let selected_rounds =
        Round::of_seq(round_seq as i64, &tournament.id, &mut *conn);

    if selected_rounds.is_empty() {
        return err_not_found();
    }

    let teams = teams::table
        .filter(teams::tournament_id.eq(&tournament_id))
        .order_by(teams::id.asc())
        .load::<Team>(&mut *conn)
        .unwrap();

    let teams_and_availability =
        get_teams_and_availability(&tournament_id, round_seq, &mut *conn)?;

    let canonical = format!(
        "/tournaments/{tournament_id}/rounds/{round_seq}/availability/teams"
    );
    if !query.editing() {
        let table = ManageAvailabilityTable {
            tournament_id: &tournament_id,
            rounds: &selected_rounds,
            teams: &teams,
            teams_and_availability: &teams_and_availability,
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
            div id="availability-teams-editor" {
            SidebarWrapper rounds=(&rounds) tournament=(&tournament) active_page=(Some(crate::tournaments::manage::sidebar::SidebarPage::Setup)) selected_seq=(Some(round_seq as i64)) {
                h1 {
                    "Manage availabilities for rounds "
                    @for (i, round) in selected_rounds.iter().enumerate() {
                        @if i > 0 {
                            ", "
                        }
                        (round.name)
                    }
                }
                div class = "row mt-3 mb-3" {
                    @for round in &selected_rounds {
                        div class="col-md-auto" {
                            form method="post" action=(format!("/tournaments/{tournament_id}/rounds/{}/availability/teams/all?check=in", round.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{tournament_id}/rounds/{}/availability/teams/all?check=in", round.id), if format!("/tournaments/{tournament_id}/rounds/{}/availability/teams/all?check=in", round.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" {
                                button type="submit" class="btn btn-primary" {
                                    "Check in all for " (round.name)
                                }
                            }
                        }
                        div class="col-md-auto" {
                            form method="post" action=(format!("/tournaments/{tournament_id}/rounds/{}/availability/teams/all?check=out", round.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{tournament_id}/rounds/{}/availability/teams/all?check=out", round.id), if format!("/tournaments/{tournament_id}/rounds/{}/availability/teams/all?check=out", round.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" {
                                button type="submit" class="btn btn-primary" {
                                    "Check out all for " (round.name)
                                }
                            }
                        }
                    }
                }
                ManageAvailabilityTable
                    tournament_id=(&tournament_id)
                    rounds=(&selected_rounds)
                    teams=(&teams)
                    teams_and_availability=(&teams_and_availability) editing=(true) oob=(false);
            }
            }
        },
        false,
    )
}

pub async fn team_availability_updates(
    ws: WebSocketUpgrade,
    Path((tournament_id, round_seq)): Path<(String, i64)>,
    Extension(pool): Extension<DbPool>,
    Extension(tx): Extension<Sender<Msg>>,
    axum::extract::Query(query): axum::extract::Query<
        crate::tournaments::manage::workspace::WorkspaceQuery,
    >,
    user: User<false>,
) -> impl IntoResponse {
    let pool: DbPool = pool.clone();

    let viewer_id = user.id.clone();
    let pool1 = pool.clone();
    let has_rounds_result = spawn_blocking(move || {
        let mut conn = pool1
            .get()
            .map_err(|_| crate::util_resp::FailureResponse::ServerError(()))?;

        let tournament = Tournament::fetch(&tournament_id, &mut conn)?;
        tournament.check_user_is_superuser(&user.id, &mut conn)?;

        let rounds = diesel::dsl::select(diesel::dsl::exists(
            rounds::table.filter(
                rounds::tournament_id
                    .eq(tournament_id.to_string())
                    .and(rounds::seq.eq(round_seq)),
            ),
        ))
        .get_result::<bool>(&mut conn)?;

        Ok::<_, crate::util_resp::FailureResponse>((tournament, rounds))
    })
    .await;

    let (tournament, rounds_exist) = match has_rounds_result {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => return error.into_response(),
        Err(_) => {
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR
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
        handle_socket(
            socket,
            rx,
            pool,
            tournament_id,
            round_seq,
            viewer_id,
            query.editing(),
        )
    })
}

async fn handle_socket(
    socket: ws::WebSocket,
    mut rx: Receiver<Msg>,
    pool: DbPool,
    tournament_id: String,
    round_seq: i64,
    viewer_id: String,
    editing: bool,
) {
    let (mut sender, mut receiver) = socket.split();

    let mut send_task = tokio::spawn(async move {
        let mut recheck =
            tokio::time::interval(std::time::Duration::from_secs(15));
        loop {
            let event = tokio::select! {
                event = rx.recv() => match event { Ok(event) => Some(event), Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => None, Err(_) => break },
                _ = recheck.tick() => None,
            };
            if let Some(removal) = super::stream_removal(
                super::stream_access(&pool, &tournament_id, &viewer_id).await,
            ) {
                let _ = sender.send(ws::Message::Text(removal.into())).await;
                break;
            }
            let Some(msg) = event else { continue };
            if msg.tournament.id == tournament_id
                && matches!(msg.inner, MsgContents::TeamAvailabilityUpdate)
            {
                let pool1 = pool.clone();
                let tournament_id = tournament_id.clone();

                let recheck_tid = tournament_id.clone();
                let loaded = spawn_blocking(move || -> Result<String, crate::util_resp::FailureResponse> {
                    let mut conn = pool1.get().map_err(|_|crate::util_resp::FailureResponse::ServerError(()))?;

                    let rounds = rounds::table
                        .filter(
                            rounds::tournament_id
                                .eq(&tournament_id)
                                .and(rounds::seq.eq(round_seq as i64)),
                        )
                        .order_by(rounds::id.asc())
                        .load::<Round>(&mut *conn)
                        ?;

                    if rounds.is_empty(){return Err(crate::util_resp::FailureResponse::NotFound(()));}
                    let teams = teams::table
                        .filter(teams::tournament_id.eq(&tournament_id))
                        .order_by(teams::id.asc())
                        .load::<Team>(&mut *conn)
                        ?;

                    let teams_and_availability = get_teams_and_availability(
                        &tournament_id,
                        round_seq as usize,
                        &mut *conn,
                    )?;

                    let table = ManageAvailabilityTable {
                        tournament_id: &tournament_id,
                        rounds: &rounds,
                        teams: &teams,
                        teams_and_availability: &teams_and_availability,
                        editing,
                        oob: true,
                    };

                    Ok(table.render().into_inner())
                })
                .await;
                let table_html = match loaded {
                    Ok(Ok(html)) => html,
                    _ => {
                        let _=sender.send(ws::Message::Text(super::removal("Availability is temporarily unavailable. Refresh this page."))).await;
                        break;
                    }
                };
                if let Some(removal) = super::stream_removal(
                    super::stream_access(&pool, &recheck_tid, &viewer_id).await,
                ) {
                    let _ = sender.send(ws::Message::Text(removal)).await;
                    break;
                }

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

#[derive(Deserialize, serde::Serialize)]
pub struct UpdateTeamAvailabilityForm {
    available: Option<bool>,
    team: String,
}

#[derive(Deserialize, serde::Serialize)]
pub struct CheckQuery {
    check: String,
}

pub async fn update_eligibility_for_all(
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
            "teams",
            &format!(
                "/tournaments/{tournament_id}/rounds/{round_id}/availability/teams/all?check={}",
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
            "teams",
            &format!(
                "/tournaments/{tournament_id}/rounds/{round_id}/availability/teams/all?check={}",
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
            return super::availability_error(
                modal_query,
                &tournament,
                user,
                round.seq,
                "teams",
                &format!(
                    "/tournaments/{tournament_id}/rounds/{round_id}/availability/teams/all?check={}",
                    query.check
                ),
                &query,
                "Choose check in or check out.",
            );
        }
    }

    let _ = tx.send(Msg {
        tournament,
        inner: MsgContents::TeamAvailabilityUpdate,
    });

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!(
            "/tournaments/{}/rounds/{}/availability/teams",
            tournament_id, round.seq
        )),
    )
}

pub async fn update_team_availability_for_round(
    Path((tournament_id, round_id)): Path<(String, String)>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    Extension(tx): Extension<Sender<Msg>>,
    Form(form): Form<UpdateTeamAvailabilityForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let available_bool = form.available.unwrap_or(false);

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
        return super::availability_error(
            modal_query,
            &tournament,
            user,
            round.seq,
            "teams",
            &format!(
                "/tournaments/{tournament_id}/rounds/{round_id}/update_team_eligibility"
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
            "teams",
            &format!(
                "/tournaments/{tournament_id}/rounds/{round_id}/update_team_eligibility"
            ),
            &form,
            "Complete preceding rounds before changing this round’s availability.",
        );
    }

    // TODO: check that a team can't be allocated to multiple concurrent rounds

    let n = diesel::insert_into(team_availability::table)
        .values((
            team_availability::id.eq(Uuid::now_v7().to_string()),
            team_availability::round_id.eq(&round.id),
            team_availability::team_id.eq(&team.id),
            team_availability::available.eq(available_bool),
            team_availability::tournament_id.eq(&tournament.id),
        ))
        .on_conflict((team_availability::round_id, team_availability::team_id))
        .do_update()
        .set(team_availability::available.eq(available_bool))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    if available_bool {
        diesel::update(
            team_availability::table.filter(
                team_availability::team_id.eq(&team.id).and(
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
                                    team_availability::round_id.eq(rounds::id),
                                ),
                        ),
                    ),
                ),
            ),
        )
        .set(team_availability::available.eq(false))
        .execute(&mut *conn)
        .unwrap();
    }

    debug_assert!(
        team_availability::table
            .filter(team_availability::tournament_id.eq(&tournament.id))
            .filter(team_availability::team_id.eq(&team.id))
            .filter(team_availability::available.eq(true))
            .filter(
                team_availability::round_id.eq_any(
                    rounds::table
                        .filter(rounds::tournament_id.eq(&tournament.id))
                        .filter(rounds::seq.eq(round.seq))
                        .select(rounds::id),
                )
            )
            .count()
            .get_result::<i64>(&mut *conn)?
            <= 1
    );

    let _ = tx.send(Msg {
        tournament,
        inner: MsgContents::TeamAvailabilityUpdate,
    });

    return crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!(
            "/tournaments/{tournament_id}/rounds/{}/availability/teams",
            round.seq
        )),
    );
}
