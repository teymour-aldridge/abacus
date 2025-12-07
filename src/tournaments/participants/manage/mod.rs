use std::collections::HashSet;

use crate::{
    state::Conn,
    tournaments::{manage::sidebar::SidebarWrapper, rounds::TournamentRounds},
    util_resp::{StandardResponse, success},
    widgets::actions::Actions,
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
    tournaments::{Tournament, participants::TournamentParticipants},
};

pub mod create_judge;
pub mod create_speaker;
pub mod create_team;
pub mod gen_private_url;
pub mod institution_selector;
pub mod manage_judge;
pub mod manage_private_urls;
pub mod manage_team;

pub struct ParticipantsTable(Tournament, TournamentParticipants);

impl Renderable for ParticipantsTable {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            div class="row" hx-ext="ws" hx-swap-oob="morphdom"
            "ws-connect"=(format!("/tournaments/{}/participants/ws", self.0.id)) {
                div class="col mb-3 col-md-6" {
                    h3 {
                        "Judges"
                    }
                    Actions options=(&[
                        (format!("/tournaments/{}/judges/create", self.0.id).as_str(), "Add judge")
                    ]);

                    table class = "table  table-striped table-bordered" id="judgesTable" {
                        thead {
                            th scope="col" {
                                "Name"
                            }
                            th scope="col" {
                                "Email"
                            }
                            th scope="col" {
                                "Institution"
                            }
                            th scope="col" {
                                "Actions"
                            }
                        }
                        tbody {
                            @for judge in self.1.judges.values() {
                                tr {
                                    th scope="col" {
                                        (judge.name)
                                    }
                                    td style="word-wrap: break-word;min-width: 160px;max-width: 160px;" {
                                        (judge.email)
                                    }
                                    td {
                                        @if let Some(inst) = &judge.institution_id {
                                            (self.1.institutions.get(inst.as_str()).unwrap().name)
                                        } @else {
                                            "None"
                                        }
                                    }
                                    td {
                                        a href=(format!(
                                            "/tournaments/{}/judges/{}/edit",
                                            self.0.id,
                                            judge.id
                                            ))
                                            class="m-2" {
                                            "Edit"
                                        }
                                    }
                                }
                            }
                        }
                    }

                }
                div class="col mb-3 col-md-6" {
                    h3 {
                        "Teams"
                    }
                    Actions options=(&[
                        (format!("/tournaments/{}/teams/create", self.0.id).as_str(), "Add team")
                    ]);

                    table class="table table-striped table-bordered" id="teamsTable"
                    {
                        thead {
                            th scope="col" {
                                "#"
                            }
                            th scope="col" {
                                "Name"
                            }
                            th scope="col" {
                                "Institution"
                            }
                            th scope="col" {
                                "Actions"
                            }
                        }
                        tbody class="table-group-divider" {
                            @for team in self.1.teams.values() {
                                tr {
                                    th scope="row" {
                                        (team.number)
                                    }
                                    td {
                                        (team.name)
                                    }
                                    td {
                                       @if let Some(inst) = &team.institution_id {
                                           (self.1.institutions[inst.as_str()].name)
                                       } @else {
                                           "None"
                                       }
                                    }
                                    td {
                                        a href=(format!(
                                            "/tournaments/{}/teams/{}/edit",
                                            self.0.id,
                                            team.id
                                          ))
                                          class="m-2" {
                                            "Edit"
                                        }
                                        a href=(format!(
                                            "/tournaments/{}/teams/{}/speakers/create",
                                            self.0.id,
                                            team.id
                                          )) class="m-2" {
                                            "Add speaker"
                                        }
                                    }
                                }

                                tr {
                                    td colspan="4" {
                                        p {
                                            b {
                                                "Speakers"
                                            }
                                        }
                                        table class="table  table-bordered mb-0" {
                                            thead {
                                                th scope="col" {
                                                    "#"
                                                }
                                                th scope="col" {
                                                    "Name"
                                                }
                                                th scope="col" {
                                                    "Email"
                                                }
                                                th scope="col" {
                                                    "Actions"
                                                }
                                            }
                                            tbody {
                                                @for (i, speaker) in self.1.team_speakers.get(&team.id).unwrap_or(&HashSet::default()).iter().sorted_by_key(|speaker| {
                                                    self.1.speakers.get(speaker.as_str()).unwrap().name.clone()
                                                }).enumerate() {
                                                    @let speaker = self.1.speakers.get(speaker.as_str()).unwrap();
                                                    tr {
                                                        th scope="col" {
                                                            (i)
                                                        }
                                                        td {
                                                            (speaker.name)
                                                        }
                                                        td style="word-wrap: break-word;min-width: 160px;max-width: 160px;" {
                                                            (speaker.email)
                                                        }
                                                        td {
                                                            // todo: edit speaker page
                                                            a href=(format!("/tournaments/{}/teams/{}/speakers/{}/edit", self.0.id, team.id, speaker.id)) {
                                                                "Edit speaker"
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
            }
        }.render_to(buffer);
    }
}

#[derive(Deserialize)]
pub struct ManageParticipantsQuery {
    table_only: Option<bool>,
}

pub async fn manage_tournament_participants(
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

    if query.table_only.unwrap_or(false) {
        success(table.render())
    } else {
        success(
            Page::default()
                .user(user)
                .tournament(tournament.clone())
                .extra_head(maud! {
                    script src="https://cdn.jsdelivr.net/npm/htmx-ext-ws@2.0.2" crossorigin="anonymous" {
                    }
                })
                .body(maud! {
                    SidebarWrapper tournament=(&tournament) rounds=(&rounds) {
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
