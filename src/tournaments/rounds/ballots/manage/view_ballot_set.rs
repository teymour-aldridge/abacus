use axum::extract::Path;
use diesel::prelude::*;
use hypertext::prelude::*;
use itertools::Itertools;
use std::collections::HashMap;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::{SidebarPage, SidebarWrapper},
        rounds::{TournamentRounds, ballots::BallotRepr, draws::DebateRepr},
    },
    util_resp::{StandardResponse, success},
};

#[tracing::instrument(skip(conn))]
/// Displays the ballot set for an individual debate.
pub async fn view_ballot_set_page(
    Path((tournament_id, debate_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let all_rounds =
        TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let debate = DebateRepr::fetch(&debate_id, &mut *conn);
    let ballots = debate.latest_ballots(&mut *conn);

    let missing_ballots = debate
        .judges
        .clone()
        .into_iter()
        .filter(|judge| {
            ballots
                .iter()
                .all(|ballot| ballot.metadata.judge_id != (judge.1.id))
        })
        .collect_vec();

    let problems = BallotRepr::problems_of_set(&ballots, &tournament, &debate);

    let round = crate::tournaments::rounds::Round::fetch(
        &debate.debate.round_id,
        &mut *conn,
    )
    .unwrap();

    let history = debate.ballot_history(&mut *conn);
    let grouped_history = history
        .into_iter()
        .into_group_map_by(|b| b.metadata.judge_id.clone());

    // Look up editor usernames for ballots submitted via admin override
    let editor_ids: Vec<String> = grouped_history
        .values()
        .flatten()
        .filter_map(|b| b.metadata.editor_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let editor_names: HashMap<String, String> = if !editor_ids.is_empty() {
        crate::schema::users::table
            .filter(crate::schema::users::id.eq_any(&editor_ids))
            .select((crate::schema::users::id, crate::schema::users::username))
            .load::<(String, String)>(&mut *conn)
            .unwrap_or_default()
            .into_iter()
            .collect()
    } else {
        HashMap::new()
    };

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper
                    rounds=(&all_rounds)
                    tournament=(&tournament)
                    active_page=
                        (
                            Some(SidebarPage::Ballots)
                        )
                    selected_seq=(Some(round.seq))
                {

                    div class="container py-5" style="max-width: 800px;" {
                        header class="mb-5" {
                            h1 class="display-4 fw-bold mb-3" {
                                "Ballots for Debate " (debate.debate.number)
                            }
                        }

                        @if !missing_ballots.is_empty() {
                            div class="alert alert-warning" style="margin-bottom: 2rem;" {
                                h4 class="alert-heading" { "Missing Ballots" }
                                p { "The following judges have not yet submitted their ballots:" }
                                ul class="mb-0" {
                                    @for (_, judge) in &missing_ballots {
                                        @let judge_role = debate.judges_of_debate
                                            .iter()
                                            .find(|dj| dj.judge_id == judge.id)
                                            .map(|dj| match dj.status.as_str() {
                                                "C" => "Chair",
                                                "P" => "Panelist",
                                                "T" => "Trainee",
                                                _ => "Judge",
                                            })
                                            .unwrap_or("Judge");
                                        li { (judge.name) " (" (judge_role) ")" }
                                    }
                                }
                            }
                        }

                        @if !problems.is_empty() {
                            div class="alert alert-danger" style="margin-bottom: 2rem;" {
                                h4 class="alert-heading" { "Ballot Problems" }
                                p { "The following problems were detected with the submitted ballots:" }
                                ul class="mb-0" {
                                    @for problem in &problems {
                                        li { (problem) }
                                    }
                                }
                            }
                        }

                        h2 class="display-6 fw-bold mb-4" { "Merged View (Latest Ballots)" }
                        p class="text-muted mb-4" { "This section shows the latest ballot version submitted by each judge. These are the ballots used for consensus aggregation and to determine if a round is complete." }

                        @for ballot in &ballots {
                            @let judge = debate.judges.get(&ballot.ballot().judge_id).unwrap();
                            @let judge_role = debate.judges_of_debate
                                .iter()
                                .find(|dj| dj.judge_id == ballot.ballot().judge_id)
                                .map(|dj| match dj.status.as_str() {
                                    "C" => "Chair",
                                    "P" => "Panelist",
                                    "T" => "Trainee",
                                    _ => "Judge",
                                })
                                .unwrap_or("Judge");

                            section class="mb-5" {
                                div class="d-flex justify-content-between align-items-center mb-4" {
                                    div {
                                        h3 class="h4 text-uppercase fw-bold text-secondary mb-1" {
                                            "Ballot from " (judge.name) " (" (judge_role) ")"
                                        }
                                        p style="font-size: 0.875rem; color: var(--bs-gray-600); margin-bottom: 0;" {
                                            "Version " (ballot.metadata.version) " — Submitted at " (ballot.ballot().submitted_at.format("%Y-%m-%d %H:%M:%S").to_string())
                                        }
                                    }
                                    a href=(format!("/tournaments/{}/debates/{}/judges/{}/edit", tournament.id, debate.debate.id, judge.id)) class="btn btn-primary" {
                                        "Edit Ballot"
                                    }
                                }

                                div class="row" {
                                    @for team_id in ballot.teams() {
                                        @let debate_team = debate.teams_of_debate.iter().find(|dt| dt.team_id == *team_id).unwrap();
                                        @let short_name = crate::tournaments::rounds::side_names::name_of_side(&tournament, debate_team.side, debate_team.seq, true);
                                        @let full_team_name = debate.teams.get(&team_id).unwrap().name.clone();

                                        div class="col-md-6 mb-3" {
                                            ul class="list-group" {
                                                @for score in ballot.scores_of_team(&team_id) {
                                                    @let speaker = debate
                                                        .speakers_of_team
                                                        .get(&team_id)
                                                        .unwrap_or_else(|| {
                                                            panic!("Failed to find speakers for team_id: {}", team_id)
                                                        })
                                                        .iter()
                                                        .find(|s| s.id == score.speaker_id)
                                                        .unwrap();

                                                    @let position_name =
                                                        tournament.speaker_position_name(
                                                            debate_team.side,
                                                            debate_team.seq,
                                                            score.speaker_position
                                                        );

                                                    li class="list-group-item d-flex justify-content-between align-items-center" style="border-left: 2px solid var(--bs-gray-900); border-right: none; border-top: 1px solid var(--bs-gray-200); border-bottom: none;" {
                                                        div {
                                                            strong style="font-weight: 600; margin-right: 0.5rem;" { (position_name) }
                                                            span style="color: var(--bs-gray-900);" { (speaker.name) }
                                                        }
                                                        @if let Some(s) = score.score {
                                                            span class="badge bg-dark" {
                                                                (s)
                                                            }
                                                        } @else {
                                                            span class="badge bg-secondary" {
                                                                "-"
                                                            }
                                                        }
                                                    }
                                                }

                                                @let total: f32 = ballot.scores_of_team(&team_id).iter().filter_map(|s| s.score).sum();
                                                li class="list-group-item d-flex justify-content-between align-items-center bg-light" style="border-left: 2px solid var(--bs-gray-900); border-right: none; border-top: 2px solid var(--bs-gray-900); border-bottom: 1px solid var(--bs-gray-200);" {
                                                    em style="color: var(--bs-gray-700); font-size: 0.875rem;" {
                                                        "Total for " (full_team_name) " (" (short_name) ")"
                                                    }
                                                    span class="badge bg-dark" {
                                                        (total)
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        @for (_, judge) in &missing_ballots {
                            @let judge_role = debate.judges_of_debate
                                .iter()
                                .find(|dj| dj.judge_id == judge.id)
                                .map(|dj| match dj.status.as_str() {
                                    "C" => "Chair",
                                    "P" => "Panelist",
                                    "T" => "Trainee",
                                    _ => "Judge",
                                })
                                .unwrap_or("Judge");

                            section class="mb-5" {
                                div class="d-flex justify-content-between align-items-center mb-4" {
                                    div {
                                        h3 class="h4 text-uppercase fw-bold text-secondary mb-1" {
                                            "Ballot from " (judge.name) " (" (judge_role) ")"
                                        }
                                        p style="font-size: 0.875rem; color: var(--bs-gray-600); margin-bottom: 0;" {
                                            "No ballot submitted"
                                        }
                                    }
                                    a href=(format!("/tournaments/{}/debates/{}/judges/{}/edit", tournament.id, debate.debate.id, judge.id)) class="btn btn-success" {
                                        "Create Ballot"
                                    }
                                }
                            }
                        }

                        hr class="my-5";

                        h2 class="display-6 fw-bold mb-4" { "History of Ballots" }
                        p class="text-muted mb-4" { "This section displays the full timeline of ballots submitted for this debate by every judge." }

                        @for (judge_id, judge_ballots) in &grouped_history {
                            @let judge = debate.judges.get(judge_id).unwrap();
                            @let judge_role = debate.judges_of_debate
                                .iter()
                                .find(|dj| &dj.judge_id == judge_id)
                                .map(|dj| match dj.status.as_str() {
                                    "C" => "Chair",
                                    "P" => "Panelist",
                                    "T" => "Trainee",
                                    _ => "Judge",
                                })
                                .unwrap_or("Judge");

                            div class="card mb-4" {
                                div class="card-header bg-light" {
                                    h4 class="h5 mb-0" { (judge.name) " (" (judge_role) ")" }
                                }
                                ul class="list-group list-group-flush" {
                                    @for ballot in judge_ballots {
                                        li class="list-group-item d-flex justify-content-between align-items-center p-3" {
                                            div {
                                                h5 class="mb-1" { "Version " (ballot.metadata.version) }
                                                small class="text-muted" { 
                                                    "Submitted at " (ballot.ballot().submitted_at.format("%Y-%m-%d %H:%M:%S").to_string())
                                                    @if let Some(editor_id) = &ballot.metadata.editor_id {
                                                        @let editor_name = editor_names.get(editor_id).map(|s| s.as_str()).unwrap_or("unknown");
                                                        " (edited by " (editor_name) ")"
                                                    }
                                                }
                                            }
                                            a href=(format!("/tournaments/{}/debates/{}/ballots/{}/view", tournament.id, debate.debate.id, ballot.metadata.id)) class="btn btn-sm btn-outline-secondary" {
                                                "View Ballot"
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

#[tracing::instrument(skip(conn))]
/// Displays a read-only historic version of a ballot.
pub async fn view_single_ballot_page(
    Path((tournament_id, debate_id, ballot_id)): Path<(String, String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let all_rounds =
        TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let debate = DebateRepr::fetch(&debate_id, &mut *conn);
    let round = crate::tournaments::rounds::Round::fetch(
        &debate.debate.round_id,
        &mut *conn,
    )
    .unwrap();

    let ballot = BallotRepr::fetch(&ballot_id, &mut *conn);
    let judge = debate.judges.get(&ballot.ballot().judge_id).unwrap();
    let judge_role = debate
        .judges_of_debate
        .iter()
        .find(|dj| dj.judge_id == ballot.ballot().judge_id)
        .map(|dj| match dj.status.as_str() {
            "C" => "Chair",
            "P" => "Panelist",
            "T" => "Trainee",
            _ => "Judge",
        })
        .unwrap_or("Judge");

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper
                    rounds=(&all_rounds)
                    tournament=(&tournament)
                    active_page=(Some(SidebarPage::Ballots))
                    selected_seq=(Some(round.seq))
                {
                    div class="container py-5" style="max-width: 800px;" {
                        header class="mb-5" {
                            a href=(format!("/tournaments/{}/debates/{}/ballots", tournament.id, debate.debate.id)) class="btn btn-sm btn-outline-secondary mb-3" {
                                "← Back to Ballot Overview"
                            }
                            h1 class="display-4 fw-bold mb-2" {
                                "Ballot (Version " (ballot.metadata.version) ")"
                            }
                            h2 class="h4 text-muted" { 
                                "Debate " (debate.debate.number) " — " (judge.name) " (" (judge_role) ")" 
                            }
                            p class="text-muted mt-2" {
                                "Submitted at " (ballot.ballot().submitted_at.format("%Y-%m-%d %H:%M:%S").to_string())
                            }
                        }

                        section class="mb-5" {
                            div class="row" {
                                @for team_id in ballot.teams() {
                                    @let debate_team = debate.teams_of_debate.iter().find(|dt| dt.team_id == *team_id).unwrap();
                                    @let short_name = crate::tournaments::rounds::side_names::name_of_side(&tournament, debate_team.side, debate_team.seq, true);
                                    @let full_team_name = debate.teams.get(&team_id).unwrap().name.clone();

                                    div class="col-md-6 mb-3" {
                                        ul class="list-group" {
                                            @for score in ballot.scores_of_team(&team_id) {
                                                @let speaker = debate
                                                    .speakers_of_team
                                                    .get(&team_id)
                                                    .unwrap_or_else(|| {
                                                        panic!("Failed to find speakers for team_id: {}", team_id)
                                                    })
                                                    .iter()
                                                    .find(|s| s.id == score.speaker_id)
                                                    .unwrap();

                                                @let position_name =
                                                    tournament.speaker_position_name(
                                                        debate_team.side,
                                                        debate_team.seq,
                                                        score.speaker_position
                                                    );

                                                li class="list-group-item d-flex justify-content-between align-items-center" style="border-left: 2px solid var(--bs-gray-900); border-right: none; border-top: 1px solid var(--bs-gray-200); border-bottom: none;" {
                                                    div {
                                                        strong style="font-weight: 600; margin-right: 0.5rem;" { (position_name) }
                                                        span style="color: var(--bs-gray-900);" { (speaker.name) }
                                                    }
                                                    @if let Some(s) = score.score {
                                                        span class="badge bg-dark" {
                                                            (s)
                                                        }
                                                    } @else {
                                                        span class="badge bg-secondary" {
                                                            "-"
                                                        }
                                                    }
                                                }
                                            }

                                            @let total: f32 = ballot.scores_of_team(&team_id).iter().filter_map(|s| s.score).sum();
                                            li class="list-group-item d-flex justify-content-between align-items-center bg-light" style="border-left: 2px solid var(--bs-gray-900); border-right: none; border-top: 2px solid var(--bs-gray-900); border-bottom: 1px solid var(--bs-gray-200);" {
                                                em style="color: var(--bs-gray-700); font-size: 0.875rem;" {
                                                    "Total for " (full_team_name) " (" (short_name) ")"
                                                }
                                                span class="badge bg-dark" {
                                                    (total)
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
