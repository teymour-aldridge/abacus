use std::collections::HashSet;

use crate::{
    state::Conn,
    tournaments::rounds::TournamentRounds,
    util_resp::{StandardResponse, success},
};
use axum::{
    extract::{
        Extension, Path, Query,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use diesel::prelude::*;

use hypertext::prelude::*;
use itertools::{Either, Itertools};
use serde::Deserialize;
use tokio::{sync::broadcast::Sender, task::spawn_blocking};

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::tournaments,
    state::DbPool,
    template::Page,
    tournaments::{
        Tournament, manage::sidebar::SidebarWrapper,
        participants::TournamentParticipants,
    },
};

pub mod constraints;
pub mod create_judge;
pub mod create_speaker;
pub mod create_team;
pub mod gen_private_url;
pub mod institution_selector;
pub mod manage_judge;
pub mod manage_private_urls;
pub mod manage_team;
pub mod team_form;

pub struct ParticipantsTable(Tournament, TournamentParticipants);

impl Renderable for ParticipantsTable {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            div class="d-flex flex-column flex-lg-row gap-4 mt-3" hx-ext="ws" hx-swap-oob="morphdom"
            "ws-connect"=(format!("/tournaments/{}/participants/ws", self.0.id)) {
                // Teams Column
                div class="flex-fill" {
                    div class="d-flex justify-content-between align-items-end mb-4 pb-2 border-bottom" {
                        h5 class="mb-0 text-uppercase fw-bold" style="letter-spacing: 2px;" { "1. Teams" }
                        a href=(format!("/tournaments/{}/teams/create", self.0.id)) class="btn btn-primary btn-sm" { "+ Team" }
                    }

                    div class="table-responsive" {
                        table class="table table-hover align-middle" id="teamsTable" {
                            thead class="border-bottom" {
                                tr {
                                    th scope="col" class="text-uppercase small fw-bold text-muted py-3" style="width: 60px;" { "#" }
                                    th scope="col" class="text-uppercase small fw-bold text-muted py-3 pe-1 d-none d-lg-table-cell" { "Institution" }
                                    th scope="col" class="text-uppercase small fw-bold text-muted py-3 ps-1" { "Name" }
                                    th scope="col" class="text-end text-uppercase small fw-bold text-muted py-3" { "Actions" }
                                }
                            }
                            tbody {
                                @for team in self.1.teams.values() {
                                    // Main team row with name, institution, and actions
                                    tr {
                                        th scope="row" class="text-center py-3 fw-normal text-muted" { (team.number) }
                                        td class="d-none d-lg-table-cell py-3 pe-1" {
                                            @if let Some(inst) = &team.institution_id {
                                                span class="fw-bold fs-5" { (self.1.institutions[inst.as_str()].name) }
                                            } @else {
                                                span class="text-muted fw-light" { "—" }
                                            }
                                        }
                                        td class="py-3 ps-1" {
                                            span class="fw-bold fs-5" { (team.name) }
                                        }
                                        td class="text-end py-3" {
                                            div class="d-flex justify-content-end gap-2" {
                                                a href=(format!("/tournaments/{}/teams/{}/edit", self.0.id, team.id)) class="btn btn-sm btn-outline-dark" { "Edit" }
                                                a href=(format!("/tournaments/{}/teams/{}/speakers/create", self.0.id, team.id)) class="btn btn-sm btn-outline-dark" { "+ Speaker" }
                                            }
                                        }
                                    }
                                    // Speakers row - displayed below team
                                    tr {
                                        td class="p-0" { }
                                        td colspan="3" class="pb-4 pt-2" {
                                            div class="ps-3 border-start" {
                                                @if self.1.team_speakers.get(&team.id).unwrap_or(&HashSet::default()).is_empty() {
                                                    p class="text-muted mb-0 small fst-italic py-1" { "No speakers" }
                                                } @else {
                                                    @for (i, speaker) in self.1.team_speakers.get(&team.id).unwrap_or(&HashSet::default()).iter().sorted_by_key(|speaker| {
                                                        self.1.speakers.get(speaker.as_str()).unwrap().name.clone()
                                                    }).enumerate() {
                                                        @let speaker = self.1.speakers.get(speaker.as_str()).unwrap();
                                                        div class="d-flex justify-content-between align-items-start py-1" {
                                                            div {
                                                                span class="text-muted small me-2" { (i + 1) "." }
                                                                span class="small" { (speaker.name) }
                                                                span class="text-muted small ms-2 d-none d-md-inline font-monospace" { (speaker.email) }
                                                            }
                                                            div class="d-flex gap-2" {
                                                                a href=(format!("/tournaments/{}/participants/speaker/{}/constraints", self.0.id, speaker.id)) class="text-decoration-none text-muted small" { "Constraints" }
                                                                a href=(format!("/tournaments/{}/teams/{}/speakers/{}/edit", self.0.id, team.id, speaker.id)) class="text-decoration-none text-muted small" { "Edit" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Judges Column
                div class="flex-fill" {
                    div class="d-flex justify-content-between align-items-end mb-4 pb-2 border-bottom" {
                        h5 class="mb-0 text-uppercase fw-bold" style="letter-spacing: 2px;" { "2. Judges" }
                        a href=(format!("/tournaments/{}/judges/create", self.0.id)) class="btn btn-primary btn-sm" { "+ Judge" }
                    }

                    div class="table-responsive" {
                        table class="table table-hover align-middle" id="judgesTable" {
                            thead class="border-bottom" {
                                tr {
                                    th scope="col" class="text-uppercase small fw-bold text-muted py-3" { "Name" }
                                    th scope="col" class="text-uppercase small fw-bold text-muted py-3 d-none d-md-table-cell" { "Institution" }
                                    th scope="col" class="text-end text-uppercase small fw-bold text-muted py-3" { "Actions" }
                                }
                            }
                            tbody {
                                @for judge in self.1.judges.values() {
                                    tr {
                                        td class="py-4" {
                                            div class="fw-bold fs-5" { (judge.name) }
                                            div class="text-muted small font-monospace mt-1" { (judge.email) }
                                        }
                                        td class="d-none d-md-table-cell py-4" {
                                            @if let Some(inst) = &judge.institution_id {
                                                (self.1.institutions.get(inst.as_str()).unwrap().name)
                                            } @else {
                                                span class="text-muted fw-light" { "—" }
                                            }
                                        }
                                        td class="text-end py-4" {
                                            div class="d-flex justify-content-end gap-2" {
                                                a href=(format!("/tournaments/{}/participants/judge/{}/constraints", self.0.id, judge.id)) class="btn btn-sm btn-outline-dark" { "Constraints" }
                                                a href=(format!("/tournaments/{}/judges/{}/edit", self.0.id, judge.id)) class="btn btn-sm btn-outline-dark" { "Edit" }
                                            }
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

#[derive(Deserialize)]
pub struct ManageParticipantsQuery {
    table_only: Option<bool>,
}

pub async fn manage_tournament_participants_impl(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Query(query): Query<ManageParticipantsQuery>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let participants = TournamentParticipants::load(&tid, &mut *conn);
    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let table = ParticipantsTable(tournament.clone(), participants.clone());

    let current_rounds =
        crate::tournaments::rounds::Round::current_rounds(&tid, &mut *conn);

    if query.table_only.unwrap_or(false) {
        success(table.render())
    } else {
        success(
            Page::new()
                .active_nav(crate::template::ActiveNav::Participants)
                .user(user)
                .tournament(tournament.clone())
                .extra_head(maud! {
                    script src="https://cdn.jsdelivr.net/npm/htmx-ext-ws@2.0.2" crossorigin="anonymous" {
                    }
                })
                .current_rounds(current_rounds.clone())
                .body(maud! {
                    SidebarWrapper tournament=(&tournament) rounds=(&rounds) selected_seq=(current_rounds.first().map(|r| r.seq)) active_page=(None) {
                        h1 {
                            "Participants"
                        }

                        (table)
                    }
                })
                .render(),
        )
    }
}

pub async fn manage_tournament_participants(
    Path(tid): Path<String>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
    Query(query): Query<ManageParticipantsQuery>,
) -> StandardResponse {
    if let Some(user) = user {
        // Check permissions
        let tournament = Tournament::fetch(&tid, &mut *conn)?;
        if tournament
            .check_user_is_superuser(&user.id, &mut *conn)
            .is_ok()
        {
            return manage_tournament_participants_impl(
                Path(tid),
                user,
                conn,
                Query(query),
            )
            .await;
        }

        return crate::tournaments::participants::public::public_participants_page(
            Path(tid),
            Some(user),
            conn,
        )
        .await;
    }

    crate::tournaments::participants::public::public_participants_page(
        Path(tid),
        None,
        conn,
    )
    .await
}

pub async fn tournament_participant_updates(
    ws: WebSocketUpgrade,
    Path(tid): Path<String>,
    Extension(pool): Extension<DbPool>,
    Extension(tx): Extension<Sender<Msg>>,
    user: User<false>,
) -> impl IntoResponse {
    let tid1 = tid.clone();
    let pool1 = pool.clone();

    // Validate permission before upgrading
    let tournament_validation = spawn_blocking(move || {
        let mut conn = pool1.get().unwrap();
        let tournament = tournaments::table
            .filter(tournaments::id.eq(tid1))
            .first::<Tournament>(&mut conn)
            .optional()
            .unwrap();

        if let Some(tournament) = tournament {
            if tournament
                .check_user_has_permission(
                    &user.id,
                    crate::permission::Permission::ManageParticipants,
                    &mut *conn,
                )
                .is_err()
            {
                None
            } else {
                Some(tournament)
            }
        } else {
            None
        }
    })
    .await
    .unwrap();

    let tournament = match tournament_validation {
        Some(t) => t,
        None => return axum::http::StatusCode::FORBIDDEN.into_response(),
    };

    ws.on_upgrade(move |socket| {
        handle_socket(socket, pool, tx, tid, tournament)
    })
}

async fn handle_socket(
    mut socket: WebSocket,
    pool: DbPool,
    tx: Sender<Msg>,
    tid: String,
    tournament: Tournament,
) {
    let pool2 = pool.clone();
    let tid2 = tid.clone();
    let get_serializable_data = move || {
        let mut conn = pool2.get().unwrap();

        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            Ok(TournamentParticipants::load(&tid2, &mut *conn))
        })
        .unwrap()
    };

    let mut rx = tx.subscribe();

    loop {
        let msg = tokio::select! {
            msg = rx.recv() => Either::Left(msg),
            msg = socket.recv() => Either::Right(msg),
        };

        match msg {
            Either::Left(Ok(msg)) => {
                if msg.tournament.id == tournament.id
                    && matches!(msg.inner, MsgContents::ParticipantsUpdate)
                {
                    let participants =
                        spawn_blocking(get_serializable_data.clone())
                            .await
                            .unwrap();
                    let html =
                        ParticipantsTable(tournament.clone(), participants)
                            .render()
                            .into_inner();

                    if socket.send(Message::Text(html)).await.is_err() {
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
