//! Code to edit the draw.

use std::{collections::HashSet, fmt::Write};

use axum::{
    Extension, Form,
    extract::{Path, Query, WebSocketUpgrade, ws},
    response::{IntoResponse, Redirect},
};
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use futures::{sink::SinkExt, stream::StreamExt};
use hypertext::prelude::*;
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
        debates, judges, judges_of_debate, rooms, rounds, teams_of_debate,
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
            draws::{Debate, DebateRepr, DebateTeam, RoundDrawRepr},
        },
    },
    util_resp::{
        StandardResponse, bad_request, err_not_found, see_other_ok, success,
    },
    widgets::alert::ErrorAlert,
};

#[derive(Deserialize, Debug)]
pub struct EditDrawsQueryString {
    #[serde(default)]
    rounds: Vec<String>,
    selected_judge_id: Option<String>,
    source_debate_id: Option<String>,
    selected_room_id: Option<String>,
    source_room_debate_id: Option<String>,
    error: Option<String>,
    #[serde(default)]
    partial: bool,
}

#[tracing::instrument(skip(conn))]
pub async fn edit_draws_page(
    Path(tournament_id): Path<String>,
    axum_extra::extract::Query(query): axum_extra::extract::Query<
        EditDrawsQueryString,
    >,
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

    let draw = match load_draw_allocator_context(
        &tournament.id,
        &rounds_vec,
        &mut *conn,
    ) {
        Some(draw) if draw.rounds.len() == rounds_vec.len() => draw,
        Some(_) | None => {
            return err_not_found();
        }
    };
    let DrawAllocatorContext {
        rounds: rounds2edit,
        round_ids,
        reprs: draw_reprs,
        participants,
        unallocated_rooms,
    } = draw;

    let current_rounds = crate::tournaments::rounds::Round::current_rounds(
        &tournament.id,
        &mut *conn,
    );

    if query.partial {
        let state = DrawAllocatorState::from_query(
            &participants,
            &unallocated_rooms,
            &draw_reprs,
            query.selected_judge_id.as_deref(),
            query.source_debate_id.as_deref(),
            query.selected_room_id.as_deref(),
            query.source_room_debate_id.as_deref(),
        );

        return success(
            DrawAllocatorPage {
                tournament: &tournament,
                reprs: &draw_reprs,
                participants: &participants,
                unallocated_rooms: &unallocated_rooms,
                round_ids: &round_ids,
                state,
                error: query.error.as_deref(),
                oob: false,
            }
            .render(),
        );
    }

    success(
        Page::new()
            .tournament(tournament.clone())
            .user(user)
            .current_rounds(current_rounds)
            .active_nav(crate::template::ActiveNav::Draw)
            .extra_head(
                maud! {
                    script src="https://cdn.jsdelivr.net/npm/htmx-ext-ws@2.0.2" crossorigin="anonymous" {
                    }
                }
            )
            .body(maud! {
                SidebarWrapper rounds=(&_all_rounds) tournament=(&tournament) active_page=(Some(crate::tournaments::manage::sidebar::SidebarPage::Draw)) selected_seq=(Some(rounds2edit[0].seq)) {
                    (DrawAllocatorPage {
                        tournament: &tournament,
                        reprs: &draw_reprs,
                        participants: &participants,
                        unallocated_rooms: &unallocated_rooms,
                        round_ids: &round_ids,
                        state: DrawAllocatorState::from_query(
                            &participants,
                            &unallocated_rooms,
                            &draw_reprs,
                            query.selected_judge_id.as_deref(),
                            query.source_debate_id.as_deref(),
                            query.selected_room_id.as_deref(),
                            query.source_room_debate_id.as_deref(),
                        ),
                        error: query.error.as_deref(),
                        oob: false,
                    })
                }
            })
            .render(),
    )
}

struct DrawAllocatorPage<'a> {
    tournament: &'a Tournament,
    reprs: &'a [RoundDrawRepr],
    participants: &'a TournamentParticipants,
    unallocated_rooms: &'a [Room],
    round_ids: &'a [String],
    state: DrawAllocatorState<'a>,
    error: Option<&'a str>,
    oob: bool,
}

struct DrawAllocatorContext {
    rounds: Vec<Round>,
    round_ids: Vec<String>,
    reprs: Vec<RoundDrawRepr>,
    participants: TournamentParticipants,
    unallocated_rooms: Vec<Room>,
}

#[derive(Clone)]
enum DrawAllocatorState<'a> {
    Allocation,
    JudgeSelected(SelectedJudge<'a>),
    RoomSelected(SelectedRoom<'a>),
}

#[derive(Clone)]
struct SelectedJudge<'a> {
    judge: &'a Judge,
    location: JudgeAllocatedAt<'a>,
    panel_snapshot: String,
}

#[derive(Clone)]
struct SelectedRoom<'a> {
    room_id: &'a str,
    location: JudgeAllocatedAt<'a>,
}

#[derive(Clone, Copy)]
enum JudgeAllocatedAt<'a> {
    Unallocated,
    Debate(&'a str),
}

impl JudgeAllocatedAt<'_> {
    fn debate_id(&self) -> Option<&str> {
        match self {
            JudgeAllocatedAt::Unallocated => None,
            JudgeAllocatedAt::Debate(debate_id) => Some(debate_id),
        }
    }
}

impl<'a> DrawAllocatorState<'a> {
    fn from_query(
        participants: &'a TournamentParticipants,
        unallocated_rooms: &'a [Room],
        reprs: &'a [RoundDrawRepr],
        selected_judge_id: Option<&'a str>,
        source_debate_id: Option<&'a str>,
        selected_room_id: Option<&'a str>,
        _source_room_debate_id: Option<&'a str>,
    ) -> Self {
        if let Some(judge) =
            selected_judge_id.and_then(|id| participants.judges.get(id))
        {
            let Some(panel_snapshot) = source_panel_snapshot_from_repr(
                participants,
                reprs,
                source_debate_id,
            ) else {
                return Self::Allocation;
            };

            let location = match source_debate_id {
                Some(debate_id) => JudgeAllocatedAt::Debate(debate_id),
                None => JudgeAllocatedAt::Unallocated,
            };

            return Self::JudgeSelected(SelectedJudge {
                judge,
                location,
                panel_snapshot,
            });
        }

        if let Some(room_id) = selected_room_id {
            if let Some((room_id, location)) =
                selected_room_from_repr(unallocated_rooms, reprs, room_id)
            {
                return Self::RoomSelected(SelectedRoom { room_id, location });
            }
        }

        Self::Allocation
    }

    fn selected_judge(&self) -> Option<&'a Judge> {
        match self {
            Self::Allocation => None,
            Self::JudgeSelected(source) => Some(source.judge),
            Self::RoomSelected(_) => None,
        }
    }

    fn selected_judge_is(&self, judge_id: &str) -> bool {
        self.selected_judge()
            .is_some_and(|judge| judge.id == judge_id)
    }

    fn selected_room_is(&self, room_id: &str) -> bool {
        match self {
            Self::RoomSelected(source) => source.room_id == room_id,
            Self::Allocation | Self::JudgeSelected(_) => false,
        }
    }
}

impl Renderable for DrawAllocatorPage<'_> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        let edit_url = edit_path(self.tournament, self.round_ids);
        let ws_url = format!(
            "/tournaments/{}/rounds/draws/edit/ws?rounds={}",
            self.tournament.id,
            self.round_ids.join(",")
        );
        maud! {
            div id="draw-allocator" class="draw-allocator"
                hx-ext="ws" hx-swap-oob=(if self.oob { "morphdom" } else { "false" }) "ws-connect"=(ws_url) {
                div class="draw-allocator-header" {
                    h1 { "Draw editor" }
                    a class="btn btn-outline-secondary btn-sm" href=(edit_url)
                        hx-get=(partial_href(&edit_url))
                        hx-target="#draw-allocator"
                        hx-swap="outerHTML show:none"
                        hx-push-url=(edit_url) {
                        "Clear selection"
                    }
                }

                @if let Some(error) = draw_error_message(self.error) {
                    div class="alert alert-danger" role="alert" { (error) }
                }

                section class="draw-unallocated-bar" {
                    h2 { "Unallocated rooms" }
                    div class="draw-judge-strip" {
                        @for room in self.unallocated_rooms {
                            @let href = select_room_href(self.tournament, self.round_ids, &room.id, None);
                            a class=(room_link_class(&self.state, &room.id, "judge-pill room-pill"))
                                href=(&href)
                                hx-get=(partial_href(&href))
                                hx-target="#draw-allocator"
                                hx-swap="outerHTML show:none"
                                hx-push-url=(&href) {
                                (room.name)
                            }
                        }
                        @if self.unallocated_rooms.is_empty() {
                            span class="draw-empty-slot" { "No unallocated rooms" }
                        }
                        @if let DrawAllocatorState::RoomSelected(source) = &self.state {
                            span class="draw-inline-action" {
                                (MoveRoomActionForm {
                                    tournament: self.tournament,
                                    round_ids: self.round_ids,
                                    room_id: source.room_id,
                                    source_debate_id: source.location.debate_id(),
                                    target_debate_id: None,
                                    label: "De-allocate".to_string(),
                                    disabled: matches!(source.location, JudgeAllocatedAt::Unallocated),
                                })
                            }
                        }
                    }
                }

                section class="draw-unallocated-bar" {
                    h2 { "Unallocated judges" }
                    div class="draw-judge-strip" {
                        @let allocated_judge_ids = allocated_judge_ids(self.reprs);
                        @for judge in self.participants.judges.values().filter(|judge| !allocated_judge_ids.contains(&judge.id)) {
                            @let href = select_judge_href(self.tournament, self.round_ids, &judge.id, None);
                            a class=(judge_link_class(&self.state, &judge.id, "judge-pill"))
                                href=(&href)
                                hx-get=(partial_href(&href))
                                hx-target="#draw-allocator"
                                hx-swap="outerHTML show:none"
                                hx-push-url=(&href) {
                                (judge.name) " (j" (judge.number) ")"
                            }
                        }
                        @if let DrawAllocatorState::JudgeSelected(source) = &self.state {
                            span class="draw-inline-action" {
                                (MoveJudgeActionForm {
                                    tournament: self.tournament,
                                    judge: source.judge,
                                    source_debate_id: source.location.debate_id(),
                                    source_panel: Some(&source.panel_snapshot),
                                    target_debate_id: None,
                                    target_panel: panel_snapshot_for_unallocated_from_repr(self.participants, self.reprs),
                                    role: Role::Panelist,
                                    label: "Move here".to_string(),
                                    disabled: matches!(source.location, JudgeAllocatedAt::Unallocated),
                                })
                            }
                        }
                    }
                }

                @for repr in self.reprs {
                    section class="draw-round-section" {
                        h2 { (repr.round.name) }
                        table class="table draw-table" {
                            thead {
                                tr {
                                    th scope="col" { "#" }
                                    th scope="col" { "Room" }
                                    @for i in 0..self.tournament.teams_per_side {
                                        @for side in 0..2 {
                                            th scope="col" {
                                                (crate::tournaments::rounds::side_names::name_of_side(self.tournament, side, i, true))
                                            }
                                        }
                                    }
                                    th scope="col" { "Judges" }
                                }
                            }
                            tbody {
                                @for debate in &repr.debates {
                                    tr {
                                        th scope="row" { (debate.debate.number) }
                                        td class="draw-room-cell" {
                                            (render::room_cell(self.tournament, self.round_ids, debate, &self.state))
                                        }
                                        @for debate_team in &debate.teams_of_debate {
                                            td class="draw-team-cell" {
                                                @let team = self.participants.teams.get(&debate_team.team_id).unwrap();
                                                a href=(format!("/tournaments/{}/teams/{}", self.tournament.id, debate_team.team_id)) {
                                                    (self.participants.canonical_name_of_team(team))
                                                }
                                            }
                                        }
                                        td class="draw-panel-cell" {
                                            (render::judge_role(self.tournament, self.round_ids, self.participants, debate, &self.state, Role::Chair, "Chair"))
                                            (render::judge_role(self.tournament, self.round_ids, self.participants, debate, &self.state, Role::Panelist, "Panelist"))
                                            (render::judge_role(self.tournament, self.round_ids, self.participants, debate, &self.state, Role::Trainee, "Trainee"))
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

            }
        }
        .render_to(buffer);
    }
}

mod render {
    use super::*;

    pub(super) fn room_cell<'a>(
        tournament: &'a Tournament,
        round_ids: &'a [String],
        debate: &'a DebateRepr,
        state: &'a DrawAllocatorState<'a>,
    ) -> impl Renderable + 'a {
        let selected_already_in_target = match state {
            DrawAllocatorState::RoomSelected(source) => {
                source.location.debate_id() == Some(debate.debate.id.as_str())
            }
            DrawAllocatorState::Allocation
            | DrawAllocatorState::JudgeSelected(_) => false,
        };

        maud! {
            div class="draw-room-slot" {
                @if let (Some(room), Some(room_id)) = (&debate.room, debate.debate.room_id.as_deref()) {
                    @let href = select_room_href(tournament, round_ids, room_id, Some(&debate.debate.id));
                    a class=(room_link_class(state, room_id, "judge-pill room-pill"))
                        href=(&href)
                        hx-get=(partial_href(&href))
                        hx-target="#draw-allocator"
                        hx-swap="outerHTML show:none"
                        hx-push-url=(&href) {
                        (room.name)
                    }
                } @else {
                    span class="draw-empty-slot" { "Open" }
                }
                @if let DrawAllocatorState::RoomSelected(source) = state {
                    div class="draw-cell-action" {
                        (MoveRoomActionForm {
                            tournament,
                            round_ids,
                            room_id: source.room_id,
                            source_debate_id: source.location.debate_id(),
                            target_debate_id: Some(&debate.debate.id),
                            label: if debate.room.is_some() {
                                "Swap here".to_string()
                            } else {
                                "Move here".to_string()
                            },
                            disabled: selected_already_in_target,
                        })
                    }
                }
            }
        }
    }

    pub(super) fn judge_role<'a>(
        tournament: &'a Tournament,
        round_ids: &'a [String],
        participants: &'a TournamentParticipants,
        debate: &'a DebateRepr,
        state: &'a DrawAllocatorState<'a>,
        role: Role,
        role_label: &'a str,
    ) -> impl Renderable + 'a {
        let status = role.to_string();
        let judges = debate
            .judges_of_debate
            .iter()
            .filter(|dj| dj.status == status)
            .collect_vec();
        let panel_snapshot = panel_snapshot_for_debate(debate);
        let selected_already_in_target_role = match state {
            DrawAllocatorState::Allocation => false,
            DrawAllocatorState::JudgeSelected(source) => {
                source.location.debate_id() == Some(debate.debate.id.as_str())
                    && judges.iter().any(|dj| dj.judge_id == source.judge.id)
            }
            DrawAllocatorState::RoomSelected(_) => false,
        };

        maud! {
            div class="draw-role-row" {
                div class="draw-role-label" { (role_label) }
                div class="draw-role-judges" {
                    @for debate_judge in &judges {
                        @let judge = participants.judges.get(&debate_judge.judge_id).unwrap();
                        @let href = select_judge_href(tournament, round_ids, &judge.id, Some(&debate.debate.id));
                        a class=(judge_link_class(state, &judge.id, &role_class(role)))
                            href=(&href)
                            hx-get=(partial_href(&href))
                            hx-target="#draw-allocator"
                            hx-swap="outerHTML show:none"
                            hx-push-url=(&href) {
                            (judge.name) " (j" (judge.number) ")"
                        }
                    }
                    @if judges.is_empty() {
                        span class="draw-empty-slot" { "Open" }
                    }
                    @if let DrawAllocatorState::JudgeSelected(source) = state {
                        div class="draw-cell-action" {
                            (MoveJudgeActionForm {
                                tournament,
                                judge: source.judge,
                                source_debate_id: source.location.debate_id(),
                                source_panel: Some(&source.panel_snapshot),
                                target_debate_id: Some(&debate.debate.id),
                                target_panel: panel_snapshot.clone(),
                                role,
                                label: format!("Move here as {}", role_label.to_lowercase()),
                                disabled: selected_already_in_target_role,
                            })
                        }
                    }
                }
            }
        }
    }
}

fn allocated_judge_ids(reprs: &[RoundDrawRepr]) -> HashSet<String> {
    reprs
        .iter()
        .flat_map(|repr| {
            repr.debates.iter().flat_map(|debate| {
                debate
                    .judges_of_debate
                    .iter()
                    .map(|debate_judge| debate_judge.judge_id.clone())
            })
        })
        .collect()
}

fn allocated_room_ids(reprs: &[RoundDrawRepr]) -> HashSet<String> {
    reprs
        .iter()
        .flat_map(|repr| {
            repr.debates
                .iter()
                .filter_map(|debate| debate.debate.room_id.clone())
        })
        .collect()
}

fn unallocated_rooms_for_repr(
    tournament_id: &str,
    reprs: &[RoundDrawRepr],
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Vec<Room> {
    let allocated_room_ids = allocated_room_ids(reprs);
    rooms::table
        .filter(rooms::tournament_id.eq(tournament_id))
        .order_by(rooms::priority.asc())
        .load::<Room>(conn)
        .unwrap()
        .into_iter()
        .filter(|room| !allocated_room_ids.contains(&room.id))
        .collect()
}

fn selected_room_from_repr<'a>(
    unallocated_rooms: &'a [Room],
    reprs: &'a [RoundDrawRepr],
    room_id: &str,
) -> Option<(&'a str, JudgeAllocatedAt<'a>)> {
    if let Some(room) = unallocated_rooms.iter().find(|room| room.id == room_id)
    {
        return Some((&room.id, JudgeAllocatedAt::Unallocated));
    }

    reprs
        .iter()
        .flat_map(|repr| &repr.debates)
        .find_map(|debate| {
            debate.room.as_ref()?;
            let debate_room_id = debate.debate.room_id.as_deref()?;
            if debate_room_id == room_id {
                Some((
                    debate_room_id,
                    JudgeAllocatedAt::Debate(&debate.debate.id),
                ))
            } else {
                None
            }
        })
}

fn load_draw_allocator_context(
    tournament_id: &str,
    requested_round_ids: &[String],
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Option<DrawAllocatorContext> {
    let rounds2edit = rounds::table
        .filter(rounds::id.eq_any(requested_round_ids))
        .load::<Round>(conn)
        .optional()
        .unwrap()?;

    let round_ids =
        rounds2edit.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
    let reprs = rounds2edit
        .iter()
        .cloned()
        .map(|round| RoundDrawRepr::of_round(round, conn))
        .collect::<Vec<_>>();
    let participants = TournamentParticipants::load(tournament_id, conn);
    let unallocated_rooms =
        unallocated_rooms_for_repr(tournament_id, &reprs, conn);

    Some(DrawAllocatorContext {
        rounds: rounds2edit,
        round_ids,
        reprs,
        participants,
        unallocated_rooms,
    })
}

fn edit_path(tournament: &Tournament, round_ids: &[String]) -> String {
    edit_path_for_tournament_id(&tournament.id, round_ids)
}

fn edit_path_for_tournament_id(
    tournament_id: &str,
    round_ids: &[String],
) -> String {
    format!(
        "/tournaments/{tournament_id}/rounds/draws/edit?{}",
        rounds_query(round_ids)
    )
}

fn edit_path_with_error(
    tournament_id: &str,
    round_ids: &[String],
    error: &str,
) -> String {
    format!(
        "{}&error={error}",
        edit_path_for_tournament_id(tournament_id, round_ids)
    )
}

fn rounds_query(round_ids: &[String]) -> String {
    round_ids
        .iter()
        .map(|round_id| format!("rounds={round_id}"))
        .join("&")
}

fn partial_href(href: &str) -> String {
    if href.contains('?') {
        format!("{href}&partial=true")
    } else {
        format!("{href}?partial=true")
    }
}

fn select_judge_href(
    tournament: &Tournament,
    round_ids: &[String],
    judge_id: &str,
    source_debate_id: Option<&str>,
) -> String {
    let mut href = format!(
        "{}&selected_judge_id={judge_id}",
        edit_path(tournament, round_ids)
    );
    if let Some(source_debate_id) = source_debate_id {
        write!(href, "&source_debate_id={source_debate_id}").unwrap();
    }
    href
}

fn select_room_href(
    tournament: &Tournament,
    round_ids: &[String],
    room_id: &str,
    source_debate_id: Option<&str>,
) -> String {
    let mut href = format!(
        "{}&selected_room_id={room_id}",
        edit_path(tournament, round_ids)
    );
    if let Some(source_debate_id) = source_debate_id {
        write!(href, "&source_room_debate_id={source_debate_id}").unwrap();
    }
    href
}

fn judge_link_class(
    state: &DrawAllocatorState<'_>,
    judge_id: &str,
    base_class: &str,
) -> String {
    if state.selected_judge_is(judge_id) {
        format!("{base_class} selected")
    } else {
        base_class.to_string()
    }
}

fn room_link_class(
    state: &DrawAllocatorState<'_>,
    room_id: &str,
    base_class: &str,
) -> String {
    if state.selected_room_is(room_id) {
        format!("{base_class} selected")
    } else {
        base_class.to_string()
    }
}

fn role_class(role: Role) -> String {
    match role {
        Role::Chair => "judge-pill judge-pill-chair".to_string(),
        Role::Panelist => "judge-pill judge-pill-panelist".to_string(),
        Role::Trainee => "judge-pill judge-pill-trainee".to_string(),
    }
}

fn source_panel_snapshot_from_repr(
    participants: &TournamentParticipants,
    reprs: &[RoundDrawRepr],
    source_debate_id: Option<&str>,
) -> Option<String> {
    match source_debate_id {
        Some(source_debate_id) => reprs
            .iter()
            .flat_map(|repr| &repr.debates)
            .find(|debate| debate.debate.id == source_debate_id)
            .map(panel_snapshot_for_debate),
        None => Some(panel_snapshot_for_unallocated_from_repr(
            participants,
            reprs,
        )),
    }
}

fn panel_snapshot_for_debate(debate: &DebateRepr) -> String {
    let rows = debate
        .judges_of_debate
        .iter()
        .map(|dj| (dj.judge_id.clone(), dj.status.clone()))
        .collect_vec();
    panel_snapshot("debate", Some(&debate.debate.id), rows)
}

fn panel_snapshot_for_unallocated_from_repr(
    participants: &TournamentParticipants,
    reprs: &[RoundDrawRepr],
) -> String {
    let allocated = allocated_judge_ids(reprs);
    let rows = participants
        .judges
        .values()
        .filter(|judge| !allocated.contains(&judge.id))
        .map(|judge| (judge.id.clone(), "U".to_string()))
        .collect_vec();
    panel_snapshot("unallocated", None, rows)
}

fn panel_snapshot(
    kind: &str,
    debate_id: Option<&str>,
    mut rows: Vec<(String, String)>,
) -> String {
    rows.sort_unstable();
    let mut canonical = String::new();
    canonical.push_str(kind);
    if let Some(debate_id) = debate_id {
        canonical.push(':');
        canonical.push_str(debate_id);
    }
    for (judge_id, status) in rows {
        canonical.push('|');
        canonical.push_str(&judge_id);
        canonical.push(':');
        canonical.push_str(&status);
    }
    canonical
}

fn panel_change_key(from_panel: &str, to_panel: &str) -> String {
    let mut canonical = String::new();
    canonical.push_str("from:");
    canonical.push_str(from_panel);
    canonical.push_str("|to:");
    canonical.push_str(to_panel);
    stable_signature(&canonical)
}

fn panel_snapshot_for_debate_id(
    debate_id: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<String, diesel::result::Error> {
    let rows = judges_of_debate::table
        .filter(judges_of_debate::debate_id.eq(debate_id))
        .select((judges_of_debate::judge_id, judges_of_debate::status))
        .load::<(String, String)>(conn)?;
    Ok(panel_snapshot("debate", Some(debate_id), rows))
}

fn panel_snapshot_for_unallocated(
    tournament_id: &str,
    round_seq: i64,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<String, diesel::result::Error> {
    let allocated_ids = judges_of_debate::table
        .inner_join(
            debates::table.on(debates::id.eq(judges_of_debate::debate_id)),
        )
        .inner_join(rounds::table.on(rounds::id.eq(debates::round_id)))
        .filter(rounds::tournament_id.eq(tournament_id))
        .filter(rounds::seq.eq(round_seq))
        .select(judges_of_debate::judge_id)
        .load::<String>(conn)?
        .into_iter()
        .collect::<HashSet<_>>();
    let rows = judges::table
        .filter(judges::tournament_id.eq(tournament_id))
        .select(judges::id)
        .load::<String>(conn)?
        .into_iter()
        .filter(|judge_id| !allocated_ids.contains(judge_id))
        .map(|judge_id| (judge_id, "U".to_string()))
        .collect_vec();
    Ok(panel_snapshot("unallocated", None, rows))
}

fn debate_round_seq(
    debate_id: &str,
    tournament_id: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<i64, diesel::result::Error> {
    debates::table
        .inner_join(rounds::table.on(rounds::id.eq(debates::round_id)))
        .filter(debates::id.eq(debate_id))
        .filter(debates::tournament_id.eq(tournament_id))
        .select(rounds::seq)
        .first::<i64>(conn)
}

fn round_ids_for_seq(
    tournament_id: &str,
    round_seq: i64,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<Vec<String>, diesel::result::Error> {
    rounds::table
        .filter(rounds::tournament_id.eq(tournament_id))
        .filter(rounds::seq.eq(round_seq))
        .select(rounds::id)
        .load::<String>(conn)
}

fn current_debate_for_judge_in_seq(
    judge_id: &str,
    tournament_id: &str,
    round_seq: i64,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<Option<String>, diesel::result::Error> {
    judges_of_debate::table
        .inner_join(
            debates::table.on(debates::id.eq(judges_of_debate::debate_id)),
        )
        .inner_join(rounds::table.on(rounds::id.eq(debates::round_id)))
        .filter(judges_of_debate::judge_id.eq(judge_id))
        .filter(rounds::tournament_id.eq(tournament_id))
        .filter(rounds::seq.eq(round_seq))
        .select(judges_of_debate::debate_id)
        .first::<String>(conn)
        .optional()
}

fn stable_signature(input: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn draw_error_message(code: Option<&str>) -> Option<&'static str> {
    match code {
        Some("conflict") => {
            Some("One of those panels was modified, please try again.")
        }
        _ => None,
    }
}

struct MoveJudgeActionForm<'a> {
    tournament: &'a Tournament,
    judge: &'a Judge,
    source_debate_id: Option<&'a str>,
    source_panel: Option<&'a str>,
    target_debate_id: Option<&'a str>,
    target_panel: String,
    role: Role,
    label: String,
    disabled: bool,
}

impl Renderable for MoveJudgeActionForm<'_> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            form class="draw-move-form" method="post" action=(format!("/tournaments/{}/rounds/draws/edit/move", self.tournament.id)) {
                input type="hidden" name="judge_id" value=(self.judge.id);
                input type="hidden" name="to_debate_id" value=(self.target_debate_id.unwrap_or(""));
                input type="hidden" name="role" value=(self.role.to_string());
                input type="hidden" name="from_debate_id" value=(self.source_debate_id.unwrap_or(""));
                input type="hidden" name="change_key" value=(panel_change_key(self.source_panel.unwrap_or(""), &self.target_panel));
                button class="btn btn-outline-secondary btn-sm" type="submit" disabled[self.disabled] {
                    (self.label)
                }
            }
        }
        .render_to(buffer);
    }
}

struct MoveRoomActionForm<'a> {
    tournament: &'a Tournament,
    round_ids: &'a [String],
    room_id: &'a str,
    source_debate_id: Option<&'a str>,
    target_debate_id: Option<&'a str>,
    label: String,
    disabled: bool,
}

impl Renderable for MoveRoomActionForm<'_> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            form class="draw-move-form" method="post" action=(format!("/tournaments/{}/rounds/draws/edit/move_room", self.tournament.id)) {
                input type="hidden" name="room_id" value=(self.room_id);
                input type="hidden" name="from_debate_id" value=(self.source_debate_id.unwrap_or(""));
                input type="hidden" name="to_debate_id" value=(self.target_debate_id.unwrap_or(""));
                @for round_id in self.round_ids {
                    input type="hidden" name="rounds" value=(round_id);
                }
                button class="btn btn-outline-secondary btn-sm" type="submit" disabled[self.disabled] {
                    (self.label)
                }
            }
        }
        .render_to(buffer);
    }
}

#[derive(Deserialize)]
pub struct ChannelQuery {
    rounds: Option<String>,
}

/// Provides a WebSocket channel which updates clients when the draw changes.
pub async fn draw_updates(
    ws: WebSocketUpgrade,
    Path(tournament_id): Path<String>,
    Query(query): Query<ChannelQuery>,
    Extension(pool): Extension<DbPool>,
    Extension(tx): Extension<tokio::sync::broadcast::Sender<Msg>>,
    user: User<false>,
) -> impl IntoResponse {
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

        let rounds = rounds::table
            .filter(rounds::tournament_id.eq(&tournament.id))
            .filter(rounds::id.eq_any(&round_ids))
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
                let round_ids_clone = round_ids.clone();

                let rendered = spawn_blocking(move || {
                    let mut conn = pool1.get().unwrap();
                    render_draw_allocator(
                        &tournament,
                        &round_ids_clone,
                        &mut conn,
                    )
                })
                .await
                .unwrap();

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

fn render_draw_allocator(
    tournament: &Tournament,
    round_ids: &[String],
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> String {
    let draw =
        load_draw_allocator_context(&tournament.id, round_ids, conn).unwrap();

    DrawAllocatorPage {
        tournament,
        reprs: &draw.reprs,
        participants: &draw.participants,
        unallocated_rooms: &draw.unallocated_rooms,
        round_ids,
        state: DrawAllocatorState::Allocation,
        error: None,
        oob: true,
    }
    .render()
    .into_inner()
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

    let rounds = match rounds::table
        .filter(rounds::id.eq_any(&round_ids))
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
        match debates::table
            .filter(debates::round_id.eq_any(rounds))
            .inner_join(
                judges_of_debate::table.on(judges_of_debate::debate_id
                    .eq(debates::id)
                    .and(
                        judges_of_debate::judge_id.eq_any(
                            judges::table
                                .filter(judges::number.eq(judge_no as i64))
                                .select(judges::id),
                        ),
                    )),
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
    let judge = match judges::table
        .filter(judges::number.eq(judge_no as i64))
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
        match debates::table
            .filter(
                debates::round_id
                    .eq_any(&debate_ids)
                    .and(debates::number.eq(debate_no as i64)),
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
            diesel::delete(judges_of_debate::table.filter(
                judges_of_debate::debate_id.eq(alloc.debate.id).and(
                    judges_of_debate::judge_id.eq(alloc.debate_judge.judge_id),
                ),
            ))
            .execute(&mut *conn)
            .unwrap();
        }
    };

    let _create_new_alloc = {
        if let Some(alloc) = debate_to_alloc_to {
            diesel::insert_into(judges_of_debate::table)
                .values((
                    judges_of_debate::id.eq(Uuid::now_v7().to_string()),
                    judges_of_debate::debate_id.eq(alloc.id),
                    judges_of_debate::judge_id.eq(judge.id),
                    judges_of_debate::status.eq(role.to_string()),
                    judges_of_debate::tournament_id.eq(judge.tournament_id),
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
    from_debate_id: Option<String>,
    change_key: Option<String>,
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

    let judge = match judges::table
        .filter(judges::id.eq(&form.judge_id))
        .filter(judges::tournament_id.eq(&tournament.id))
        .first::<Judge>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(j) => j,
        None => {
            return bad_request(maud! { "Judge not found" }.render());
        }
    };

    let submitted_source_debate_id =
        form.from_debate_id.as_deref().filter(|id| !id.is_empty());
    let to_debate_id = form.to_debate_id.as_deref().filter(|s| !s.is_empty());

    let source_round_seq = match submitted_source_debate_id {
        Some(debate_id) => {
            match debate_round_seq(debate_id, &tournament.id, &mut *conn) {
                Ok(seq) => Some(seq),
                Err(diesel::result::Error::NotFound) => {
                    return bad_request(
                        maud! { "Source debate not found" }.render(),
                    );
                }
                Err(e) => {
                    return bad_request(maud! { (e.to_string()) }.render());
                }
            }
        }
        None => None,
    };
    let target_round_seq = match to_debate_id {
        Some(debate_id) => {
            match debate_round_seq(debate_id, &tournament.id, &mut *conn) {
                Ok(seq) => Some(seq),
                Err(diesel::result::Error::NotFound) => {
                    return bad_request(
                        maud! { "Target debate not found" }.render(),
                    );
                }
                Err(e) => {
                    return bad_request(maud! { (e.to_string()) }.render());
                }
            }
        }
        None => None,
    };

    let affected_round_seq = match (source_round_seq, target_round_seq) {
        (Some(source), Some(target)) if source == target => source,
        (Some(_), Some(_)) => {
            return bad_request(
                maud! { "Source and target debates are not in the same round sequence" }
                    .render(),
            );
        }
        (Some(source), None) => source,
        (None, Some(target)) => target,
        (None, None) => {
            return bad_request(
                maud! { "No source or target debate specified" }.render(),
            );
        }
    };

    let round_ids =
        match round_ids_for_seq(&tournament.id, affected_round_seq, &mut *conn)
        {
            Ok(round_ids) => round_ids,
            Err(e) => {
                return bad_request(maud! { (e.to_string()) }.render());
            }
        };

    let conflict_redirect = || {
        see_other_ok(Redirect::to(&edit_path_with_error(
            &tournament_id,
            &round_ids,
            "conflict",
        )))
    };

    let transaction_result = conn.transaction(|conn| {
        let current_source_debate_id = current_debate_for_judge_in_seq(
            &judge.id,
            &tournament.id,
            affected_round_seq,
            conn,
        )?;

        let current_source_panel =
            if let Some(debate_id) = current_source_debate_id.as_deref() {
                panel_snapshot_for_debate_id(debate_id, conn)?
            } else {
                panel_snapshot_for_unallocated(
                    &tournament.id,
                    affected_round_seq,
                    conn,
                )?
            };

        let current_target_panel = if let Some(debate_id) = to_debate_id {
            panel_snapshot_for_debate_id(debate_id, conn)?
        } else {
            panel_snapshot_for_unallocated(
                &tournament.id,
                affected_round_seq,
                conn,
            )?
        };

        if form.change_key.as_deref().filter(|s| !s.is_empty())
            != Some(
                panel_change_key(&current_source_panel, &current_target_panel)
                    .as_str(),
            )
        {
            return Err(diesel::result::Error::RollbackTransaction);
        }

        diesel::delete(
            judges_of_debate::table.filter(
                judges_of_debate::judge_id.eq(&judge.id).and(
                    judges_of_debate::debate_id.eq_any(
                        debates::table
                            .inner_join(
                                rounds::table
                                    .on(rounds::id.eq(debates::round_id)),
                            )
                            .filter(rounds::tournament_id.eq(&tournament.id))
                            .filter(rounds::seq.eq(affected_round_seq))
                            .select(debates::id),
                    ),
                ),
            ),
        )
        .execute(conn)?;

        if let Some(to_debate_id) = to_debate_id {
            let debate = match debates::table
                .filter(
                    debates::id
                        .eq(to_debate_id)
                        .and(debates::tournament_id.eq(&tournament.id)),
                )
                .first::<Debate>(conn)
                .optional()?
            {
                Some(d) => d,
                None => {
                    return Err(diesel::result::Error::NotFound);
                }
            };

            let role = match Role::of_str(&form.role) {
                Ok(role) => role,
                Err(e) => {
                    return Err(diesel::result::Error::QueryBuilderError(
                        e.into(),
                    ));
                }
            };

            if role == Role::Chair {
                diesel::update(
                    judges_of_debate::table.filter(
                        judges_of_debate::debate_id
                            .eq(&debate.id)
                            .and(judges_of_debate::status.eq("C")),
                    ),
                )
                .set(judges_of_debate::status.eq(Role::Panelist.to_string()))
                .execute(conn)?;
            }

            diesel::insert_into(judges_of_debate::table)
                .values((
                    judges_of_debate::id.eq(Uuid::now_v7().to_string()),
                    judges_of_debate::debate_id.eq(debate.id),
                    judges_of_debate::judge_id.eq(judge.id),
                    judges_of_debate::status.eq(role.to_string()),
                    judges_of_debate::tournament_id.eq(tournament.id.clone()),
                ))
                .execute(conn)?;
        }
        Ok(())
    });

    match transaction_result {
        Ok(()) => {}
        Err(diesel::result::Error::RollbackTransaction) => {
            return conflict_redirect();
        }
        Err(diesel::result::Error::NotFound) => {
            return bad_request(maud! { "Debate not found" }.render());
        }
        Err(e) => {
            return bad_request(maud! { (e.to_string()) }.render());
        }
    }

    for round_id in &round_ids {
        let _ = tx.send(Msg {
            tournament: tournament.clone(),
            inner: MsgContents::DrawUpdated(round_id.clone()),
        });
    }

    see_other_ok(Redirect::to(&edit_path_for_tournament_id(
        &tournament_id,
        &round_ids,
    )))
}

#[derive(Deserialize, Debug)]
pub struct MoveRoomForm {
    room_id: String,
    from_debate_id: Option<String>,
    to_debate_id: Option<String>,
    rounds: Vec<String>,
}

pub async fn move_room(
    Path(tournament_id): Path<String>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<MoveRoomForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round_ids = form.rounds;
    if round_ids.is_empty() {
        return bad_request(maud! { "No rounds specified" }.render());
    }

    let room = match rooms::table
        .filter(rooms::id.eq(&form.room_id))
        .filter(rooms::tournament_id.eq(&tournament.id))
        .first::<Room>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(room) => room,
        None => return bad_request(maud! { "Room not found" }.render()),
    };

    let to_debate_id = form.to_debate_id.as_deref().filter(|s| !s.is_empty());
    let _submitted_source_debate_id =
        form.from_debate_id.as_deref().filter(|s| !s.is_empty());

    let transaction_result = conn.transaction(|conn| {
        let source_debate = debates::table
            .filter(debates::tournament_id.eq(&tournament.id))
            .filter(debates::round_id.eq_any(&round_ids))
            .filter(debates::room_id.eq(Some(room.id.clone())))
            .first::<Debate>(conn)
            .optional()?;

        let target_debate = match to_debate_id {
            Some(to_debate_id) => Some(
                debates::table
                    .filter(debates::tournament_id.eq(&tournament.id))
                    .filter(debates::round_id.eq_any(&round_ids))
                    .filter(debates::id.eq(to_debate_id))
                    .first::<Debate>(conn)?,
            ),
            None => None,
        };

        if source_debate.is_none() && target_debate.is_none() {
            return Err(diesel::result::Error::NotFound);
        }

        if source_debate.as_ref().map(|d| d.id.as_str()) == to_debate_id {
            return Ok(());
        }

        let displaced_room_id = target_debate
            .as_ref()
            .and_then(|debate| debate.room_id.clone());

        diesel::update(
            debates::table
                .filter(debates::tournament_id.eq(&tournament.id))
                .filter(debates::round_id.eq_any(&round_ids))
                .filter(debates::room_id.eq(Some(room.id.clone()))),
        )
        .set(debates::room_id.eq(None::<String>))
        .execute(conn)?;

        if let (Some(source_debate), Some(displaced_room_id)) =
            (&source_debate, displaced_room_id.as_ref())
        {
            if displaced_room_id != &room.id {
                diesel::update(
                    debates::table.filter(debates::id.eq(&source_debate.id)),
                )
                .set(debates::room_id.eq(Some(displaced_room_id.clone())))
                .execute(conn)?;
            }
        }

        if let Some(target_debate) = target_debate {
            diesel::update(
                debates::table.filter(debates::id.eq(&target_debate.id)),
            )
            .set(debates::room_id.eq(Some(room.id.clone())))
            .execute(conn)?;
        }

        Ok(())
    });

    match transaction_result {
        Ok(()) => {}
        Err(diesel::result::Error::NotFound) => {
            return bad_request(maud! { "Debate not found" }.render());
        }
        Err(e) => return bad_request(maud! { (e.to_string()) }.render()),
    }

    for round_id in &round_ids {
        let _ = tx.send(Msg {
            tournament: tournament.clone(),
            inner: MsgContents::DrawUpdated(round_id.clone()),
        });
    }

    see_other_ok(Redirect::to(&edit_path_for_tournament_id(
        &tournament_id,
        &round_ids,
    )))
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

    let role = Role::of_str(&form.role).unwrap_or(Role::Panelist);

    diesel::update(
        judges_of_debate::table.filter(
            judges_of_debate::judge_id
                .eq(&form.judge_id)
                .and(judges_of_debate::debate_id.eq(&form.debate_id)),
        ),
    )
    .set(judges_of_debate::status.eq(role.to_string()))
    .execute(&mut *conn)
    .unwrap();

    for round_id in &round_ids {
        let _ = tx.send(Msg {
            tournament: tournament.clone(),
            inner: MsgContents::DrawUpdated(round_id.clone()),
        });
    }

    see_other_ok(Redirect::to(&edit_path_for_tournament_id(
        &tournament_id,
        &round_ids,
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
        let team1_debate_team = teams_of_debate::table
            .filter(teams_of_debate::team_id.eq(&form.team1_id))
            .filter(teams_of_debate::tournament_id.eq(&tournament.id))
            .filter(
                teams_of_debate::debate_id.eq_any(
                    debates::table
                        .filter(debates::round_id.eq_any(&round_ids))
                        .select(debates::id),
                ),
            )
            .first::<DebateTeam>(conn)?;

        let team2_debate_team = teams_of_debate::table
            .filter(teams_of_debate::team_id.eq(&form.team2_id))
            .filter(teams_of_debate::tournament_id.eq(&tournament.id))
            .filter(
                teams_of_debate::debate_id.eq_any(
                    debates::table
                        .filter(debates::round_id.eq_any(&round_ids))
                        .select(debates::id),
                ),
            )
            .first::<DebateTeam>(conn)?;

        let temp_debate_id = team1_debate_team.debate_id.clone();
        let temp_side = team1_debate_team.side;
        let temp_seq = team1_debate_team.seq;

        diesel::update(
            teams_of_debate::table
                .filter(teams_of_debate::id.eq(&team1_debate_team.id)),
        )
        .set((
            teams_of_debate::debate_id.eq(team2_debate_team.debate_id.clone()),
            teams_of_debate::side.eq(team2_debate_team.side),
            teams_of_debate::seq.eq(team2_debate_team.seq),
        ))
        .execute(conn)?;

        diesel::update(
            teams_of_debate::table
                .filter(teams_of_debate::id.eq(&team2_debate_team.id)),
        )
        .set((
            teams_of_debate::debate_id.eq(temp_debate_id),
            teams_of_debate::side.eq(temp_side),
            teams_of_debate::seq.eq(temp_seq),
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
