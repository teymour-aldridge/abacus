use crate::tournaments::rounds::ballots::form::QsForm;
use axum::extract::Path;
use axum::response::Redirect;
use chrono::Utc;
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use hypertext::{Renderable, maud, prelude::*};
use uuid::Uuid;

use crate::{
    auth::User,
    schema::{
        tournament_ballots, tournament_debate_judges, tournament_debates,
        tournament_rounds,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        participants::{Judge, TournamentParticipants},
        rounds::{
            Round,
            ballots::{
                BallotMetadata, BallotRepr, form::fields_of_single_ballot_form,
                update_debate_status,
            },
            draws::{Debate, DebateRepr},
        },
    },
    util_resp::{
        FailureResponse, StandardResponse, bad_request, err_not_found,
        see_other_ok, success,
    },
    widgets::alert::ErrorAlert,
};

use super::super::manage::edit::BallotForm;

/// Page judges use to submit ballots. They are directed here from their private
/// URL page.
pub async fn submit_ballot_page(
    Path((tournament_id, private_url, round_id)): Path<(
        String,
        String,
        String,
    )>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let judge =
        Judge::of_private_url(&private_url, &tournament.id, &mut *conn)?;
    let round = Round::fetch(&round_id, &mut *conn)?;

    check_round_released(
        &tournament_id,
        user.clone(),
        &mut *conn,
        &tournament,
        &round,
    )?;

    check_round_not_completed(
        &tournament_id,
        user.clone(),
        &mut *conn,
        &tournament,
        &round,
    )?;

    let debate = debate_of_judge_in_round(&judge.id, &round.id, &mut *conn)?;
    let debate_repr = DebateRepr::fetch(&debate.id, &mut *conn);

    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);

    let ballot_form = fields_of_single_ballot_form(
        &tournament,
        &debate_repr,
        None,
        &mut *conn,
    );

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .body(maud! {
                div class="container py-5" style="max-width: 800px;" {
                    header class="mb-5" {
                        h1 class="display-4 fw-bold mb-3" { "Submit Ballot" }
                        span class="badge bg-light text-dark" {
                            "Round " (round.name)
                        }
                    }

                    form method="post" {
                        (ballot_form)

                        button type="submit" class="btn btn-dark btn-lg mt-4" {
                            "Submit Ballot"
                        }
                    }
                }
            })
            .render(),
    )
}

fn check_round_not_completed(
    tournament_id: &String,
    user: Option<User<true>>,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
    tournament: &Tournament,
    round: &Round,
) -> Result<(), FailureResponse> {
    if round.completed {
        let current_rounds = Round::current_rounds(tournament_id, conn);
        return Err(bad_request(
            Page::new()
                .tournament(tournament.clone())
                .user_opt(user.clone())
                .current_rounds(current_rounds)
                .body(maud! {
                    ErrorAlert msg = "Error: this round has been completed. Ballots can no longer be submitted.";
                })
                .render(),
        )
        .unwrap_err());
    }
    Ok(())
}

fn check_round_released(
    tournament_id: &String,
    user: Option<User<true>>,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
    tournament: &Tournament,
    round: &Round,
) -> Result<(), FailureResponse> {
    if round.draw_status != "released_full" {
        let current_rounds = Round::current_rounds(tournament_id, conn);
        return Err(bad_request(
            Page::new()
                .tournament(tournament.clone())
                .user_opt(user)
                .current_rounds(current_rounds)
                .body(maud! {
                    ErrorAlert msg = "Error: draw not released.";
                })
                .render(),
        )
        .unwrap_err());
    }
    Ok(())
}

fn build_submit_ballot(
    form: BallotForm,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
    tournament: &Tournament,
    round: &Round,
    debate_repr: &DebateRepr,
    participants: &TournamentParticipants,
    new_metadata: BallotMetadata,
    expected_version: i64,
    prior_version: i64,
) -> Result<BallotRepr, FailureResponse> {
    tracing::debug!("form = {form:?}");

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

/// Receives ballots, validates them, and then appends them to the database.
///
// TODO: it would be nice to display the errors inline. However, this is more
// programming effort, so currently we collate a list of problems and then
// display a list of problems at the top of the page.
#[tracing::instrument(skip(conn))]
pub async fn do_submit_ballot(
    Path((tournament_id, private_url, round_id)): Path<(
        String,
        String,
        String,
    )>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
    QsForm(form): QsForm<BallotForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let judge =
        Judge::of_private_url(&private_url, &tournament_id, &mut *conn)?;

    let round = Round::fetch(&round_id, &mut *conn)?;

    tracing::debug!("Resolved tournament={}, judge={}, round={}.", tournament.id, judge.id, round.id);

    check_round_released(
        &tournament_id,
        user.clone(),
        &mut *conn,
        &tournament,
        &round,
    )?;
    check_round_not_completed(
        &tournament_id,
        user.clone(),
        &mut *conn,
        &tournament,
        &round,
    )?;

    let debate = debate_of_judge_in_round(&judge.id, &round.id, &mut *conn)?;
    let debate_repr = DebateRepr::fetch(&debate.id, &mut *conn);

    let prior = tournament_ballots::table
        .filter(tournament_ballots::tournament_id.eq(&tournament.id))
        .filter(tournament_ballots::judge_id.eq(&judge.id))
        .filter(tournament_ballots::debate_id.eq(&debate.id))
        .order_by(tournament_ballots::submitted_at.desc())
        .first::<BallotMetadata>(&mut *conn)
        .optional()
        .unwrap();

    let expected_version = form.expected_version;
    let prior_version = prior.as_ref().map(|b| b.version).unwrap_or(0);

    let participants = TournamentParticipants::load(&tournament.id, &mut *conn);

    let new_metadata = BallotMetadata {
        id: Uuid::now_v7().to_string(),
        tournament_id: tournament_id.clone(),
        debate_id: debate.id.clone(),
        judge_id: judge.id.clone(),
        submitted_at: Utc::now().naive_utc(),
        motion_id: form.motion_id.clone(),
        version: 0, // Set later by builder based on prior_version
        change: None,
        editor_id: None,
    };

    let repr = build_submit_ballot(
        form,
        &mut *conn,
        &tournament,
        &round,
        &debate_repr,
        &participants,
        new_metadata,
        expected_version,
        prior_version,
    )?;

    repr.insert(&mut *conn);

    // Refresh debate repr so we pick up the newly inserted ballot
    let debate_repr = DebateRepr::fetch(&debate.id, &mut *conn);
    update_debate_status(&debate_repr, &tournament, &mut *conn);

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/privateurls/{}",
        tournament_id, private_url
    )))
}

fn debate_of_judge_in_round(
    judge_id: &str,
    round_id: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<Debate, FailureResponse> {
    match tournament_debates::table
        .inner_join(
            tournament_rounds::table
                .on(tournament_rounds::id.eq(tournament_debates::round_id)),
        )
        .filter(tournament_rounds::id.eq(round_id))
        .filter(tournament_rounds::draw_status.eq("released_full"))
        .filter(diesel::dsl::exists(
            tournament_debate_judges::table
                .filter(tournament_debate_judges::judge_id.eq(judge_id))
                .filter(
                    tournament_debate_judges::debate_id
                        .eq(tournament_debates::id),
                ),
        ))
        .select(tournament_debates::all_columns)
        .first::<Debate>(conn)
        .optional()
        .unwrap()
    {
        Some(debate) => Ok(debate),
        None => err_not_found().map(|_| unreachable!()),
    }
}
