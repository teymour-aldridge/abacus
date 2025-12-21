use axum::extract::Path;
use hypertext::prelude::*;
use itertools::Itertools;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{TournamentRounds, draws::DebateRepr},
    },
    util_resp::{StandardResponse, success},
};

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
    let ballots = debate.ballots(&mut *conn);

    let missing_ballots = debate
        .judges
        .clone()
        .into_iter()
        .filter(|judge| {
            ballots
                .iter()
                .all(|ballot| ballot.ballot.judge_id != (judge.1.id))
        })
        .collect_vec();

    let problems = {
        let mut problems = Vec::new();
        for ballot in &ballots {
            for other_ballot in &ballots {
                if ballot.ballot.id == other_ballot.ballot.id {
                    continue;
                }

                problems.extend(
                    ballot.get_human_readable_description_for_problems(
                        &other_ballot,
                        &tournament,
                        &debate,
                    ),
                )
            }
        }
        problems
    };

    let round = crate::tournaments::rounds::Round::fetch(
        &debate.debate.round_id,
        &mut *conn,
    )
    .unwrap();

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper rounds=(&all_rounds) tournament=(&tournament) active_page=(Some("ballots")) selected_seq=(Some(round.seq)) {
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
                                h2 class="h4 text-uppercase fw-bold text-secondary mb-4" {
                                    "Ballot from " (judge.name) " (" (judge_role) ")"
                                }
                                p style="font-size: 0.875rem; color: var(--bs-gray-600); margin-bottom: 1.5rem;" {
                                    "Submitted at " (ballot.ballot().submitted_at.format("%Y-%m-%d %H:%M:%S").to_string())
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

                                                    @let position_name = match (tournament.teams_per_side, debate_team.side, debate_team.seq, score.speaker_position) {
                                                        (1, 0, 0, 0) => "PM",
                                                        (1, 0, 0, 1) => "DPM",
                                                        (1, 1, 0, 0) => "LO",
                                                        (1, 1, 0, 1) => "DLO",
                                                        (2, 0, 0, 0) => "PM",
                                                        (2, 0, 0, 1) => "DPM",
                                                        (2, 1, 0, 0) => "LO",
                                                        (2, 1, 0, 1) => "DLO",
                                                        (2, 0, 1, 0) => "MG",
                                                        (2, 0, 1, 1) => "GW",
                                                        (2, 1, 1, 0) => "MO",
                                                        (2, 1, 1, 1) => "OW",
                                                        _ => "Speaker",
                                                    };

                                                    li class="list-group-item d-flex justify-content-between align-items-center" style="border-left: 2px solid var(--bs-gray-900); border-right: none; border-top: 1px solid var(--bs-gray-200); border-bottom: none;" {
                                                        div {
                                                            strong style="font-weight: 600; margin-right: 0.5rem;" { (position_name) }
                                                            span style="color: var(--bs-gray-900);" { (speaker.name) }
                                                        }
                                                        span class="badge bg-dark" {
                                                            (score.score)
                                                        }
                                                    }
                                                }

                                                @let total: f32 = ballot.scores_of_team(&team_id).iter().map(|s| s.score).sum();
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
                }
            })
            .render(),
    )
}
