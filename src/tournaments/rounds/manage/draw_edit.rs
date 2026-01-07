//! Code to edit the draw.

use std::fmt::Write;

use axum::{
    Extension, Form,
    extract::{Path, Query, WebSocketUpgrade, ws},
    response::{IntoResponse, Redirect},
};
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use futures::{sink::SinkExt, stream::StreamExt};
use hypertext::{Raw, prelude::*};
use itertools::Itertools;
use serde::Deserialize;
use tokio::{
    sync::broadcast::{Receiver, Sender},
    task::spawn_blocking,
};
use uuid::Uuid;

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{
        tournament_debate_judges, tournament_debate_teams, tournament_debates,
        tournament_judges, tournament_rooms, tournament_rounds,
    },
    state::{Conn, DbPool},
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        participants::{DebateJudge, Judge, TournamentParticipants},
        rooms::Room,
        rounds::{
            Round, TournamentRounds,
            draws::{Debate, DebateTeam, RoundDrawRepr},
        },
    },
    util_resp::{
        StandardResponse, bad_request, err_not_found, see_other_ok, success,
    },
    widgets::alert::ErrorAlert,
};

#[derive(Deserialize, Debug)]
pub struct RoundsQuery {
    #[serde(default)]
    rounds: Vec<String>,
}

#[tracing::instrument(skip(conn))]
pub async fn edit_multiple_draws_page(
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

    let _all_rounds =
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
                SidebarWrapper rounds=(&_all_rounds) tournament=(&tournament) active_page=(Some(crate::tournaments::manage::sidebar::SidebarPage::Draw)) selected_seq=(Some(rounds2edit[0].seq)) {
                    div id="root" {}
                    script {
                        (Raw::dangerously_create(&format!(
                            r#"window.drawEditorConfig = {{
                                tournamentId: "{}",
                                roundIds: [{}]
                            }};"#,
                            tournament.id,
                            round_ids.iter().map(|id| format!("\"{}\"", id)).join(",")
                        )))
                    }
                    script type="module" crossorigin="anonymous" src="/draw_editor.js" {}
                    link rel="stylesheet" crossorigin="anonymous" href="/draw_editor.css";
                }
            })
            .render(),
    )
}

#[derive(Deserialize)]
pub struct ChannelQuery {
    rounds: Option<String>,
}

/// Provides a WebSocket channel which notifies clients when the draw has been
/// updated. After receiving this message, the client should then reload the
/// draw (using [`edit_draw_page_tab_dir`], with the `table_only` flag set to
/// true).
pub async fn draw_updates(
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
    let round_ids = round_ids.clone();

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

use crate::tournaments::teams::Team;
use serde::Serialize;

#[derive(Serialize)]
struct DrawUpdate {
    unallocated_judges: Vec<Judge>,
    unallocated_teams: Vec<Team>,
    unallocated_rooms: Vec<Room>,
    rounds: Vec<RoundDrawRepr>,
}

// ... existing code

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

        let rounds = tournament_rounds::table
            .filter(tournament_rounds::tournament_id.eq(&tournament_id_clone))
            .filter(tournament_rounds::id.eq_any(&round_ids_clone))
            .load::<Round>(&mut conn)
            .unwrap();

        let reprs = rounds
            .into_iter()
            .map(|round| RoundDrawRepr::of_round(round, &mut *conn))
            .collect::<Vec<_>>();

        let participants =
            TournamentParticipants::load(&tournament_id_clone, &mut *conn);
        let allocated_judge_ids: std::collections::HashSet<String> = reprs
            .iter()
            .flat_map(|repr| {
                repr.debates.iter().flat_map(|debate| {
                    debate.judges_of_debate.iter().map(|dj| dj.judge_id.clone())
                })
            })
            .collect();

        let unallocated_judges = participants
            .judges
            .values()
            .filter(|j| !allocated_judge_ids.contains(&j.id))
            .cloned()
            .collect();

        let unallocated_teams = participants.teams.values().cloned().collect();

        // Get unallocated rooms
        let all_rooms = tournament_rooms::table
            .filter(tournament_rooms::tournament_id.eq(&tournament_id_clone))
            .load::<Room>(&mut *conn)
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

        DrawUpdate {
            unallocated_judges,
            unallocated_teams,
            unallocated_rooms,
            rounds: reprs,
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

                    let rounds = tournament_rounds::table
                        .filter(
                            tournament_rounds::tournament_id
                                .eq(&tournament_id_clone),
                        )
                        .filter(tournament_rounds::id.eq_any(&round_ids_clone))
                        .load::<Round>(&mut conn)
                        .unwrap();

                    let reprs = rounds
                        .into_iter()
                        .map(|round| RoundDrawRepr::of_round(round, &mut *conn))
                        .collect::<Vec<_>>();

                    let participants = TournamentParticipants::load(
                        &tournament_id_clone,
                        &mut *conn,
                    );
                    let allocated_judge_ids: std::collections::HashSet<String> =
                        reprs
                            .iter()
                            .flat_map(|repr| {
                                repr.debates.iter().flat_map(|debate| {
                                    debate
                                        .judges_of_debate
                                        .iter()
                                        .map(|dj| dj.judge_id.clone())
                                })
                            })
                            .collect();

                    let unallocated_judges = participants
                        .judges
                        .values()
                        .filter(|j| !allocated_judge_ids.contains(&j.id))
                        .cloned()
                        .collect();

                    let unallocated_teams =
                        participants.teams.values().cloned().collect();

                    // Get unallocated rooms
                    let all_rooms = tournament_rooms::table
                        .filter(
                            tournament_rooms::tournament_id
                                .eq(&tournament_id_clone),
                        )
                        .load::<Room>(&mut *conn)
                        .unwrap();
                    let allocated_room_ids: std::collections::HashSet<String> =
                        reprs
                            .iter()
                            .flat_map(|repr| {
                                repr.debates.iter().filter_map(|debate| {
                                    debate.debate.room_id.clone()
                                })
                            })
                            .collect();
                    let unallocated_rooms = all_rooms
                        .into_iter()
                        .filter(|r| !allocated_room_ids.contains(&r.id))
                        .collect();

                    DrawUpdate {
                        unallocated_judges,
                        unallocated_teams,
                        unallocated_rooms,
                        rounds: reprs,
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

// ... existing code

#[derive(Deserialize)]
pub struct EditDrawForm {
    cmd: String,
}

#[derive(Deserialize)]
pub struct SubmitQuery {
    rounds: Option<String>,
}

pub async fn submit_cmd(
    Path(tournament_id): Path<String>,
    Query(query): Query<SubmitQuery>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<EditDrawForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round_ids: Vec<String> = query
        .rounds
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let rounds = match tournament_rounds::table
        .filter(tournament_rounds::id.eq_any(&round_ids))
        .load::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(t) => t,
        None => return err_not_found(),
    };

    let cmd = match Cmd::parse(&form.cmd) {
        Ok(cmd) => cmd,
        Err(e) => {
            return bad_request(
                ErrorAlert {
                    msg: format!("Invalid command provided: {e}"),
                }
                .render(),
            );
        }
    };

    let (judge_number, debate_number, role) = match cmd {
        Cmd::Trainee(judge, debate) => (judge, debate, Role::Trainee),
        Cmd::Panelist(judge, debate) => (judge, debate, Role::Panelist),
        Cmd::Chair(judge, debate) => (judge, debate, Role::Chair),
    };

    let apply_move =
        apply_move(judge_number, debate_number, role, &rounds, &mut *conn);
    match apply_move {
        Ok(()) => see_other_ok(Redirect::to(&format!(
            "/tournaments/{tournament_id}/rounds/draws/edit?rounds={}",
            round_ids.iter().join(",")
        ))),
        Err(e) => bad_request(
            ErrorAlert {
                msg: format!("Error evaluating command: {e}"),
            }
            .render(),
        ),
    }
}

pub struct JudgeDebateAllocation {
    debate: Debate,
    debate_judge: DebateJudge,
}

impl JudgeDebateAllocation {
    /// Find the position to which the given judge has been assigned.
    fn find(
        judge_no: u32,
        rounds: &[String],
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Option<Self> {
        match tournament_debates::table
            .filter(tournament_debates::round_id.eq_any(rounds))
            .inner_join(
                tournament_debate_judges::table.on(
                    tournament_debate_judges::debate_id
                        .eq(tournament_debates::id)
                        .and(
                            tournament_debate_judges::judge_id.eq_any(
                                tournament_judges::table
                                    .filter(
                                        tournament_judges::number
                                            .eq(judge_no as i64),
                                    )
                                    .select(tournament_judges::id),
                            ),
                        ),
                ),
            )
            .first::<(Debate, DebateJudge)>(&mut *conn)
            .optional()
            .unwrap()
        {
            Some((debate, debate_judge)) => Some(Self {
                debate,
                debate_judge,
            }),
            None => None,
        }
    }
}

fn apply_move(
    judge_no: u32,
    debate_no: Option<u32>,
    role: Role,
    rounds: &[Round],
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<(), String> {
    let judge = match tournament_judges::table
        .filter(tournament_judges::number.eq(judge_no as i64))
        .first::<Judge>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(judge) => judge,
        None => return Err(format!("No such judge with numnber j{judge_no}")),
    };

    let debate_ids = rounds
        .iter()
        .map(|round| round.id.clone())
        .collect::<Vec<_>>();

    let existing_alloc =
        JudgeDebateAllocation::find(judge_no, &debate_ids, &mut *conn);

    let debate_to_alloc_to = if let Some(debate_no) = debate_no {
        match tournament_debates::table
            .filter(
                tournament_debates::round_id
                    .eq_any(&debate_ids)
                    .and(tournament_debates::number.eq(debate_no as i64)),
            )
            .first::<Debate>(conn)
            .optional()
            .unwrap()
        {
            Some(d) => Some(d),
            None => {
                return Err(format!(
                    "Debate with number {debate_no} does not exist."
                ));
            }
        }
    } else {
        None
    };

    let _delete_existing_alloc = {
        if let Some(alloc) = existing_alloc {
            diesel::delete(
                tournament_debate_judges::table.filter(
                    tournament_debate_judges::debate_id
                        .eq(alloc.debate.id)
                        .and(
                            tournament_debate_judges::judge_id
                                .eq(alloc.debate_judge.judge_id),
                        ),
                ),
            )
            .execute(&mut *conn)
            .unwrap();
        }
    };

    let _create_new_alloc = {
        if let Some(alloc) = debate_to_alloc_to {
            diesel::insert_into(tournament_debate_judges::table)
                .values((
                    tournament_debate_judges::debate_id.eq(alloc.id),
                    tournament_debate_judges::judge_id.eq(judge.id),
                    tournament_debate_judges::status.eq(role.to_string()),
                ))
                .execute(&mut *conn)
                .unwrap();
        }
    };

    Ok(())
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Role {
    Trainee,
    Panelist,
    Chair,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_char(match self {
            Role::Trainee => 'T',
            Role::Panelist => 'P',
            Role::Chair => 'C',
        })
    }
}

impl Role {
    pub fn of_str(item: &str) -> Result<Self, String> {
        match item {
            "C" => Ok(Role::Chair),
            "P" => Ok(Role::Panelist),
            "T" => Ok(Role::Trainee),
            "" => Ok(Role::Panelist), // Default role
            _ => Err(format!("Invalid role: {}", item)),
        }
    }
}

pub enum Cmd {
    Trainee(u32, Option<u32>),
    Panelist(u32, Option<u32>),
    Chair(u32, Option<u32>),
}

impl Cmd {
    pub fn parse(input: &str) -> Result<Self, String> {
        crate::cmd::CmdParser::new()
            .parse(input)
            .map_err(|e| e.to_string())
    }
}

#[derive(Deserialize, Debug)]
pub struct MoveJudgeForm {
    judge_id: String,
    to_debate_id: Option<String>,
    role: String,
    rounds: Vec<String>,
}

pub async fn move_judge(
    Path(tournament_id): Path<String>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<MoveJudgeForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round_ids = form.rounds;

    let judge = match tournament_judges::table
        .filter(tournament_judges::id.eq(&form.judge_id))
        .filter(tournament_judges::tournament_id.eq(&tournament.id))
        .first::<Judge>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(j) => j,
        None => {
            return bad_request(maud! { "Judge not found" }.render());
        }
    };

    let transaction_result = conn.transaction(|conn| {
        diesel::delete(
            tournament_debate_judges::table.filter(
                tournament_debate_judges::judge_id.eq(&judge.id).and(
                    tournament_debate_judges::debate_id.eq_any(
                        tournament_debates::table
                            .filter(
                                tournament_debates::round_id.eq_any(&round_ids),
                            )
                            .select(tournament_debates::id),
                    ),
                ),
            ),
        )
        .execute(conn)?;

        if let Some(to_debate_id) = form.to_debate_id.filter(|s| !s.is_empty())
        {
            let debate = match tournament_debates::table
                .filter(
                    tournament_debates::id
                        .eq(&to_debate_id)
                        .and(tournament_debates::round_id.eq_any(&round_ids)),
                )
                .first::<Debate>(conn)
                .optional()?
            {
                Some(d) => d,
                None => {
                    // This will rollback the transaction
                    return Err(diesel::result::Error::NotFound);
                }
            };

            let role = match Role::of_str(&form.role) {
                Ok(role) => role,
                Err(e) => {
                    // This will rollback the transaction
                    return Err(diesel::result::Error::QueryBuilderError(
                        e.into(),
                    ));
                }
            };

            diesel::insert_into(tournament_debate_judges::table)
                .values((
                    tournament_debate_judges::id.eq(Uuid::new_v4().to_string()),
                    tournament_debate_judges::debate_id.eq(debate.id),
                    tournament_debate_judges::judge_id.eq(judge.id),
                    tournament_debate_judges::status.eq(role.to_string()),
                    tournament_debate_judges::tournament_id
                        .eq(tournament.id.clone()),
                ))
                .execute(conn)?;
        }
        Ok(success(Default::default()))
    });

    let res = match transaction_result {
        Ok(res) => res,
        Err(diesel::result::Error::NotFound) => {
            return bad_request(maud! { "Debate not found" }.render());
        }
        Err(e) => {
            return bad_request(maud! { (e.to_string()) }.render());
        }
    };

    for round_id in &round_ids {
        let _ = tx.send(Msg {
            tournament: tournament.clone(),
            inner: MsgContents::DrawUpdated(round_id.clone()),
        });
    }

    res
}

#[derive(Deserialize)]
pub struct ChangeRoleForm {
    judge_id: String,
    debate_id: String,
    role: String,
    rounds: Vec<String>,
}

/// Handles changing a judge's role in a debate
pub async fn change_judge_role(
    Path(tournament_id): Path<String>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    Form(form): Form<ChangeRoleForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round_ids = form.rounds;

    let role = match form.role.as_str() {
        "C" => Role::Chair,
        "P" => Role::Panelist,
        "T" => Role::Trainee,
        _ => Role::Panelist,
    };

    diesel::update(
        tournament_debate_judges::table.filter(
            tournament_debate_judges::judge_id
                .eq(&form.judge_id)
                .and(tournament_debate_judges::debate_id.eq(&form.debate_id)),
        ),
    )
    .set(tournament_debate_judges::status.eq(role.to_string()))
    .execute(&mut *conn)
    .unwrap();

    for round_id in &round_ids {
        let _ = tx.send(Msg {
            tournament: tournament.clone(),
            inner: MsgContents::DrawUpdated(round_id.clone()),
        });
    }

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{tournament_id}/rounds/draws/edit?{}",
        round_ids.iter().map(|r| format!("rounds={}", r)).join("&")
    )))
}

#[derive(Deserialize, Debug)]
pub struct MoveTeamForm {
    team1_id: String,
    team2_id: String,
    rounds: Vec<String>,
}

pub async fn move_team(
    Path(tournament_id): Path<String>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<MoveTeamForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round_ids = form.rounds;

    let transaction_result = conn.transaction(|conn| {
        let team1_debate_team = tournament_debate_teams::table
            .filter(tournament_debate_teams::team_id.eq(&form.team1_id))
            .filter(tournament_debate_teams::tournament_id.eq(&tournament.id))
            .filter(
                tournament_debate_teams::debate_id.eq_any(
                    tournament_debates::table
                        .filter(tournament_debates::round_id.eq_any(&round_ids))
                        .select(tournament_debates::id),
                ),
            )
            .first::<DebateTeam>(conn)?;

        let team2_debate_team = tournament_debate_teams::table
            .filter(tournament_debate_teams::team_id.eq(&form.team2_id))
            .filter(tournament_debate_teams::tournament_id.eq(&tournament.id))
            .filter(
                tournament_debate_teams::debate_id.eq_any(
                    tournament_debates::table
                        .filter(tournament_debates::round_id.eq_any(&round_ids))
                        .select(tournament_debates::id),
                ),
            )
            .first::<DebateTeam>(conn)?;

        let temp_debate_id = team1_debate_team.debate_id.clone();
        let temp_side = team1_debate_team.side;
        let temp_seq = team1_debate_team.seq;

        diesel::update(
            tournament_debate_teams::table
                .filter(tournament_debate_teams::id.eq(&team1_debate_team.id)),
        )
        .set((
            tournament_debate_teams::debate_id
                .eq(team2_debate_team.debate_id.clone()),
            tournament_debate_teams::side.eq(team2_debate_team.side),
            tournament_debate_teams::seq.eq(team2_debate_team.seq),
        ))
        .execute(conn)?;

        diesel::update(
            tournament_debate_teams::table
                .filter(tournament_debate_teams::id.eq(&team2_debate_team.id)),
        )
        .set((
            tournament_debate_teams::debate_id.eq(temp_debate_id),
            tournament_debate_teams::side.eq(temp_side),
            tournament_debate_teams::seq.eq(temp_seq),
        ))
        .execute(conn)?;

        Ok(success(Default::default()))
    });

    let res = match transaction_result {
        Ok(res) => res,
        Err(diesel::result::Error::NotFound) => {
            return bad_request(maud! { "Team not found in draw." }.render());
        }
        Err(e) => {
            return bad_request(maud! { (e.to_string()) }.render());
        }
    };

    for round_id in &round_ids {
        let _ = tx.send(Msg {
            tournament: tournament.clone(),
            inner: MsgContents::DrawUpdated(round_id.clone()),
        });
    }

    res
}
