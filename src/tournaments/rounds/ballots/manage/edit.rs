use crate::tournaments::rounds::ballots::form::QsForm;
use crate::ui::response::{ModalQuery, after_edit, form_page};
use axum::extract::{Path, Query};
use axum::response::Redirect;
use chrono::Utc;
use diesel::{connection::LoadConnection, sqlite::Sqlite};
use hypertext::{Renderable, maud, prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::User,
    state::Conn,
    tournaments::{
        Tournament,
        rounds::{
            Round,
            ballots::{
                BallotMetadata, BallotRepr, form::fields_of_single_ballot_form,
                update_debate_status,
            },
            draws::DebateRepr,
        },
    },
    util_resp::{StandardResponse, err_not_found},
};

fn redirect_to_ballot_set(
    query: ModalQuery,
    tournament_id: &str,
    debate_id: &str,
) -> StandardResponse {
    crate::tournaments::manage::workspace::after_redirect(
        query,
        Redirect::to(&format!(
            "/tournaments/{}/debates/{}/ballots",
            tournament_id, debate_id
        )),
    )
}

#[derive(Serialize, Deserialize, Clone)]
/// Ballot form used for judges to submit ballots, and for tab directors to
/// edit them behind the scenes.
///
/// Our HTML form logic requires that this form be parsed with [`serde_qs`]
/// rather than the standard axum (or axum_extra) extractors.
#[derive(Debug)]
pub struct BallotForm {
    #[serde(default)]
    pub teams: Vec<BallotFormSingleTeamEntry>,
    pub motion_id: String,
    #[serde(default)]
    pub expected_version: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BallotFormSingleSpeakerEntry {
    pub id: String,
    pub score: Option<f32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
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
            expected_version: repr.metadata.version + 1,
        }
    }
}

#[tracing::instrument(skip(
    form,
    tournament,
    round,
    debate_repr,
    participants,
    new_metadata,
    prior_version,
    conn
))]
fn build_edit_ballot(
    form: BallotForm,
    tournament: &Tournament,
    round: &Round,
    debate_repr: &DebateRepr,
    participants: &crate::tournaments::participants::TournamentParticipants,
    new_metadata: BallotMetadata,
    expected_version: i64,
    prior_version: i64,
    has_prior_ballot: bool,
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
        has_prior_ballot,
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
    Query(query): Query<ModalQuery>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    super::scope_debate(&tournament_id, &debate_id, &mut *conn)?;
    let debate_repr = DebateRepr::try_fetch(&debate_id, &mut *conn)?;
    let round =
        Round::fetch(&tournament_id, &debate_repr.debate.round_id, &mut *conn)?;

    if debate_repr.motions.is_empty() {
        return crate::util_resp::bad_request(maud! { div class="alert alert-warning" { "Ballot entry is unavailable until a motion is available for this debate." } }.render());
    }

    // todo: set flash message explaining why the redirect happened
    if round.draw_status != "released_full" {
        return redirect_to_ballot_set(query, &tournament_id, &debate_id);
    }

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

    render_edit(
        query,
        tournament,
        user,
        &round,
        &debate_repr,
        &judge.name,
        &judge.id,
        ballot_form_data.as_ref(),
        None,
    )
}

fn render_edit(
    query: ModalQuery,
    tournament: Tournament,
    user: User<true>,
    round: &Round,
    debate: &DebateRepr,
    judge_name: &str,
    judge_id: &str,
    form: Option<&BallotForm>,
    error: Option<&str>,
) -> StandardResponse {
    let url = format!(
        "/tournaments/{}/debates/{}/ballots",
        tournament.id, debate.debate.id
    );
    let action = format!(
        "/tournaments/{}/debates/{}/judges/{}/edit",
        tournament.id, debate.debate.id, judge_id
    );
    let modal_action = format!("{action}?modal=true");
    let fields = fields_of_single_ballot_form(&tournament, round, debate, form);
    form_page(
        query,
        "Edit ballot",
        &url,
        &tournament,
        user,
        maud! {
            span class="badge bg-secondary" { "Restricted: Tournament organisers" }
            h2 { "Debate " (debate.debate.number) " — " (judge_name) }
            @if let Some(error) = error { div class="alert alert-danger" { (error) } }
            form method="post" action=(action) hx-post=(modal_action) hx-target="#ui-modal" {
                (fields)
                button type="submit" class="btn btn-dark" { "Save ballot overrides" }
            }
        },
        error.is_some(),
    )
}

pub async fn do_edit_ballot(
    Path((tournament_id, debate_id, judge_id)): Path<(String, String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Query(query): Query<ModalQuery>,
    QsForm(form): QsForm<BallotForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    super::scope_debate(&tournament_id, &debate_id, &mut *conn)?;
    let debate_repr = DebateRepr::try_fetch(&debate_id, &mut *conn)?;
    let round =
        Round::fetch(&tournament_id, &debate_repr.debate.round_id, &mut *conn)?;

    if debate_repr.motions.is_empty() {
        return crate::util_resp::bad_request(maud! { div class="alert alert-warning" { "Ballot entry is unavailable until a motion is available for this debate." } }.render());
    }
    if round.draw_status != "released_full" {
        return redirect_to_ballot_set(query, &tournament_id, &debate_id);
    }

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
    let has_prior_ballot = prior.is_some();

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
        version: 0, // Set later by builder based on prior_version
        change: Some("Admin override".to_string()),
        editor_id: Some(user.id.clone()),
    };

    let repr = build_edit_ballot(
        form.clone(),
        &tournament,
        &round,
        &debate_repr,
        &participants,
        new_metadata,
        expected_version,
        prior_version,
        has_prior_ballot,
        &mut *conn,
    );
    let repr = match repr {
        Ok(repr) => repr,
        Err(crate::util_resp::FailureResponse::BadRequest(_)) => {
            return render_edit(
                query,
                tournament.clone(),
                user,
                &round,
                &debate_repr,
                &judge.unwrap().name,
                &judge_id,
                Some(&form),
                Some(
                    "The ballot is invalid or was updated since this form opened. Check the scores, speakers, points, and version, then retry.",
                ),
            );
        }
        Err(error) => return Err(error),
    };

    repr.insert(&mut *conn);

    // Refresh debate repr and update status
    super::scope_debate(&tournament_id, &debate_id, &mut *conn)?;
    let debate_repr = DebateRepr::try_fetch(&debate_id, &mut *conn)?;
    update_debate_status(&debate_repr, &tournament, &mut *conn);

    after_edit(
        query,
        format!(
            "/tournaments/{}/debates/{}/ballots",
            tournament_id, debate_id
        ),
    )
}
