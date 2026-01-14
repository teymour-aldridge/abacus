use axum::{
    Extension,
    extract::{Path, Query, WebSocketUpgrade, ws},
    response::IntoResponse,
};
use diesel::SqliteConnection;
use diesel::prelude::*;
use futures::{sink::SinkExt, stream::StreamExt};
use hypertext::{Raw, prelude::*};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use tokio::{sync::broadcast::Receiver, task::spawn_blocking};

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{tournament_rooms, tournament_rounds},
    state::{Conn, DbPool},
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rooms::Room,
        rounds::{Round, TournamentRounds, draws::RoundDrawRepr},
    },
    util_resp::{StandardResponse, err_not_found, success},
};

#[derive(Deserialize, Debug)]
pub struct RoundsQuery {
    #[serde(default)]
    rounds: Vec<String>,
}

#[tracing::instrument(skip(conn))]
pub async fn load_room_allocator_page(
    Path(tournament_id): Path<String>,
    axum_extra::extract::Query(query): axum_extra::extract::Query<RoundsQuery>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    if query.rounds.is_empty() {
        return err_not_found();
    }
    let rounds_vec = query.rounds;

    let all_rounds =
        TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    let rounds2edit = match tournament_rounds::table
        .filter(tournament_rounds::id.eq_any(&rounds_vec))
        .load::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(t) if t.len() == rounds_vec.len() => t,
        Some(_) | None => {
            return err_not_found();
        }
    };

    let round_ids =
        rounds2edit.iter().map(|r| r.id.clone()).collect::<Vec<_>>();

    let current_rounds = crate::tournaments::rounds::Round::current_rounds(
        &tournament.id,
        &mut *conn,
    );

    success(
        Page::new()
            .tournament(tournament.clone())
            .user(user)
            .current_rounds(current_rounds)
            .active_nav(crate::template::ActiveNav::Draw)
            .body(maud! {
                SidebarWrapper rounds=(&all_rounds) tournament=(&tournament) active_page=(Some(crate::tournaments::manage::sidebar::SidebarPage::Draw)) selected_seq=(Some(rounds2edit[0].seq)) {
                    div id="root" {}
                    script {
                        (Raw::dangerously_create(&format!(
                            r#"
                            window.drawRoomAllocatorConfig = {{
                                tournamentId: "{}",
                                roundIds: [{}]
                            }};
                            "#,
                            tournament.id,
                            round_ids.iter().map(|id| format!("\"{}\"", id)).join(",")
                        )))
                    }
                    script type="module" crossorigin="anonymous" src="/draw_room_allocator.js" {}
                    link rel="stylesheet" crossorigin="anonymous" href="/draw_room_allocator.css";
                }
            })
            .render(),
    )
}

#[derive(Deserialize)]
pub struct ChannelQuery {
    rounds: Option<String>,
}

pub async fn room_allocator_updates(
    ws: WebSocketUpgrade,
    Path(tournament_id): Path<String>,
    Query(query): Query<ChannelQuery>,
    Extension(pool): Extension<DbPool>,
    Extension(tx): Extension<tokio::sync::broadcast::Sender<Msg>>,
    user: User<false>,
) -> impl IntoResponse {
    let pool: DbPool = pool.clone();

    let round_ids: Vec<String> = query
        .rounds
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let pool1 = pool.clone();
    let round_ids_clone = round_ids.clone();
    let setup_result = spawn_blocking(move || {
        let round_ids = round_ids_clone;
        let mut conn = pool1.get().unwrap();
        let tournament = Tournament::fetch(&tournament_id, &mut conn).ok()?;
        tournament
            .check_user_is_superuser(&user.id, &mut conn)
            .ok()?;

        let rounds = tournament_rounds::table
            .filter(tournament_rounds::tournament_id.eq(&tournament.id))
            .filter(tournament_rounds::id.eq_any(&round_ids))
            .load::<Round>(&mut conn)
            .optional()
            .unwrap();

        if rounds.as_ref().unwrap_or(&vec![]).len() != round_ids.len() {
            return None;
        }

        rounds.map(|_| tournament)
    })
    .await
    .unwrap();

    let tournament = match setup_result {
        Some(t) => t,
        None => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                "Not found or access denied",
            )
                .into_response();
        }
    };

    let rx = tx.subscribe();
    let tournament_id_str = tournament.id.clone();

    ws.on_upgrade(move |socket| {
        handle_socket(
            socket,
            rx,
            pool,
            tournament_id_str,
            round_ids,
            tournament,
        )
    })
}

#[derive(Serialize)]
struct DrawUpdate {
    unallocated_rooms: Vec<Room>,
    rounds: Vec<RoundDrawRepr>,
}

async fn handle_socket(
    socket: ws::WebSocket,
    mut rx: Receiver<Msg>,
    pool: DbPool,
    tournament_id: String,
    round_ids: Vec<String>,
    _tournament: Tournament,
) {
    let (mut sender, mut receiver) = socket.split();

    // Send initial state
    let pool1 = pool.clone();
    let tournament_id_clone = tournament_id.clone();
    let round_ids_clone = round_ids.clone();

    let initial_data = spawn_blocking(move || {
        let mut conn = pool1.get().unwrap();
        let (unallocated_rooms, rounds) =
            get_draw_update(&tournament_id_clone, &round_ids_clone, &mut conn);
        DrawUpdate {
            unallocated_rooms,
            rounds,
        }
    })
    .await
    .unwrap();

    let rendered = serde_json::to_string(&initial_data).unwrap();
    if sender
        .send(ws::Message::Text(rendered.into()))
        .await
        .is_err()
    {
        return;
    }

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            let should_update = if msg.tournament.id == tournament_id {
                if let MsgContents::DrawUpdated(updated_round_id) = &msg.inner {
                    round_ids.contains(updated_round_id)
                } else {
                    false
                }
            } else {
                false
            };

            if should_update {
                let pool1 = pool.clone();
                let tournament_id_clone = tournament_id.clone();
                let round_ids_clone = round_ids.clone();

                let update_data = spawn_blocking(move || {
                    let mut conn = pool1.get().unwrap();
                    let (unallocated_rooms, rounds) = get_draw_update(
                        &tournament_id_clone,
                        &round_ids_clone,
                        &mut conn,
                    );
                    DrawUpdate {
                        unallocated_rooms,
                        rounds,
                    }
                })
                .await
                .unwrap();

                let rendered = serde_json::to_string(&update_data).unwrap();

                if sender
                    .send(ws::Message::Text(rendered.into()))
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
            // keep alive
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}

fn get_draw_update(
    tournament_id: &str,
    round_ids: &[String],
    conn: &mut SqliteConnection,
) -> (Vec<Room>, Vec<RoundDrawRepr>) {
    let rounds = tournament_rounds::table
        .filter(tournament_rounds::tournament_id.eq(tournament_id))
        .filter(tournament_rounds::id.eq_any(round_ids))
        .load::<Round>(conn)
        .unwrap();

    let reprs = rounds
        .into_iter()
        .map(|round| RoundDrawRepr::of_round(round, conn))
        .collect::<Vec<_>>();

    let all_rooms = tournament_rooms::table
        .filter(tournament_rooms::tournament_id.eq(tournament_id))
        .load::<Room>(conn)
        .unwrap();

    let allocated_room_ids: std::collections::HashSet<String> = reprs
        .iter()
        .flat_map(|repr| {
            repr.debates
                .iter()
                .filter_map(|debate| debate.debate.room_id.clone())
        })
        .collect();

    let unallocated_rooms = all_rooms
        .into_iter()
        .filter(|r| !allocated_room_ids.contains(&r.id))
        .collect();
    (unallocated_rooms, reprs)
}
