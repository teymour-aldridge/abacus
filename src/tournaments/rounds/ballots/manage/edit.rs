//! Code to edit a ballot set.

use axum::{extract::Path, response::Redirect};
use axum_extra::extract::Form;
use hypertext::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::task::yield_now;
use tracing::{Instrument, Level};
use uuid::Uuid;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        rounds::{
            Motion,
            ballots::{
                Ballot, BallotRepr, BallotScore,
                form_components::{
                    MotionSelector, SpeakerInput, get_score_bounds,
                },
            },
            draws::DebateRepr,
        },
    },
    util_resp::{FailureResponse, StandardResponse, see_other_ok, success},
};

pub async fn edit_ballot_set_page(
    Path((tournament_id, debate_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let debate = DebateRepr::try_fetch(&debate_id, &mut *conn)?;
    let current_ballots_of_debate = debate.ballots(&mut *conn);

    let problems: Vec<String> = BallotRepr::problems_of_set(
        &current_ballots_of_debate,
        &tournament,
        &debate,
    );

    success(
        Page::new()
            .user(user)
            .body(EditBallotSetForm {
                tournament,
                debate,
                ballots: current_ballots_of_debate,
                problems,
            })
            .render(),
    )
}

struct EditBallotSetForm {
    tournament: Tournament,
    debate: DebateRepr,
    ballots: Vec<BallotRepr>,
    problems: Vec<String>,
}

impl hypertext::Renderable for EditBallotSetForm {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            div class="container py-5" style="max-width: 1200px;" {
                @if !self.problems.is_empty() {
                    div class="alert alert-danger" role="alert" {
                        h4 class="alert-heading" { "Problems with this ballot set:" }
                        ul class="mb-0" {
                            @for problem in &self.problems {
                                li { (problem) }
                            }
                        }
                    }
                }

                header class="mb-5" {
                    h1 class="display-4 fw-bold mb-3" { "Edit Ballot Set" }
                    span class="badge bg-light text-dark" {
                        "Round " (self.debate.debate.round_id)
                    }
                }

                form method="post" {
                    @for (ballot_idx, ballot) in self.ballots.iter().enumerate() {
                        @let judge = self.debate.judges.get(&ballot.ballot().judge_id).unwrap();

                        div class="card mb-4" {
                            div class="card-header bg-primary text-white" {
                                h3 class="h5 mb-0" { "Ballot from " (judge.name) }
                            }
                            div class="card-body" {
                                input type="hidden" name=(format!("ballots[{}].judge_id", ballot_idx)) value=(ballot.ballot().judge_id);

                                @if self.debate.motions.len() > 1 {
                                    @let motions_vec: Vec<Motion> = self.debate.motions.values().cloned().collect();
                                    (MotionSelector {
                                        motions: &motions_vec,
                                        selected_motion_id: Some(&ballot.ballot().motion_id),
                                        field_name: &format!("ballots[{}].motion_id", ballot_idx),
                                    })
                                } @else {
                                    @let motion_id = self.debate.motions.keys().next().unwrap();
                                    input type="hidden" name=(format!("ballots[{}].motion_id", ballot_idx)) value=(motion_id);
                                }

                                @let substantive_speakers = self.tournament.substantive_speakers as usize;
                                @let reply_speakers = if self.tournament.reply_speakers { 1 } else { 0 };
                                @let total_speakers = substantive_speakers + reply_speakers;

                                section class="mb-5" {
                                    h2 class="h4 text-uppercase fw-bold text-secondary mb-4" {
                                        "Speaker Scores"
                                    }

                                    @for side in 0..2 {
                                        @for seq in 0..self.tournament.teams_per_side {
                                            @let team_idx = 2 * (seq as usize) + side;
                                            @let debate_team = &self.debate.teams_of_debate[team_idx];
                                            @let team = self.debate.teams.get(&debate_team.team_id).unwrap();
                                            @let scores = ballot.scores_of_team(&debate_team.team_id);
                                            @let speakers = self.debate.speakers_of_team.get(&debate_team.team_id).unwrap();

                                            div class="mb-4" {
                                                h3 class="h6 text-uppercase fw-bold text-secondary" { (team.name) }

                                                @for speaker_row in 0..total_speakers {
                                                    @let score = scores.get(speaker_row);
                                                    @let is_reply = speaker_row >= substantive_speakers;
                                                    @let (min_score, max_score) = get_score_bounds(&self.tournament, is_reply);

                                                    (SpeakerInput {
                                                        team_name: &team.name,
                                                        speaker_position: speaker_row,
                                                        speakers,
                                                        selected_speaker_id: score.map(|s| s.speaker_id.as_str()),
                                                        score: score.map(|s| s.score),
                                                        min_score,
                                                        max_score,
                                                        score_step: self.tournament.substantive_speech_step,
                                                        speaker_field_name: &format!("ballots[{}].teams[{}].speakers[{}].id", ballot_idx, team_idx, speaker_row),
                                                        score_field_name: &format!("ballots[{}].teams[{}].speakers[{}].score", ballot_idx, team_idx, speaker_row),
                                                    })
                                                }

                                                input type="hidden" name=(format!("ballots[{}].teams[{}].id", ballot_idx, team_idx)) value=(debate_team.team_id);
                                            }

                                            @if seq < self.tournament.teams_per_side - 1 {
                                                hr class="my-3";
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    button type="submit" class="btn btn-primary btn-lg" {
                        "Save Changes"
                    }
                }
            }
        }
        .render_to(buffer);
    }
}

#[derive(Serialize, Deserialize)]
pub struct Speaker {
    pub id: String,
    pub score: f32,
}

#[derive(Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub speakers: Vec<Speaker>,
}

#[derive(Serialize, Deserialize)]
pub struct SingleBallot {
    pub teams: Vec<Team>,
    pub judge_id: String,
    pub motion_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct EditSetOfBallotsOfOneDebateForm {
    pub ballots: Vec<SingleBallot>,
}

// todo: we should guard against the ABA problem here
pub async fn do_edit_ballot_set(
    Path((tournament_id, debate_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<EditSetOfBallotsOfOneDebateForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let debate = DebateRepr::try_fetch(&debate_id, &mut *conn)?;
    let current_ballots_of_debate = debate.ballots(&mut *conn);

    let mut new_ballots = Vec::new();

    for ballot in form.ballots {
        let extant_ballot =
            match current_ballots_of_debate.iter().find(|extant_ballot| {
                extant_ballot.ballot().judge_id == ballot.judge_id
            }) {
                Some(ballot) => ballot,
                None => {
                    // todo: add error message
                    return Err(FailureResponse::BadRequest(maud! {}.render()));
                }
            };

        let potential_new_ballot_id = Uuid::now_v7().to_string();
        let potential_new_ballot = BallotRepr {
            ballot: Ballot {
                id: potential_new_ballot_id.clone(),
                tournament_id: tournament_id.clone(),
                debate_id: debate_id.clone(),
                judge_id: ballot.judge_id,
                submitted_at: chrono::Utc::now().naive_utc(),
                motion_id: ballot.motion_id,
                version: 1,
                // we'll edit these later once we determine if this ballot is
                // different from the previously present one
                change: None,
                editor_id: None,
            },
            scores: {
                ballot
                    .teams
                    .iter()
                    .map(|team| {
                        team.speakers.iter().enumerate().map(|(i, speaker)| {
                            BallotScore {
                                id: Uuid::now_v7().to_string(),
                                tournament_id: tournament_id.clone(),
                                ballot_id: potential_new_ballot_id.clone(),
                                team_id: team.id.clone(),
                                speaker_id: speaker.id.clone(),
                                speaker_position: i as i64,
                                score: speaker.score,
                            }
                        })
                    })
                    .flatten()
                    .collect()
            },
        };

        if !potential_new_ballot.is_isomorphic(
            extant_ballot,
            &tournament,
            &debate,
        ) {
            new_ballots.push(BallotRepr {
                ballot: Ballot {
                    change: Some(
                        extant_ballot
                            .get_human_readable_description_for_problems(
                                &potential_new_ballot,
                                &tournament,
                                &debate,
                            )
                            .join(", "),
                    ),
                    version: extant_ballot.ballot().version + 1,
                    editor_id: Some(user.id.clone()),
                    ..potential_new_ballot.ballot
                },
                scores: potential_new_ballot.scores.clone(),
            })
        }
    }

    {
        let span = tracing::span!(Level::TRACE, "insert_new_ballots");

        async {
            for ballot in new_ballots {
                ballot.insert(&mut *conn);
                yield_now().await;
            }
        }
        .instrument(span)
        .await;
    }

    // todo: should we redirect to the ballot set, or back to the list of all
    // the ballots?
    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/ballots/{}",
        tournament_id, debate_id
    )))
}
