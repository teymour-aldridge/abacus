use std::collections::HashSet;

use crate::{
    state::Conn,
    tournaments::{manage::sidebar::SidebarWrapper, rounds::TournamentRounds},
    util_resp::{StandardResponse, success},
    widgets::actions::Actions,
};
use diesel::prelude::*;
use hypertext::prelude::*;
use itertools::{Either, Itertools};
use rocket::{
    State,
    futures::{SinkExt, StreamExt},
    get,
};
use tokio::{sync::broadcast::Receiver, task::spawn_blocking};

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
pub mod manage_team;

pub struct ParticipantsTable(Tournament, TournamentParticipants);

impl Renderable for ParticipantsTable {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            div class="row" hx-ext="ws" hx-swap-oob="morphdom"
            "ws-connect"=(format!("/tournaments/{}/participants?channel", self.0.id)) {
                div class="col" {
                    h3 {
                        "Judges"
                    }
                    Actions options=(&[
                        (format!("/tournaments/{}/judges/create", self.0.id).as_str(), "Add judge")
                    ]);

                    table class = "table table-striped table-bordered" id="judgesTable" {
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
                                    td {
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
                div class="col" {
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
                                    td colspan="4" class="px-4 py-4" {
                                        p {
                                            b {
                                                "Speakers"
                                            }
                                        }
                                        table class="table table-bordered mb-0" {
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
                                                        td {
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

// TODO: can remove `table_only` flag (!)
#[get("/tournaments/<tid>/participants?<table_only>")]
pub async fn manage_tournament_participants(
    tid: &str,
    user: User<true>,
    mut conn: Conn<true>,
    table_only: bool,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let participants = TournamentParticipants::load(tid, &mut *conn);
    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let table = ParticipantsTable(tournament.clone(), participants.clone());

    if table_only {
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

#[get("/tournaments/<tid>/participants?channel")]
pub async fn tournament_participant_updates(
    tid: &str,
    // todo: can replace with ThreadSafeConn where TX = false
    pool: &State<DbPool>,
    ws: rocket_ws::WebSocket,
    rx: &State<Receiver<Msg>>,
    user: User<false>,
) -> Option<rocket_ws::Channel<'static>> {
    let pool: DbPool = pool.inner().clone();

    let tid1 = tid.to_string();
    let pool1 = pool.clone();
    let tournament = spawn_blocking(move || {
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

    let tournament = match tournament {
        Some(t) => t,
        None => return None,
    };

    let pool2 = pool.clone();
    let tid2 = tid.to_string();
    let get_serializable_data = move || {
        let mut conn = pool2.get().unwrap();

        conn.transaction(|conn| -> Result<_, diesel::result::Error> {
            Ok(TournamentParticipants::load(&tid2, &mut *conn))
        })
        .unwrap()
    };

    let mut rx: Receiver<Msg> = rx.inner().resubscribe();
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
                        && matches!(msg.inner, MsgContents::ParticipantsUpdate)
                    {
                        msg
                    } else {
                        continue;
                    }
                };

                match msg.inner {
                    MsgContents::ParticipantsUpdate => {
                        let participants =
                            spawn_blocking(get_serializable_data.clone())
                                .await
                                .unwrap();
                        let _ = stream
                            .send(rocket_ws::Message::Text(
                                ParticipantsTable(
                                    tournament.clone(),
                                    participants,
                                )
                                .render()
                                .into_inner(),
                            ))
                            .await;
                    }
                    _ => unreachable!(),
                }
            }
        })
    }))
}
