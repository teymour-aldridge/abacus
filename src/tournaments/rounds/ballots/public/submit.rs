use axum::extract::Path;
use axum::response::Redirect;
use axum_extra::extract::Form;
use chrono::Utc;
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use hypertext::{Renderable, maud, prelude::*};
use tracing::error;
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
                BallotMetadata, BallotRepr, BallotScore, BallotTeamRank,
                form::fields_of_single_ballot_form, update_debate_status,
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

/// Compute how many teams advance in an elimination round.
///
/// - 2-team formats: always 1.
/// - 4-team formats: 2, unless this is the final round of the break category,
///   in which case 1.
pub fn num_advancing_for_elim_round(
    tournament: &Tournament,
    round: &Round,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> usize {
    let total_teams = (tournament.teams_per_side * 2) as usize;
    if total_teams == 2 {
        1
    } else if round.is_final_of_break_category(conn) {
        1
    } else {
        total_teams / 2
    }
}

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

                        button type="submit" class="btn btn-dark btn-lg" {
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
        ).unwrap_err());
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

/// Receives ballots, validates them, and then appends them to the database.
///
// TODO: it would be nice to display the errors inline. However, this is more
// programming effort, so currently we collate a list of problems and then
// display a list of problems at the top of the page.
pub async fn do_submit_ballot(
    Path((tournament_id, private_url, round_id)): Path<(
        String,
        String,
        String,
    )>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
    Form(form): Form<BallotForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let judge =
        Judge::of_private_url(&private_url, &tournament_id, &mut *conn)?;

    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);
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

    let prior = tournament_ballots::table
        .filter(tournament_ballots::tournament_id.eq(&tournament.id))
        .filter(tournament_ballots::judge_id.eq(&judge.id))
        .filter(tournament_ballots::debate_id.eq(&debate.id))
        .order_by(tournament_ballots::submitted_at.desc())
        .first::<BallotMetadata>(&mut *conn)
        .optional()
        .unwrap();

    if !debate_repr.motions.contains_key(&form.motion_id) {
        return bad_request(
            Page::new()
                .tournament(tournament.clone())
                .user_opt(user)
                .current_rounds(current_rounds)
                .body(maud! {
                    ErrorAlert msg = "Error: data submitted incorrectly formatted (invalid motion)";
                })
                .render(),
        );
    }

    let new_ballot_id = Uuid::now_v7().to_string();
    let records_positions = tournament.round_requires_speaker_order(&round);
    let records_scores = tournament.round_requires_speaks(&round);
    let is_elim = round.is_elim();

    let teams_count = (tournament.teams_per_side * 2) as usize;
    if form.teams.len() != teams_count {
        return bad_request(
            Page::new()
                .tournament(tournament.clone())
                .user_opt(user)
                .current_rounds(current_rounds)
                .body(maud! {
                    ErrorAlert msg = "Error: data submitted incorrectly formatted (wrong number of teams)";
                })
                .render(),
        );
    }

    // Build speaker scores (only when we record speaker positions)
    let scores = if records_positions {
        let participants =
            TournamentParticipants::load(&tournament.id, &mut *conn);

        let speakers_count = tournament.substantive_speakers as usize
            + (tournament.reply_speakers as usize);

        let all_teams_have_expected_number_of_speakers = form
            .teams
            .iter()
            .any(|team| team.speakers.len() == speakers_count);
        if !all_teams_have_expected_number_of_speakers {
            return bad_request(
                Page::new()
                    .tournament(tournament.clone())
                    .user_opt(user)
                    .current_rounds(current_rounds)
                    .body(maud! {
                        ErrorAlert msg = "Error: data submitted incorrectly formatted (wrong number of speakers/scores)";
                    })
                    .render(),
            );
        }

        let mut scores = Vec::new();
        for (i, submitted_team) in form.teams.iter().enumerate() {
            let side = i % 2;
            let seq = i / 2;
            let id_of_team_at_this_position = &debate_repr
                .team_of_side_and_seq(side as i64, seq as i64)
                .team_id;

            for (j, submitted_speaker) in
                submitted_team.speakers.iter().enumerate()
            {
                let score = if records_scores {
                    if let Some(score_val) = submitted_speaker.score {
                        if j > tournament.substantive_speakers as usize {
                            error!(
                                "Expected j={} but actually j={j}",
                                tournament.substantive_speakers
                            );
                        }

                        if let Err(e) = tournament.check_score_valid(
                            rust_decimal::Decimal::from_f32_retain(score_val)
                                .unwrap(),
                            j >= tournament.substantive_speakers as usize,
                            participants
                                .speakers
                                .get(&submitted_speaker.id)
                                .unwrap()
                                .name
                                .clone(),
                        ) {
                            let msg = e;
                            return bad_request(
                                Page::new()
                                    .tournament(tournament.clone())
                                    .user_opt(user)
                                    .current_rounds(current_rounds)
                                    .body(maud! {
                                        ErrorAlert msg = (msg.as_str());
                                    })
                                    .render(),
                            );
                        }
                    }
                    submitted_speaker.score
                } else {
                    None
                };

                scores.push(BallotScore {
                    id: Uuid::now_v7().to_string(),
                    tournament_id: tournament_id.clone(),
                    ballot_id: new_ballot_id.clone(),
                    team_id: id_of_team_at_this_position.clone(),
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
        let num_advancing =
            num_advancing_for_elim_round(&tournament, &round, &mut *conn);
        let advancing = form.all_advancing_team_ids(&debate_repr);

        // Validate count
        if advancing.len() != num_advancing {
            return bad_request(
                Page::new()
                    .tournament(tournament.clone())
                    .user_opt(user)
                    .current_rounds(current_rounds)
                    .body(maud! {
                        ErrorAlert msg = (format!(
                            "Error: expected {} advancing team(s), but {} were selected",
                            num_advancing,
                            advancing.len()
                        ).as_str());
                    })
                    .render(),
            );
        }

        build_team_ranks_from_advancing(
            &advancing,
            &new_ballot_id,
            &tournament_id,
            &debate_repr,
        )
    } else {
        build_team_ranks_from_scores(
            &scores,
            &new_ballot_id,
            &tournament_id,
            &debate_repr,
            &tournament,
        )
    };

    let metadata = BallotMetadata {
        id: new_ballot_id,
        tournament_id: tournament_id.clone(),
        debate_id: debate.id.clone(),
        judge_id: judge.id.clone(),
        submitted_at: Utc::now().naive_utc(),
        motion_id: form.motion_id,
        version: prior.map(|b| b.version + 1).unwrap_or(0),
        change: None,
        editor_id: None,
    };

    let repr = BallotRepr::new_prelim(metadata, scores, team_ranks);
    repr.insert(&mut *conn);

    // Refresh debate repr so we pick up the newly inserted ballot
    let debate_repr = DebateRepr::fetch(&debate.id, &mut *conn);
    update_debate_status(&debate_repr, &tournament, &mut *conn);

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/privateurls/{}",
        tournament_id, private_url
    )))
}

/// Build team rank entries from speaker scores.
///
/// Teams receive one point for each team that they beat.
pub fn build_team_ranks_from_scores(
    scores: &[BallotScore],
    ballot_id: &str,
    tournament_id: &str,
    debate: &DebateRepr,
    tournament: &Tournament,
) -> Vec<BallotTeamRank> {
    let n_teams = (tournament.teams_per_side * 2) as usize;

    let mut team_totals: Vec<(String, f64)> = Vec::new();
    for dt in &debate.teams_of_debate {
        let total: f64 = scores
            .iter()
            .filter(|s| s.team_id == dt.team_id)
            .filter_map(|s| s.score.map(|v| v as f64))
            .sum();
        team_totals.push((dt.team_id.clone(), total));
    }

    team_totals.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Assign points: in a 2-team format, winner=1, loser=0.
    // In a 4-team format, 1st=3, 2nd=2, 3rd=1, 4th=0.
    team_totals
        .iter()
        .enumerate()
        .map(|(rank, (team_id, _))| BallotTeamRank {
            id: Uuid::now_v7().to_string(),
            tournament_id: tournament_id.to_string(),
            ballot_id: ballot_id.to_string(),
            team_id: team_id.clone(),
            points: (n_teams - 1 - rank) as i64,
        })
        .collect()
}

/// For advancing rounds we adopt the convention that advancing teams receive
/// one point and that eliminated teams receive zero points.
pub fn build_team_ranks_from_advancing(
    advancing_team_ids: &[String],
    ballot_id: &str,
    tournament_id: &str,
    debate: &DebateRepr,
) -> Vec<BallotTeamRank> {
    debate
        .teams_of_debate
        .iter()
        .map(|dt| {
            let is_advancing = advancing_team_ids.contains(&dt.team_id);
            BallotTeamRank {
                id: Uuid::now_v7().to_string(),
                tournament_id: tournament_id.to_string(),
                ballot_id: ballot_id.to_string(),
                team_id: dt.team_id.clone(),
                points: is_advancing as i64,
            }
        })
        .collect()
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
