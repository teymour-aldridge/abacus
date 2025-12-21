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

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{
        tournament_debate_judges, tournament_debates, tournament_judges,
        tournament_rounds,
    },
    state::{Conn, DbPool},
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        participants::{DebateJudge, Judge, TournamentParticipants},
        rounds::{
            Round, TournamentRounds,
            draws::{
                Debate, DebateRepr, RoundDrawRepr,
                manage::{DrawTableHeaders, RoomsOfRoundTable},
            },
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

    tracing::debug!("Retrieved {} rounds to edit", rounds2edit.len());

    let reprs = rounds2edit
        .into_iter()
        .map(|round| RoundDrawRepr::of_round(round.clone(), &mut *conn))
        .collect::<Vec<_>>();

    let participants = TournamentParticipants::load(&tournament_id, &mut *conn);

    success(
        Page::new()
            .tournament(tournament.clone())
            .user(user)
            .extra_head(maud! {
                script src="https://cdn.jsdelivr.net/npm/htmx-ext-ws@2.0.2" crossorigin="anonymous" {}
            })
            .body(maud! {
                SidebarWrapper rounds=(&all_rounds) tournament=(&tournament) active_page=(Some("draw")) selected_seq=(Some(reprs[0].round.seq)) {
                    script src="https://cdn.jsdelivr.net/npm/sortablejs@1.15.3/Sortable.min.js" {}
                    script src="https://cdn.jsdelivr.net/npm/htmx-ext-response-targets@2.0.2" {
                    }

                    div class="draw-editor" {
                        h1 {
                            "Edit Draw"
                            span style="font-weight: 400; font-size: 1.25rem; display: block; margin-top: 0.5rem; color: #666666;" {
                                @for (i, repr) in reprs.iter().enumerate() {
                                    @if i > 0 {
                                        ", "
                                    }
                                    (repr.round.name)
                                }
                            }
                        }

                        (renderer_of_drag_drop_instructions(&tournament, &reprs))

                        div hx-ext="ws"
                            "ws-connect"=(
                                format!(
                                    "/tournaments/{}/rounds/draws/edit/ws?rounds={}",
                                    tournament.id,
                                    reprs.iter().map(|repr| repr.round.id.clone()).join(",")
                                )
                            )
                        {
                            (get_refreshable_part(&tournament, &reprs, &participants))
                        }

                        div id="dragDropConfig"
                            style="display:none"
                            data-tournament-id=(tournament.id)
                            data-round-ids=(reprs.iter().map(|repr| repr.round.id.clone()).join(","))
                        {}
                    }

                    // This script handles the drag-and-drop code.
                    script {
                        (Raw::dangerously_create(
                        r#"
                        (function() {
                            function initializeSortables() {
                                var config = document.getElementById('dragDropConfig');
                                if (!config) {
                                    console.error('Config element not found');
                                    return;
                                }
                                var tournamentId = config.dataset.tournamentId;
                                var roundIds = config.dataset.roundIds;

                                if (typeof Sortable === 'undefined') {
                                    console.error('Sortable.js not loaded');
                                    return;
                                }

                                // Make unallocated judges sortable
                                var unallocatedContainer = document.getElementById('unallocatedJudges');
                                if (unallocatedContainer) {
                                    Sortable.create(unallocatedContainer, {
                                        group: 'judges',
                                        animation: 150,
                                        ghostClass: 'sortable-ghost',
                                        chosenClass: 'sortable-chosen',
                                        dragClass: 'sortable-drag',
                                        sort: false,
                                        onEnd: function(evt) {
                                            console.log('Drag ended from unallocated');
                                            handleJudgeDrop(evt, tournamentId, roundIds);
                                        }
                                    });
                                }

                                var dropZones = document.querySelectorAll('.judge-drop-zone');
                                console.log('Found drop zones: ' + dropZones.length);

                                dropZones.forEach(function(el, index) {
                                    Sortable.create(el, {
                                        group: 'judges',
                                        animation: 150,
                                        ghostClass: 'sortable-ghost',
                                        chosenClass: 'sortable-chosen',
                                        dragClass: 'sortable-drag',
                                        onEnd: function(evt) {
                                            console.log('Drag ended to drop zone');
                                            handleJudgeDrop(evt, tournamentId, roundIds);
                                        }
                                    });
                                });

                                document.querySelectorAll('.judge-remove-btn').forEach(function(btn) {
                                    btn.onclick = function(e) {
                                        e.stopPropagation();
                                        e.preventDefault();
                                        var badge = this.closest('.judge-badge');
                                        var judgeId = badge.dataset.judgeId;
                                        var container = badge.closest('.judge-drop-zone');
                                        var debateId = container ? container.dataset.debateId : '';
                                        console.log('Remove clicked: ' + judgeId + ' from ' + debateId);
                                        if (debateId) {
                                            removeJudge(judgeId, debateId, tournamentId, roundIds);
                                        }
                                    };
                                });

                                console.log('Sortables initialized');
                            }

                            function handleJudgeDrop(evt, tournamentId, roundIds) {
                                var judgeId = evt.item.dataset.judgeId;
                                var toContainer = evt.to;
                                var toDebateId = toContainer.dataset.debateId || '';
                                var toRole = toContainer.dataset.role || 'P';

                                if (toRole === 'C') {
                                    var existingChairs = toContainer.querySelectorAll('.judge-badge');
                                    var otherChairs = Array.from(existingChairs).filter(function(badge) {
                                        return badge.dataset.judgeId !== judgeId;
                                    });
                                    if (otherChairs.length > 0) {
                                        var errDiv = document.getElementById('dragDropErrMsg');
                                        if (errDiv) {
                                            errDiv.innerHTML = '<div class="alert alert-danger">Only one chair per debate. Remove the existing chair first.</div>';
                                            setTimeout(function() { errDiv.innerHTML = ''; }, 3000);
                                        }
                                        setTimeout(function() { window.location.reload(); }, 500);
                                        return;
                                    }
                                }

                                console.log('handleJudgeDrop - judgeId: ' + judgeId + ', toDebateId: ' + toDebateId + ', toRole: ' + toRole);

                                var body = new URLSearchParams();
                                body.append('judge_id', judgeId);
                                body.append('to_debate_id', toDebateId);
                                body.append('role', toRole);
                                roundIds.split(',').forEach(id => body.append('rounds', id));

                                console.log('Sending request');

                                fetch('/tournaments/' + tournamentId + '/rounds/draws/edit/move', {
                                    method: 'POST',
                                    headers: {
                                        'Content-Type': 'application/x-www-form-urlencoded'
                                    },
                                    body: body.toString()
                                }).then(function(response) {
                                    console.log('Response status: ' + response.status);
                                    if (response.ok) {
                                        return response.text();
                                    } else {
                                        throw new Error('Move failed: ' + response.status);
                                    }
                                }).then(function(html) {
                                    console.log('Got HTML response');
                                    var container = document.getElementById('tableContainer');
                                    if (container) {
                                        container.outerHTML = html;
                                        setTimeout(initializeSortables, 100);
                                    }
                                }).catch(function(err) {
                                    console.error('Error: ' + err);
                                    var errDiv = document.getElementById('dragDropErrMsg');
                                    if (errDiv) {
                                        errDiv.innerHTML = '<div class="alert alert-danger">Failed to move judge</div>';
                                    }
                                    setTimeout(function() { window.location.reload(); }, 2000);
                                });
                            }

                            function removeJudge(judgeId, debateId, tournamentId, roundIds) {
                                console.log('removeJudge: ' + judgeId);

                                var body = new URLSearchParams();
                                body.append('judge_id', judgeId);
                                body.append('to_debate_id', '');
                                body.append('role', 'P');
                                roundIds.split(',').forEach(id => body.append('rounds', id));

                                fetch('/tournaments/' + tournamentId + '/rounds/draws/edit/move', {
                                    method: 'POST',
                                    headers: {
                                        'Content-Type': 'application/x-www-form-urlencoded'
                                    },
                                    body: body.toString()
                                }).then(function(response) {
                                    if (response.ok) {
                                        return response.text();
                                    } else {
                                        throw new Error('Remove failed');
                                    }
                                }).then(function(html) {
                                    var container = document.getElementById('tableContainer');
                                    if (container) {
                                        container.outerHTML = html;
                                        setTimeout(initializeSortables, 100);
                                    }
                                }).catch(function(err) {
                                    console.error('Error: ' + err);
                                    window.location.reload();
                                });
                            }

                            if (document.readyState === 'loading') {
                                document.addEventListener('DOMContentLoaded', initializeSortables);
                            } else {
                                initializeSortables();
                            }

                            // Re-initialize Sortables after WebSocket content updates
                            document.body.addEventListener('htmx:wsAfterMessage', function() {
                                setTimeout(initializeSortables, 100);
                            });
                        })();
                        "#))
                    }
                }
            })
            .render(),
    )
}

fn renderer_of_drag_drop_instructions(
    _tournament: &Tournament,
    _rounds: &[RoundDrawRepr],
) -> impl Renderable {
    maud! {
        div id="dragDropErrMsg" {}
    }
}

fn get_refreshable_part(
    tournament: &Tournament,
    reprs: &[RoundDrawRepr],
    participants: &TournamentParticipants,
) -> impl Renderable {
    let allocated_judge_ids: std::collections::HashSet<String> = reprs
        .iter()
        .flat_map(|repr| {
            repr.debates.iter().flat_map(|debate| {
                debate.judges_of_debate.iter().map(|dj| dj.judge_id.clone())
            })
        })
        .collect();

    maud! {
        div id="tableContainer"
            hx-swap-oob="morphdom"
            data-tournament-rounds=(reprs.iter().map(|repr| repr.round.id.clone()).join(","))
        {
            h3 {
                "Unallocated Judges"
            }
            div class="mb-4 sticky-top unallocated-judges-container" {
                div id="unallocatedJudges" class="" {
                    @for judge in participants.judges.values().filter(|j| !allocated_judge_ids.contains(&j.id)) {
                        div class="judge-badge"
                            data-judge-id=(judge.id)
                            data-judge-number=(judge.number)
                            data-role="P"
                            draggable="true"
                        {
                            (judge.name) " (j" (judge.number) ")"
                        }
                    }
                    @if participants.judges.values().all(|j| allocated_judge_ids.contains(&j.id)) {
                        span class="text-muted" { "All judges are allocated" }
                    }
                }
            }

            table class="table" {
                DrawTableHeaders tournament=(&tournament) editable=(true);

                @for repr in reprs {
                    @if repr.debates.is_empty() {
                        div class="alert alert-warning" role="alert" {
                            "Note: there exists no draw for " (repr.round.name)
                        }
                    } @else {
                        RoomsOfRoundTable
                            tournament=(&tournament)
                            repr=(&repr)
                            actions=(|_: &DebateRepr| maud! {"None"})
                            participants=(participants)
                            body_only=(true);
                    }
                }

            }

        }
    }
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

async fn handle_socket(
    socket: ws::WebSocket,
    mut rx: Receiver<Msg>,
    pool: DbPool,
    tournament_id: String,
    round_ids: Vec<String>,
    tournament: Tournament,
) {
    let (mut sender, mut receiver) = socket.split();

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
                let tournament = tournament.clone();
                let tournament_id = tournament_id.clone();
                let round_ids = round_ids.clone();

                let rendered = spawn_blocking(move || {
                    let mut conn = pool1.get().unwrap();

                    let rounds = match tournament_rounds::table
                        .filter(tournament_rounds::tournament_id.eq(&tournament.id))
                        .filter(tournament_rounds::id.eq_any(&round_ids))
                        .load::<Round>(&mut conn)
                        .optional()
                        .unwrap() {
                            Some(rounds) if rounds.len() == round_ids.len() => {
                                rounds
                            },
                            Some(_) | None => {
                                return maud! {
                                    ErrorAlert msg=("Looks like a round was deleted. Please refresh the page!");
                                }.render().into_inner()
                            },
                        };


                    let reprs =
                        rounds.into_iter().map(|round| {
                            RoundDrawRepr::of_round(round, &mut *conn)
                        }).collect::<Vec<_>>();

                    let participants = TournamentParticipants::load(&tournament_id, &mut *conn);

                    get_refreshable_part(&tournament, &reprs, &participants)
                        .render()
                        .into_inner()
                })
                .await.unwrap();

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
    pub fn of_str(item: &str) -> Role {
        match item {
            "C" => Role::Chair,
            "P" => Role::Panelist,
            "T" => Role::Trainee,
            _ => unreachable!(
                "should not pass incorrect values to Role::from_str"
            ),
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

    diesel::delete(
        tournament_debate_judges::table.filter(
            tournament_debate_judges::judge_id.eq(&judge.id).and(
                tournament_debate_judges::debate_id.eq_any(
                    tournament_debates::table
                        .filter(tournament_debates::round_id.eq_any(&round_ids))
                        .select(tournament_debates::id),
                ),
            ),
        ),
    )
    .execute(&mut *conn)
    .unwrap();

    if let Some(to_debate_id) = form.to_debate_id.filter(|s| !s.is_empty()) {
        let debate = match tournament_debates::table
            .filter(
                tournament_debates::id
                    .eq(&to_debate_id)
                    .and(tournament_debates::round_id.eq_any(&round_ids)),
            )
            .first::<Debate>(&mut *conn)
            .optional()
            .unwrap()
        {
            Some(d) => d,
            None => {
                return bad_request(maud! { "Debate not found" }.render());
            }
        };

        let role = match form.role.as_str() {
            "C" => Role::Chair,
            "P" => Role::Panelist,
            "T" => Role::Trainee,
            _ => Role::Panelist,
        };

        diesel::insert_into(tournament_debate_judges::table)
            .values((
                tournament_debate_judges::debate_id.eq(debate.id),
                tournament_debate_judges::judge_id.eq(judge.id),
                tournament_debate_judges::status.eq(role.to_string()),
            ))
            .execute(&mut *conn)
            .unwrap();
    }

    let rounds = match tournament_rounds::table
        .filter(tournament_rounds::id.eq_any(&round_ids))
        .load::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(r) => r,
        None => return err_not_found(),
    };

    let reprs: Vec<RoundDrawRepr> = rounds
        .into_iter()
        .map(|round| RoundDrawRepr::of_round(round, &mut *conn))
        .collect();

    let participants = TournamentParticipants::load(&tournament_id, &mut *conn);

    for round_id in &round_ids {
        let _ = tx.send(Msg {
            tournament: tournament.clone(),
            inner: MsgContents::DrawUpdated(round_id.clone()),
        });
    }

    let render =
        get_refreshable_part(&tournament, &reprs, &participants).render();
    success(render)
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
