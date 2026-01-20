use axum::extract::Path;
use axum::response::Redirect;
use axum_extra::extract::Form;
use chrono::Utc;
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use hypertext::{Renderable, maud, prelude::*};
use uuid::Uuid;

use crate::{
    auth::User,
    schema::{
        tournament_ballots, tournament_debate_judges, tournament_debates,
        tournament_judges, tournament_round_motions, tournament_rounds,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        participants::{Judge, TournamentParticipants},
        rounds::{
            Motion, Round,
            ballots::{
                Ballot, BallotRepr, BallotScore,
                form_components::{
                    MotionSelector, SpeakerInput, get_score_bounds,
                },
                manage::edit::SingleBallot,
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

    let judge = match tournament_judges::table
        .filter(
            tournament_judges::private_url
                .eq(&private_url)
                .and(tournament_judges::tournament_id.eq(&tournament_id)),
        )
        .first::<Judge>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(judge) => judge,
        None => return err_not_found(),
    };

    let round = Round::fetch(&round_id, &mut *conn)?;

    if round.draw_status != "released_full" {
        let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);
        return bad_request(
            Page::new()
                .tournament(tournament.clone())
                .user_opt(user)
                .current_rounds(current_rounds)
                .body(maud! {
                    ErrorAlert msg = "Error: draw not released.";
                })
                .render(),
        );
    }

    let debate = debate_of_judge_in_round(&judge.id, &round.id, &mut *conn)?;

    let debate_repr = DebateRepr::fetch(&debate.id, &mut *conn);

    let is_superuser = if let Some(ref user) = user {
        tournament
            .check_user_is_superuser(&user.id, &mut *conn)
            .is_ok()
    } else {
        false
    };

    let mut motions_query = tournament_round_motions::table
        .filter(tournament_round_motions::round_id.eq(&round.id))
        .into_boxed();

    if !is_superuser {
        motions_query = motions_query
            .filter(tournament_round_motions::published_at.is_not_null());
    }

    let motions: Vec<Motion> =
        motions_query.load::<Motion>(&mut *conn).unwrap();

    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .body(SubmitBallotForm {
                tournament,
                debate: debate_repr,
                motions,
                form_data: None,
            })
            .render(),
    )
}

/// Renders the form to submit a given ballot.
struct SubmitBallotForm {
    tournament: Tournament,
    debate: DebateRepr,
    motions: Vec<Motion>,
    form_data: Option<SingleBallot>,
}

impl Renderable for SubmitBallotForm {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        let substantive_speakers =
            self.tournament.substantive_speakers as usize;
        let reply_speakers = if self.tournament.reply_speakers { 1 } else { 0 };
        let total_speakers = substantive_speakers + reply_speakers;

        maud! {
            div class="container py-5" style="max-width: 800px;" {
                header class="mb-5" {
                    h1 class="display-4 fw-bold mb-3" { "Submit Ballot" }
                    span class="badge bg-light text-dark" {
                        "Round " (self.debate.debate.round_id)
                    }
                }

                form method="post" {
                    @if self.motions.len() > 1 {
                        (MotionSelector {
                            motions: &self.motions,
                            selected_motion_id: self.form_data.as_ref().map(|d| d.motion_id.as_str()),
                            field_name: "motion_id",
                        })
                    }

                    section class="mb-5" {
                        h2 class="h4 text-uppercase fw-bold text-secondary mb-4" {
                            "Speaker Scores"
                        }

                        @for side in 0..2 {
                            div class="mb-5" {
                                @for seq in 0..self.tournament.teams_per_side {
                                    @let team_idx = 2 * (seq as usize) + side;
                                    @let debate_team = &self.debate.teams_of_debate[team_idx];
                                    @let team = self.debate.teams.get(&debate_team.team_id).unwrap();
                                    @let speakers = self.debate.speakers_of_team.get(&debate_team.team_id).unwrap();

                                    @if seq > 0 {
                                        hr class="my-3";
                                    }

                                    div class="mb-3" {
                                        h3 class="h5 text-muted" { (team.name) }

                                        @for speaker_row in 0..total_speakers {
                                            @let is_reply = speaker_row >= substantive_speakers;
                                            @let (min_score, max_score) = get_score_bounds(&self.tournament, is_reply);
                                            @let form_speaker = self.form_data.as_ref()
                                                .and_then(|d| d.teams.get(team_idx))
                                                .and_then(|t| t.speakers.get(speaker_row));

                                            (SpeakerInput {
                                                team_name: &team.name,
                                                speaker_position: speaker_row,
                                                speakers,
                                                selected_speaker_id: form_speaker.map(|s| s.id.as_str()),
                                                score: form_speaker.map(|s| s.score),
                                                min_score,
                                                max_score,
                                                score_step: self.tournament.substantive_speech_step,
                                                speaker_field_name: &format!("teams[{}].speakers[{}].id", team_idx, speaker_row),
                                                score_field_name: &format!("teams[{}].speakers[{}].score", team_idx, speaker_row),
                                            })
                                        }

                                        input type="hidden" name=(format!("teams[{}].id", team_idx)) value=(debate_team.team_id);
                                    }
                                }
                            }
                        }
                    }

                    button type="submit" class="btn btn-dark btn-lg" {
                        "Submit Ballot"
                    }
                }
            }
        }
        .render_to(buffer);
    }
}

pub async fn do_submit_ballot(
    Path((tournament_id, private_url, round_id)): Path<(
        String,
        String,
        String,
    )>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
    Form(form): Form<SingleBallot>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;

    let judge = match tournament_judges::table
        .filter(
            tournament_judges::private_url
                .eq(&private_url)
                .and(tournament_judges::tournament_id.eq(&tournament_id)),
        )
        .first::<Judge>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(judge) => judge,
        None => return err_not_found(),
    };

    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);
    let round = Round::fetch(&round_id, &mut *conn)?;

    // todo: remove stringly typed API
    if round.draw_status != "released_full" {
        return bad_request(
            Page::new()
                .tournament(tournament.clone())
                .user_opt(user)
                .current_rounds(current_rounds)
                .body(maud! {
                    ErrorAlert msg = "Error: draw not released.";
                })
                .render(),
        );
    }

    let debate = debate_of_judge_in_round(&judge.id, &round.id, &mut *conn)?;
    let debate_repr = DebateRepr::fetch(&debate.id, &mut *conn);

    // todo: check whether (according to configured rules) it is permissable
    //       to submit a second ballot if an existing one exists
    let pre_existing_ballot = tournament_ballots::table
        .filter(tournament_ballots::tournament_id.eq(&tournament.id))
        .filter(tournament_ballots::judge_id.eq(&judge.id))
        .filter(tournament_ballots::debate_id.eq(&debate.id))
        .order_by(tournament_ballots::submitted_at.desc())
        .first::<Ballot>(&mut *conn)
        .optional()
        .unwrap();

    let participants = TournamentParticipants::load(&tournament.id, &mut *conn);

    let teams_count = (tournament.teams_per_side * 2) as usize;
    let speakers_count = tournament.substantive_speakers as usize
        + (tournament.reply_speakers as usize);

    let new_ballot_id = Uuid::now_v7().to_string();

    let repr = BallotRepr {
        ballot: Ballot {
            id: new_ballot_id.clone(),
            tournament_id: tournament_id.clone(),
            debate_id: debate.id,
            judge_id: judge.id.clone(),
            submitted_at: Utc::now().naive_utc(),
            motion_id: {
                if !debate_repr.motions.contains_key(&form.motion_id) {
                    return bad_request(
                        Page::new()
                            .tournament(tournament.clone())
                            .user_opt(user)
                            .current_rounds(current_rounds)
                            .body(maud! {
                                ErrorAlert msg = "Error: data submitted incorrectly formatted (invalid motion)";
                            })
                            .render()
                    );
                }
                form.motion_id
            },
            version: pre_existing_ballot.map(|b| b.version + 1).unwrap_or(1),
            // todo: implement a function to describe changes between two
            //       ballots
            // todo: should we describe the changes in a human-readable summary
            //       or instead try to visualise them (diff-style)?
            change: None,
            editor_id: Some(judge.id.clone()),
        },
        scores: {
            if form.teams.len() != teams_count
                || form
                    .teams
                    .iter()
                    .any(|team| team.speakers.len() != speakers_count)
            {
                return bad_request(
                    Page::new()
                        .tournament(tournament.clone())
                        .user_opt(user)
                        .current_rounds(current_rounds)
                        .body(maud! {
                            ErrorAlert msg = "Error: data submitted incorrectly formatted (wrong number of speakers/scores)";
                        })
                        .render()
                );
            }

            form.teams
                .iter()
                .enumerate()
                .map(|(i, submitted_team)| {
                    // 0 = OG, 1 = OO, 2 = CG, 3 = CO
                    let side = i % 2;
                    // 0 / 2 = 0, 1 / 2 = 0, 2 / 2 = 1, 3 / 2 = 1
                    let seq = i / 2;
                    if debate_repr.team_of_side_and_seq(side as i64, seq as i64).team_id
                        != submitted_team.id
                    {
                        return bad_request(
                            Page::new()
                                .tournament(tournament.clone())
                                .user_opt(user.clone())
                                .current_rounds(current_rounds.clone())
                                .body(maud! {
                                    // todo: ErrorAlert.msg ->
                                    //       ErrorAlert.children
                                    // so that we can write
                                    // ErrorAlert {
                                    //      contents
                                    // }
                                    ErrorAlert msg = "Team submitted does not match the provided data.";
                                })
                                .render(),
                        )
                        .map(|_| unreachable!());
                    };

                    submitted_team.speakers.iter().enumerate().map(
                        |(j, submitted_speaker_and_score)| {
                            if let Err(e) = tournament.check_score_valid(
                                rust_decimal::Decimal::from_f32_retain(
                                    submitted_speaker_and_score.score,
                                )
                                .unwrap(),
                                // todo: compute based on format rules
                                // (we should probably create a new ),
                                // potentially adding a new field to the form
                                false,
                                participants
                                    .speakers
                                    .get(&submitted_speaker_and_score.id)
                                    .unwrap()
                                    .name
                                    .clone(),
                            ) {
                                return bad_request(
                                    Page::new()
                                        .tournament(tournament.clone())
                                        .user_opt(user.clone())
                                        .current_rounds(current_rounds.clone())
                                        .body(maud! {
                                            // todo: ErrorAlert.msg ->
                                            //       ErrorAlert.children
                                            // so that we can write
                                            // ErrorAlert {
                                            //      contents
                                            // }
                                            ErrorAlert msg = (e.clone());
                                        })
                                        .render(),
                                )
                                .map(|_| unreachable!());
                            }

                            Ok(BallotScore {
                                id: Uuid::now_v7().to_string(),
                                tournament_id: tournament_id.clone(),
                                ballot_id: new_ballot_id.clone(),
                                team_id: submitted_team.id.clone(),
                                speaker_id: submitted_speaker_and_score
                                    .id
                                    .clone(),
                                speaker_position: j as i64,
                                score: submitted_speaker_and_score.score,
                            })
                        },
                    ).collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect()
        },
    };

    repr.insert(&mut *conn);

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
