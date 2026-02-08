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
            Motion, Round,
            ballots::{
                BallotMetadata, BallotRepr, BallotScore,
                common_ballot_html::BallotFormFields,
                public::submit::{
                    build_team_ranks_from_advancing,
                    build_team_ranks_from_scores, num_advancing_for_elim_round,
                },
                update_debate_status,
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
    let round = Round::fetch(&debate.debate.round_id, &mut *conn)?;
    let current_ballots_of_debate = debate.ballots(&mut *conn);

    let problems = BallotRepr::problems_of_set(
        &current_ballots_of_debate,
        &tournament,
        &debate,
    );

    let num_advancing = if round.is_elim() {
        Some(num_advancing_for_elim_round(
            &tournament,
            &round,
            &mut *conn,
        ))
    } else {
        None
    };

    success(
        Page::new()
            .user(user)
            .body(maud! {
                div class="container py-5" style="max-width: 1200px;" {
                    @if !problems.is_empty() {
                        div class="alert alert-danger" role="alert" {
                            h4 class="alert-heading" { "Problems with this ballot set:" }
                            ul class="mb-0" {
                                @for problem in &problems {
                                    li { (problem) }
                                }
                            }
                        }
                    }

                    header class="mb-5" {
                        h1 class="display-4 fw-bold mb-3" { "Edit Ballot Set" }
                    }

                    form method="post" {
                        @let motions_vec: Vec<Motion> = debate.motions.values().cloned().collect();

                        @for (ballot_idx, ballot) in current_ballots_of_debate.iter().enumerate() {
                            @let judge = debate.judges.get(&ballot.ballot().judge_id).unwrap();

                            div class="card mb-4" {
                                div class="card-header bg-primary text-white" {
                                    h3 class="h5 mb-0" { "Ballot from " (judge.name) }
                                }
                                div class="card-body" {
                                    (BallotFormFields {
                                        tournament: &tournament,
                                        debate: &debate,
                                        round: &round,
                                        motions: &motions_vec,
                                        current_values: Some(ballot),
                                        field_prefix: &format!("ballots[{}].", ballot_idx),
                                        num_advancing,
                                    })
                                }
                            }
                        }

                        button type="submit" class="btn btn-primary btn-lg" {
                            "Save Changes"
                        }
                    }
                }
            })
            .render(),
    )
}

#[derive(Serialize, Deserialize)]
pub struct Speaker {
    pub id: String,
    pub score: Option<f32>,
}

#[derive(Serialize, Deserialize)]
pub struct Team {
    pub speakers: Vec<Speaker>,
}

#[derive(Serialize, Deserialize)]
pub struct SingleBallot {
    #[serde(default)]
    pub teams: Vec<Team>,
    pub motion_id: String,
    /// For elimination rounds with a single advancing team (radio button).
    #[serde(default)]
    pub winning_team_id: Option<String>,
    /// For elimination rounds with multiple advancing teams (checkboxes).
    #[serde(default)]
    pub advancing_team_ids: Vec<String>,
}

impl SingleBallot {
    /// Returns the complete list of advancing team IDs, merging the
    /// single-winner radio field with the multi-winner checkbox fields.
    pub fn all_advancing_team_ids(&self) -> Vec<String> {
        let mut ids = self.advancing_team_ids.clone();
        if let Some(ref id) = self.winning_team_id {
            if !id.is_empty() {
                ids.push(id.clone());
            }
        }
        ids
    }
}

#[derive(Serialize, Deserialize)]
pub struct EditSetOfBallotsOfOneDebateForm {
    pub ballots: Vec<SingleBallot>,
}

pub async fn do_edit_ballot_set(
    Path((tournament_id, debate_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<EditSetOfBallotsOfOneDebateForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let debate = DebateRepr::try_fetch(&debate_id, &mut *conn)?;
    let round = Round::fetch(&debate.debate.round_id, &mut *conn)?;
    let current_ballots_of_debate = debate.ballots(&mut *conn);
    let is_elim = round.is_elim();

    let records_positions = tournament.round_requires_speaker_order(&round);
    let records_scores = tournament.round_requires_speaks(&round);

    let mut new_ballots = Vec::new();

    for (ballot_idx, submitted_ballot) in form.ballots.iter().enumerate() {
        let extant_ballot = match current_ballots_of_debate.get(ballot_idx) {
            Some(ballot) => ballot,
            None => {
                return Err(FailureResponse::BadRequest(maud! {}.render()));
            }
        };

        let new_ballot_id = Uuid::now_v7().to_string();

        // Build scores from form data (only when recording positions)
        let scores = if records_positions {
            let mut scores = Vec::new();
            for (i, submitted_team) in submitted_ballot.teams.iter().enumerate()
            {
                let side = i % 2;
                let seq = i / 2;
                let team_id = &debate
                    .team_of_side_and_seq(side as i64, seq as i64)
                    .team_id;

                for (j, submitted_speaker) in
                    submitted_team.speakers.iter().enumerate()
                {
                    let score = if records_scores {
                        submitted_speaker.score
                    } else {
                        None
                    };

                    scores.push(BallotScore {
                        id: Uuid::now_v7().to_string(),
                        tournament_id: tournament_id.clone(),
                        ballot_id: new_ballot_id.clone(),
                        team_id: team_id.clone(),
                        speaker_id: submitted_speaker.id.clone(),
                        speaker_position: j as i64,
                        score,
                    });
                }
            }
            scores
        } else {
            Vec::new()
        };

        // Build team ranks
        let team_ranks = if is_elim {
            build_team_ranks_from_advancing(
                &submitted_ballot.all_advancing_team_ids(),
                &new_ballot_id,
                &tournament_id,
                &debate,
            )
        } else {
            build_team_ranks_from_scores(
                &scores,
                &new_ballot_id,
                &tournament_id,
                &debate,
                &tournament,
            )
        };

        let potential_new_ballot = BallotRepr::new_prelim(
            BallotMetadata {
                id: new_ballot_id,
                tournament_id: tournament_id.clone(),
                debate_id: debate_id.clone(),
                judge_id: extant_ballot.ballot().judge_id.clone(),
                submitted_at: chrono::Utc::now().naive_utc(),
                motion_id: submitted_ballot.motion_id.clone(),
                version: extant_ballot.ballot().version + 1,
                change: None,
                editor_id: None,
            },
            scores,
            team_ranks,
        );

        if !potential_new_ballot.is_isomorphic(
            extant_ballot,
            &tournament,
            &debate,
        ) {
            let change_description = extant_ballot
                .get_human_readable_description_for_problems(
                    &potential_new_ballot,
                    &tournament,
                    &debate,
                )
                .join(", ");

            let mut ballot = potential_new_ballot;
            ballot.metadata.change = Some(change_description);
            ballot.metadata.editor_id = Some(user.id.clone());
            new_ballots.push(ballot);
        }
    }

    {
        let span = tracing::span!(Level::TRACE, "insert_new_ballots");

        async {
            for ballot in &new_ballots {
                ballot.insert(&mut *conn);
                yield_now().await;
            }
        }
        .instrument(span)
        .await;
    }

    // Refresh debate repr and update status
    let debate = DebateRepr::fetch(&debate_id, &mut *conn);
    update_debate_status(&debate, &tournament, &mut *conn);

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/ballots/{}",
        tournament_id, debate_id
    )))
}
