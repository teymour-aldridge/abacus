use crate::{
    auth::User,
    schema::{
        judge_room_constraints, speaker_room_constraints,
        tournament_room_categories,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        participants::{Judge, Speaker},
        rooms::{JudgeRoomConstraint, RoomCategory, SpeakerRoomConstraint},
        rounds::TournamentRounds,
    },
    util_resp::{FailureResponse, StandardResponse, see_other_ok, success},
};
use axum::{Form, extract::Path, response::Redirect};
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq)]
enum ParticipantType {
    Speaker,
    Judge,
}

impl ParticipantType {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "speaker" | "speakers" => Some(ParticipantType::Speaker),
            "judge" | "judges" => Some(ParticipantType::Judge),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            ParticipantType::Speaker => "speaker",
            ParticipantType::Judge => "judge",
        }
    }
}

#[derive(Deserialize)]
pub struct MoveConstraintForm {
    category_id: String,
    direction: String, // "up" or "down"
}

#[derive(Deserialize)]
pub struct AddConstraintForm {
    category_id: String,
}

#[derive(Deserialize)]
pub struct RemoveConstraintForm {
    category_id: String,
}

struct ConstraintData {
    active_constraints: Vec<(RoomCategory, i64)>, // (category, preference)
    available_categories: Vec<RoomCategory>,
    all_categories: Vec<RoomCategory>,
}

fn fetch_speaker_constraints(
    speaker_id: &str,
    tid: &str,
    conn: &mut SqliteConnection,
) -> Result<ConstraintData, FailureResponse> {
    let all_categories = tournament_room_categories::table
        .filter(tournament_room_categories::tournament_id.eq(tid))
        .load::<RoomCategory>(conn)
        .map_err(FailureResponse::from)?;

    let constraints = speaker_room_constraints::table
        .filter(speaker_room_constraints::speaker_id.eq(speaker_id))
        .order_by(speaker_room_constraints::pref.asc())
        .load::<SpeakerRoomConstraint>(conn)
        .map_err(FailureResponse::from)?;

    let constrained_cat_ids: std::collections::HashSet<String> =
        constraints.iter().map(|c| c.category_id.clone()).collect();

    let available_categories: Vec<RoomCategory> = all_categories
        .iter()
        .filter(|c| !constrained_cat_ids.contains(&c.id))
        .cloned()
        .collect();

    let active_constraints: Vec<(RoomCategory, i64)> = constraints
        .iter()
        .map(|c| {
            let cat = all_categories
                .iter()
                .find(|cat| cat.id == c.category_id)
                .unwrap();
            (cat.clone(), c.pref)
        })
        .collect();

    Ok(ConstraintData {
        active_constraints,
        available_categories,
        all_categories,
    })
}

fn fetch_judge_constraints(
    judge_id: &str,
    tid: &str,
    conn: &mut SqliteConnection,
) -> Result<ConstraintData, FailureResponse> {
    let all_categories = tournament_room_categories::table
        .filter(tournament_room_categories::tournament_id.eq(tid))
        .load::<RoomCategory>(conn)
        .map_err(FailureResponse::from)?;

    let constraints = judge_room_constraints::table
        .filter(judge_room_constraints::judge_id.eq(judge_id))
        .order_by(judge_room_constraints::pref.asc())
        .load::<JudgeRoomConstraint>(conn)
        .map_err(FailureResponse::from)?;

    let constrained_cat_ids: std::collections::HashSet<String> =
        constraints.iter().map(|c| c.category_id.clone()).collect();

    let available_categories: Vec<RoomCategory> = all_categories
        .iter()
        .filter(|c| !constrained_cat_ids.contains(&c.id))
        .cloned()
        .collect();

    let active_constraints: Vec<(RoomCategory, i64)> = constraints
        .iter()
        .map(|c| {
            let cat = all_categories
                .iter()
                .find(|cat| cat.id == c.category_id)
                .unwrap();
            (cat.clone(), c.pref)
        })
        .collect();

    Ok(ConstraintData {
        active_constraints,
        available_categories,
        all_categories,
    })
}

pub async fn manage_constraints_page(
    Path((tid, participant_type, participant_id)): Path<(
        String,
        String,
        String,
    )>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let ptype = ParticipantType::from_str(&participant_type)
        .ok_or_else(|| FailureResponse::NotFound(()))?;

    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let current_rounds =
        crate::tournaments::rounds::Round::current_rounds(&tid, &mut *conn);

    let (participant_name, constraint_data) = match ptype {
        ParticipantType::Speaker => {
            use crate::schema::tournament_speakers;
            let speaker = tournament_speakers::table
                .filter(tournament_speakers::id.eq(&participant_id))
                .first::<Speaker>(&mut *conn)
                .map_err(FailureResponse::from)?;
            let data =
                fetch_speaker_constraints(&participant_id, &tid, &mut *conn)?;
            (speaker.name, data)
        }
        ParticipantType::Judge => {
            use crate::schema::tournament_judges;
            let judge = tournament_judges::table
                .filter(tournament_judges::id.eq(&participant_id))
                .first::<Judge>(&mut *conn)
                .map_err(FailureResponse::from)?;
            let data =
                fetch_judge_constraints(&participant_id, &tid, &mut *conn)?;
            (judge.name, data)
        }
    };

    success(
        Page::new()
            .active_nav("participants")
            .user(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds.clone())
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) selected_seq=(current_rounds.first().map(|r| r.seq)) active_page=(Some("participants")) {
                    div class="container-fluid px-3 px-md-4 py-3" {
                        // Header
                        div class="d-flex flex-column flex-sm-row justify-content-between align-items-start align-items-sm-center gap-2 border-bottom pb-3 mb-4" {
                            div {
                                h6 class="text-uppercase text-muted mb-1 small fw-bold" { "Room Constraints" }
                                h2 class="h5 fw-bold mb-0" { 
                                    (match ptype {
                                        ParticipantType::Speaker => "Speaker: ",
                                        ParticipantType::Judge => "Judge: ",
                                    })
                                    (participant_name) 
                                }
                            }
                            a href=(format!("/tournaments/{}/participants", tid)) class="btn btn-outline-secondary btn-sm" { "← Back" }
                        }

                        // Instructions
                        div class="alert alert-info mb-4" {
                            h6 class="alert-heading mb-2" { "How to manage room constraints:" }
                            ol class="mb-0 ps-3 small" {
                                li { "Add room categories from the list below" }
                                li { "Reorder them using ↑↓ buttons" }
                                li { "Top priority = most preferred room" }
                                li { "Changes are saved automatically" }
                            }
                        }

                        // Active Constraints Section
                        div class="mb-4" {
                            h5 class="mb-3 fw-bold" { "Room Preferences (in order)" }
                            
                            div class="list-group mb-3" {
                                @if constraint_data.active_constraints.is_empty() {
                                    div class="list-group-item text-muted text-center py-4 fst-italic" {
                                        "No room preferences set. Add categories below."
                                    }
                                } @else {
                                    @for (i, (cat, _)) in constraint_data.active_constraints.iter().enumerate() {
                                        div class="list-group-item d-flex align-items-center gap-2 p-2 p-sm-3" {
                                            // Priority number
                                            span class="badge bg-primary rounded-circle d-flex align-items-center justify-content-center" style="width: 32px; height: 32px; flex-shrink: 0;" {
                                                (i + 1)
                                            }
                                            
                                            // Category info
                                            div class="flex-grow-1 min-width-0" {
                                                div class="fw-bold text-truncate" { (cat.private_name) }
                                                div class="text-muted small text-truncate" { (cat.public_name) }
                                            }
                                            
                                            // Controls
                                            div class="d-flex gap-1 flex-shrink-0" {
                                                form method="post" action=(format!("/tournaments/{}/participants/{}/{}/constraints/move", tid, ptype.as_str(), participant_id)) class="d-inline" {
                                                    input type="hidden" name="category_id" value=(cat.id);
                                                    input type="hidden" name="direction" value="up";
                                                    button 
                                                        type="submit" 
                                                        class="btn btn-sm btn-outline-secondary" 
                                                        title="Move up"
                                                        disabled[i == 0]
                                                    {
                                                        "↑"
                                                    }
                                                }
                                                form method="post" action=(format!("/tournaments/{}/participants/{}/{}/constraints/move", tid, ptype.as_str(), participant_id)) class="d-inline" {
                                                    input type="hidden" name="category_id" value=(cat.id);
                                                    input type="hidden" name="direction" value="down";
                                                    button 
                                                        type="submit" 
                                                        class="btn btn-sm btn-outline-secondary" 
                                                        title="Move down"
                                                        disabled[i == constraint_data.active_constraints.len() - 1]
                                                    {
                                                        "↓"
                                                    }
                                                }
                                                form method="post" action=(format!("/tournaments/{}/participants/{}/{}/constraints/remove", tid, ptype.as_str(), participant_id)) class="d-inline" {
                                                    input type="hidden" name="category_id" value=(cat.id);
                                                    button type="submit" class="btn btn-sm btn-outline-danger" title="Remove" {
                                                        "✕"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Available Categories Section
                        div class="mb-4" {
                            h5 class="mb-3 fw-bold" { "Available Room Categories" }
                            
                            @if constraint_data.available_categories.is_empty() && !constraint_data.all_categories.is_empty() {
                                div class="alert alert-success small" {
                                    "All categories have been added to preferences."
                                }
                            }
                            @else if constraint_data.all_categories.is_empty() {
                                div class="alert alert-warning small" {
                                    "No room categories defined. Please create categories first."
                                }
                            }
                            @else {
                                div class="list-group" {
                                    @for cat in &constraint_data.available_categories {
                                        div class="list-group-item d-flex align-items-center gap-2 p-2 p-sm-3" {
                                            div class="flex-grow-1 min-width-0" {
                                                div class="fw-bold text-truncate" { (cat.private_name) }
                                                div class="text-muted small text-truncate" { (cat.public_name) }
                                            }
                                            form method="post" action=(format!("/tournaments/{}/participants/{}/{}/constraints/add", tid, ptype.as_str(), participant_id)) class="d-inline" {
                                                input type="hidden" name="category_id" value=(cat.id);
                                                button type="submit" class="btn btn-sm btn-primary flex-shrink-0" {
                                                    "+ Add"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .render(),
    )
}

pub async fn move_constraint(
    Path((tid, participant_type, participant_id)): Path<(
        String,
        String,
        String,
    )>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<MoveConstraintForm>,
) -> StandardResponse {
    let ptype = ParticipantType::from_str(&participant_type)
        .ok_or_else(|| FailureResponse::NotFound(()))?;

    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    conn.transaction(|conn| {
        match ptype {
            ParticipantType::Speaker => {
                let mut constraints = speaker_room_constraints::table
                    .filter(
                        speaker_room_constraints::speaker_id
                            .eq(&participant_id),
                    )
                    .order_by(speaker_room_constraints::pref.asc())
                    .load::<SpeakerRoomConstraint>(conn)?;

                if let Some(pos) = constraints
                    .iter()
                    .position(|c| c.category_id == form.category_id)
                {
                    let new_pos = match form.direction.as_str() {
                        "up" if pos > 0 => pos - 1,
                        "down" if pos < constraints.len() - 1 => pos + 1,
                        _ => pos,
                    };

                    if new_pos != pos {
                        let item = constraints.remove(pos);
                        constraints.insert(new_pos, item);

                        // Update all preferences
                        for (i, constraint) in constraints.iter().enumerate() {
                            diesel::update(speaker_room_constraints::table)
                                .filter(
                                    speaker_room_constraints::id
                                        .eq(&constraint.id),
                                )
                                .set(
                                    speaker_room_constraints::pref
                                        .eq((i + 1) as i64),
                                )
                                .execute(conn)?;
                        }
                    }
                }
            }
            ParticipantType::Judge => {
                let mut constraints = judge_room_constraints::table
                    .filter(
                        judge_room_constraints::judge_id.eq(&participant_id),
                    )
                    .order_by(judge_room_constraints::pref.asc())
                    .load::<JudgeRoomConstraint>(conn)?;

                if let Some(pos) = constraints
                    .iter()
                    .position(|c| c.category_id == form.category_id)
                {
                    let new_pos = match form.direction.as_str() {
                        "up" if pos > 0 => pos - 1,
                        "down" if pos < constraints.len() - 1 => pos + 1,
                        _ => pos,
                    };

                    if new_pos != pos {
                        let item = constraints.remove(pos);
                        constraints.insert(new_pos, item);

                        // Update all preferences
                        for (i, constraint) in constraints.iter().enumerate() {
                            diesel::update(judge_room_constraints::table)
                                .filter(
                                    judge_room_constraints::id
                                        .eq(&constraint.id),
                                )
                                .set(
                                    judge_room_constraints::pref
                                        .eq((i + 1) as i64),
                                )
                                .execute(conn)?;
                        }
                    }
                }
            }
        }

        Ok::<(), diesel::result::Error>(())
    })
    .map_err(FailureResponse::from)?;

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/participants/{}/{}/constraints",
        tid,
        ptype.as_str(),
        participant_id
    )))
}

pub async fn add_constraint(
    Path((tid, participant_type, participant_id)): Path<(
        String,
        String,
        String,
    )>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<AddConstraintForm>,
) -> StandardResponse {
    let ptype = ParticipantType::from_str(&participant_type)
        .ok_or_else(|| FailureResponse::NotFound(()))?;

    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    conn.transaction(|conn| {
        match ptype {
            ParticipantType::Speaker => {
                let max_pref = speaker_room_constraints::table
                    .filter(
                        speaker_room_constraints::speaker_id
                            .eq(&participant_id),
                    )
                    .select(diesel::dsl::max(speaker_room_constraints::pref))
                    .first::<Option<i64>>(conn)?
                    .unwrap_or(0);

                let constraint = SpeakerRoomConstraint {
                    id: uuid::Uuid::new_v4().to_string(),
                    speaker_id: participant_id.clone(),
                    category_id: form.category_id.clone(),
                    pref: max_pref + 1,
                };
                diesel::insert_into(speaker_room_constraints::table)
                    .values(&constraint)
                    .execute(conn)?;
            }
            ParticipantType::Judge => {
                let max_pref = judge_room_constraints::table
                    .filter(
                        judge_room_constraints::judge_id.eq(&participant_id),
                    )
                    .select(diesel::dsl::max(judge_room_constraints::pref))
                    .first::<Option<i64>>(conn)?
                    .unwrap_or(0);

                let constraint = JudgeRoomConstraint {
                    id: uuid::Uuid::new_v4().to_string(),
                    judge_id: participant_id.clone(),
                    category_id: form.category_id.clone(),
                    pref: max_pref + 1,
                };
                diesel::insert_into(judge_room_constraints::table)
                    .values(&constraint)
                    .execute(conn)?;
            }
        }

        Ok::<(), diesel::result::Error>(())
    })
    .map_err(FailureResponse::from)?;

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/participants/{}/{}/constraints",
        tid,
        ptype.as_str(),
        participant_id
    )))
}

pub async fn remove_constraint(
    Path((tid, participant_type, participant_id)): Path<(
        String,
        String,
        String,
    )>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<RemoveConstraintForm>,
) -> StandardResponse {
    let ptype = ParticipantType::from_str(&participant_type)
        .ok_or_else(|| FailureResponse::NotFound(()))?;

    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    conn.transaction(|conn| {
        match ptype {
            ParticipantType::Speaker => {
                diesel::delete(
                    speaker_room_constraints::table
                        .filter(
                            speaker_room_constraints::speaker_id
                                .eq(&participant_id),
                        )
                        .filter(
                            speaker_room_constraints::category_id
                                .eq(&form.category_id),
                        ),
                )
                .execute(conn)?;

                // Renumber remaining constraints
                let constraints = speaker_room_constraints::table
                    .filter(
                        speaker_room_constraints::speaker_id
                            .eq(&participant_id),
                    )
                    .order_by(speaker_room_constraints::pref.asc())
                    .load::<SpeakerRoomConstraint>(conn)?;

                for (i, constraint) in constraints.iter().enumerate() {
                    diesel::update(speaker_room_constraints::table)
                        .filter(speaker_room_constraints::id.eq(&constraint.id))
                        .set(speaker_room_constraints::pref.eq((i + 1) as i64))
                        .execute(conn)?;
                }
            }
            ParticipantType::Judge => {
                diesel::delete(
                    judge_room_constraints::table
                        .filter(
                            judge_room_constraints::judge_id
                                .eq(&participant_id),
                        )
                        .filter(
                            judge_room_constraints::category_id
                                .eq(&form.category_id),
                        ),
                )
                .execute(conn)?;

                // Renumber remaining constraints
                let constraints = judge_room_constraints::table
                    .filter(
                        judge_room_constraints::judge_id.eq(&participant_id),
                    )
                    .order_by(judge_room_constraints::pref.asc())
                    .load::<JudgeRoomConstraint>(conn)?;

                for (i, constraint) in constraints.iter().enumerate() {
                    diesel::update(judge_room_constraints::table)
                        .filter(judge_room_constraints::id.eq(&constraint.id))
                        .set(judge_room_constraints::pref.eq((i + 1) as i64))
                        .execute(conn)?;
                }
            }
        }

        Ok::<(), diesel::result::Error>(())
    })
    .map_err(FailureResponse::from)?;

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/participants/{}/{}/constraints",
        tid,
        ptype.as_str(),
        participant_id
    )))
}
