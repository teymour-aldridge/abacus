use crate::tournaments::rounds::ballots::form::QsForm;
use axum::extract::Path;
use axum::response::Redirect;
use chrono::Utc;
use diesel::{connection::LoadConnection, sqlite::Sqlite};
use hypertext::{Renderable, maud, prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::{SidebarPage, SidebarWrapper},
        rounds::{
            Round, TournamentRounds,
            ballots::{
                BallotMetadata, BallotRepr, form::fields_of_single_ballot_form,
                update_debate_status,
            },
            draws::DebateRepr,
        },
    },
    util_resp::{StandardResponse, err_not_found, see_other_ok, success},
};

#[derive(Serialize, Deserialize)]
/// Ballot form used for judges to submit ballots, and for tab directors to
/// edit them behind the scenes.
///
/// Our HTML form logic requires that this form be parsed with [`serde_qs`]
/// rather than the standard axum (or axum_extra) extractors.
pub struct BallotForm {
    #[serde(default)]
    pub teams: Vec<BallotFormSingleTeamEntry>,
    pub motion_id: String,
    #[serde(default)]
    pub expected_version: i64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BallotFormSingleSpeakerEntry {
    pub id: String,
    pub score: Option<f32>,
}

#[derive(Serialize, Deserialize)]
pub struct BallotFormSingleTeamEntry {
    pub speakers: Vec<BallotFormSingleSpeakerEntry>,
    pub points: Option<usize>,
}

impl BallotForm {
    pub fn all_advancing_team_ids(&self, repr: &DebateRepr) -> Vec<String> {
        self.teams
            .iter()
            .enumerate()
            .filter(|(_, team)| {
                team.points.expect(
                    "should have validated that all teams[i].points.is_some() by now"
                ) == 1
            })
            .map(|(i, _)| repr.teams_of_debate[i].team_id.clone())
            .collect()
    }

    pub fn from_repr(
        repr: &BallotRepr,
        debate: &DebateRepr,
        tournament: &Tournament,
    ) -> Self {
        let mut teams = Vec::new();
        for side in 0..2 {
            for seq in 0..tournament.teams_per_side {
                let debate_team = debate.team_of_side_and_seq(side, seq);
                let team_id = &debate_team.team_id;

                let mut speakers = Vec::new();
                let team_scores = repr.scores_of_team(team_id);

                // Assuming scores are ordered by position if they exist, but we can't guarantee that in the DB.
                // It's safer to build a dummy list of empty speaker entries according to the size expected,
                // and then populate it.
                let mut score_map = std::collections::HashMap::new();
                for score in team_scores {
                    score_map.insert(score.speaker_position, score);
                }

                // Substantive speakers + optional reply speaker
                let speaker_count = tournament.substantive_speakers as i64
                    + if tournament.reply_speakers { 1 } else { 0 };
                for position in 0..speaker_count {
                    if let Some(score) = score_map.get(&position) {
                        speakers.push(BallotFormSingleSpeakerEntry {
                            id: score.speaker_id.clone(),
                            score: score.score,
                        });
                    } else {
                        // Normally this means the speaker wasn't filled out, or records_positions is false
                        // We push a dummy to keep arrays aligned with the form expectations
                        speakers.push(BallotFormSingleSpeakerEntry {
                            id: String::new(),
                            score: None,
                        });
                    }
                }

                let points = repr
                    .team_ranks
                    .iter()
                    .find(|tr| &tr.team_id == team_id)
                    .map(|tr| tr.points as usize);

                teams.push(BallotFormSingleTeamEntry { speakers, points });
            }
        }

        Self {
            teams,
            motion_id: repr.metadata.motion_id.clone(),
            expected_version: repr.metadata.version,
        }
    }
}

fn build_edit_ballot(
    form: BallotForm,
    tournament: &Tournament,
    round: &Round,
    debate_repr: &DebateRepr,
    participants: &crate::tournaments::participants::TournamentParticipants,
    new_metadata: BallotMetadata,
    expected_version: i64,
    prior_version: i64,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<BallotRepr, crate::util_resp::FailureResponse> {
    use crate::util_resp::bad_request_from_string;

    let mut builder = crate::tournaments::rounds::ballots::BallotBuilder::new(
        tournament,
        debate_repr,
        round,
        participants,
        new_metadata,
        expected_version,
        prior_version,
        conn,
    )
    .map_err(bad_request_from_string)?;

    for (i, submitted_team) in form.teams.into_iter().enumerate() {
        let side = i % 2;
        let seq = i / 2;

        let mut speaker_builder = builder.team_speakers_builder();
        for speaker in submitted_team.speakers {
            speaker_builder = speaker_builder
                .add_speaker(&speaker.id, speaker.score)
                .map_err(bad_request_from_string)?;
        }

        let speakers =
            speaker_builder.build().map_err(bad_request_from_string)?;
        builder
            .add_team(side, seq, speakers, submitted_team.points)
            .map_err(bad_request_from_string)?;
    }

    builder.build().map_err(bad_request_from_string)
}

pub async fn edit_ballot_page(
    Path((tournament_id, debate_id, judge_id)): Path<(String, String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let all_rounds =
        TournamentRounds::fetch(&tournament_id, &mut *conn).unwrap();
    let debate_repr = DebateRepr::fetch(&debate_id, &mut *conn);
    let round = Round::fetch(&debate_repr.debate.round_id, &mut *conn)?;

    let latest_ballots = debate_repr.latest_ballots(&mut *conn);
    let judge_ballot = latest_ballots
        .into_iter()
        .find(|b| b.metadata.judge_id == judge_id);
    let judge = debate_repr.judges.get(&judge_id);

    if judge.is_none() {
        return err_not_found();
    }
    let judge = judge.unwrap();

    let ballot_form_data = judge_ballot
        .map(|b| BallotForm::from_repr(&b, &debate_repr, &tournament));

    let cloned_tournament = tournament.clone();
    let ballot_form_fields = fields_of_single_ballot_form(
        &cloned_tournament,
        &debate_repr,
        ballot_form_data.as_ref(),
        &mut *conn,
    );

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
                            h1 class="display-4 fw-bold mb-3" { "Edit Ballot" }
                            h2 class="h4 text-muted mb-3" { "Debate " (debate_repr.debate.number) " — Judge " (judge.name) }
                        }

                        form method="post" {
                            (ballot_form_fields)

                            button type="submit" class="btn btn-dark btn-lg mt-4" {
                                "Save Ballot Overrides"
                            }
                            a href=(format!("/tournaments/{}/debates/{}/ballots", tournament_id, debate_id)) class="btn btn-outline-secondary btn-lg mt-4 ms-2" {
                                "Cancel"
                            }
                        }
                    }
                }
            })
            .render(),
    )
}

pub async fn do_edit_ballot(
    Path((tournament_id, debate_id, judge_id)): Path<(String, String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    QsForm(form): QsForm<BallotForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let debate_repr = DebateRepr::fetch(&debate_id, &mut *conn);
    let round = Round::fetch(&debate_repr.debate.round_id, &mut *conn)?;

    let judge = debate_repr.judges.get(&judge_id);
    if judge.is_none() {
        return err_not_found();
    }

    let prior = debate_repr
        .latest_ballots(&mut *conn)
        .into_iter()
        .find(|b| b.metadata.judge_id == judge_id);

    let expected_version = form.expected_version;
    let prior_version = prior.as_ref().map(|b| b.metadata.version).unwrap_or(0);

    let participants =
        crate::tournaments::participants::TournamentParticipants::load(
            &tournament.id,
            &mut *conn,
        );

    let new_metadata = BallotMetadata {
        id: Uuid::now_v7().to_string(),
        tournament_id: tournament_id.clone(),
        debate_id: debate_id.clone(),
        judge_id: judge_id.clone(),
        submitted_at: Utc::now().naive_utc(),
        motion_id: form.motion_id.clone(),
        version: 0,   // Set later by builder based on prior_version
        change: None, // You could populate this with e.g. "Admin Override"
        editor_id: Some(user.id.clone()),
    };

    let repr = build_edit_ballot(
        form,
        &tournament,
        &round,
        &debate_repr,
        &participants,
        new_metadata,
        expected_version,
        prior_version,
        &mut *conn,
    )?;

    repr.insert(&mut *conn);

    // Refresh debate repr and update status
    let debate_repr = DebateRepr::fetch(&debate_id, &mut *conn);
    update_debate_status(&debate_repr, &tournament, &mut *conn);

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/debates/{}/ballots",
        tournament_id, debate_id
    )))
}
