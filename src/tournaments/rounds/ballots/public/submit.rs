use std::collections::HashSet;

use axum::extract::Path;
use diesel::{
    connection::LoadConnection,
    prelude::*,
    sql_types::{BigInt, Nullable},
    sqlite::Sqlite,
};
use hypertext::{Renderable, maud, prelude::*};
use rust_decimal::prelude::ToPrimitive;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::User,
    schema::{
        tournament_ballots, tournament_debate_judges, tournament_debates,
        tournament_judges, tournament_round_motions, tournament_rounds,
        tournament_speaker_score_entries,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        participants::Judge,
        rounds::{
            Motion, Round,
            draws::{Debate, DebateRepr},
        },
    },
    util_resp::{
        FailureResponse, StandardResponse, bad_request, err_not_found, success,
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

    let motions: Vec<Motion> = tournament_round_motions::table
        .filter(tournament_round_motions::round_id.eq(&round.id))
        .load::<Motion>(&mut *conn)
        .unwrap();

    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .body(BallotFormRenderer {
                tournament,
                debate: debate_repr,
                motions,
            })
            .render(),
    )
}

/// Renders the form to submit a given ballot.
struct BallotFormRenderer {
    tournament: Tournament,
    debate: DebateRepr,
    motions: Vec<Motion>,
}

impl Renderable for BallotFormRenderer {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            div class="container py-5" style="max-width: 800px;" {
                header class="mb-5" {
                    h1 class="display-4 fw-bold mb-3" {
                        "Submit Ballot"
                    }
                    span class="badge bg-light text-dark" {
                        "Round " (self.debate.debate.round_id)
                    }
                }

                form method="post" {
                    @if self.motions.len() > 1 {
                        section class="mb-5" {
                            h2 class="h4 text-uppercase fw-bold text-secondary mb-4" {
                                "Motion"
                            }
                            select class="form-select" name="motion" required {
                                option selected value="" {
                                    "Select motion"
                                }
                                @for motion in &self.motions {
                                    option value=(motion.id) {
                                        (motion.motion)
                                    }
                                }
                            }
                        }
                    }

                    section class="mb-5" {
                        h2 class="h4 text-uppercase fw-bold text-secondary mb-4" {
                            "Speaker Scores"
                        }

                        @for seq in 0..self.tournament.teams_per_side {
                            @for row in 0..(self.tournament.substantive_speakers as usize) {
                                div class="row mb-3" {
                                    div class="col-md-6" {
                                        @let debate_team = &self.debate.teams_of_debate[2 * (seq as usize)];
                                        @let speakers = self.debate.speakers_of_team.get(&debate_team.team_id).unwrap_or_else(|| {
                                            panic!(
                                                "Unable to retrieve speakers for team ID {} in debate ID {}. Debug info: {:?}",
                                                debate_team.id,
                                                self.debate.debate.id,
                                                self.debate.speakers_of_team
                                            );
                                        });
                                        @let team = self.debate.teams.get(&debate_team.team_id).unwrap();

                                        div class="mb-2" {
                                            label class="form-label text-uppercase fw-bold" {
                                                (team.name) " - Speaker " (row + 1)
                                            }
                                            select class="form-select mb-2" name="speakers" required {
                                                option selected value="" {
                                                    "Select speaker"
                                                }
                                                @for speaker in speakers {
                                                    option value=(speaker.id) {
                                                        (speaker.name)
                                                    }
                                                }
                                            }
                                            input
                                                required
                                                name="scores"
                                                type="number"
                                                class="form-control"
                                                min="50" max="99" step="1"
                                                placeholder="Score (50-99)";
                                        }
                                    }

                                    div class="col-md-6" {
                                        @let debate_team = &self.debate.teams_of_debate[2 * (seq as usize) + 1];
                                        @let speakers = self.debate.speakers_of_team.get(&debate_team.team_id).unwrap_or_else(|| {
                                            panic!(
                                                "Unable to retrieve speakers for team ID {} in debate ID {}. Debug info: {:?}",
                                                debate_team.id,
                                                self.debate.debate.id,
                                                self.debate.speakers_of_team
                                            );
                                        });
                                        @let team = self.debate.teams.get(&debate_team.team_id).unwrap();

                                        div class="mb-2" {
                                            label class="form-label text-uppercase fw-bold" {
                                                (team.name) " - Speaker " (row + 1)
                                            }
                                            select class="form-select mb-2" name="speakers" required {
                                                option selected value="" {
                                                    "Select speaker"
                                                }
                                                @for speaker in speakers {
                                                    option value=(speaker.id) {
                                                        (speaker.name)
                                                    }
                                                }
                                            }
                                            input
                                                required
                                                name="scores"
                                                type="number"
                                                class="form-control"
                                                min="50" max="99" step="1"
                                                placeholder="Score (50-99)";
                                        }
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

#[derive(Deserialize)]
/// A BP ballot might look like
///
/// ```ignore
/// speakers: [s1, s2, s3, s4, s5, s6, s7, s8]
/// scores: [85, 86, 87, 88, 89, 90, 91, 92]
/// ```
pub struct BallotSubmissionForm {
    #[serde(default)]
    speakers: Vec<String>,
    #[serde(default)]
    scores: Vec<f64>,
    motion: Option<String>,
}

pub async fn do_submit_ballot(
    Path((tournament_id, private_url, round_id)): Path<(
        String,
        String,
        String,
    )>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<
        BallotSubmissionForm,
    >,
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

    // TODO: how would we handle teams who are given a bye: presumably it should
    // not be possible to submit a ballot on them (i.e. the debate will be set
    // up as `bye` and will have no adjudicators).
    let actual_teams = &debate_repr.teams_of_debate;

    let teams_count = (tournament.teams_per_side * 2) as usize;
    let speakers_count = tournament.substantive_speakers as usize;
    let expected_len = teams_count * speakers_count;

    {
        if form.speakers.len() != expected_len
            || form.scores.len() != expected_len
        {
            let current_rounds =
                Round::current_rounds(&tournament_id, &mut *conn);
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

        if actual_teams.len() != teams_count {
            // Should verify logic about byes here if relevant
        }
    };

    let mut nested_speakers: Vec<Vec<String>> =
        vec![vec![String::default(); speakers_count]; teams_count];
    let mut nested_scores: Vec<Vec<f64>> =
        vec![vec![0.0; speakers_count]; teams_count];

    let mut speaker_iter = form.speakers.into_iter();
    let mut score_iter = form.scores.into_iter();

    for seq in 0..tournament.teams_per_side {
        for row in 0..speakers_count {
            let t1_idx = (seq as usize * 2) as usize;
            let s1 = speaker_iter.next().unwrap();
            let sc1 = score_iter.next().unwrap();

            nested_speakers[t1_idx][row] = s1;
            nested_scores[t1_idx][row] = sc1;

            let t2_idx = (seq as usize * 2 + 1) as usize;
            let s2 = speaker_iter.next().unwrap();
            let sc2 = score_iter.next().unwrap();

            nested_speakers[t2_idx][row] = s2;
            nested_scores[t2_idx][row] = sc2;
        }
    }

    {
        if let Some(motion) = &form.motion
            && !diesel::dsl::select(diesel::dsl::exists(
                tournament_round_motions::table
                    .filter(tournament_round_motions::round_id.eq(&round.id))
                    .filter(tournament_round_motions::id.eq(motion)),
            ))
            .get_result::<bool>(&mut *conn)
            .unwrap()
        {
            let current_rounds =
                Round::current_rounds(&tournament_id, &mut *conn);
            return bad_request(
                Page::new()
                    .tournament(tournament.clone())
                    .user_opt(user)
                    .current_rounds(current_rounds)
                    .body(maud! {
                        ErrorAlert msg = "Error: submitted motion is not a valid motion for the corresponding round.";
                    })
                    .render()
            );
        }

        if form.motion.is_none()
            && tournament_round_motions::table
                .filter(tournament_round_motions::round_id.eq(&round.id))
                .count()
                .get_result::<i64>(&mut *conn)
                .unwrap()
                > 1
        {
            let current_rounds =
                Round::current_rounds(&tournament_id, &mut *conn);
            return bad_request(
                Page::new()
                    .tournament(tournament.clone())
                    .user_opt(user)
                    .current_rounds(current_rounds)
                    .body(maud! {
                        ErrorAlert msg = "Error: motion must be specified where there is more than one motion for the given round.";
                    })
                    .render(),
            );
        }
    };

    let mut scoresheets = Vec::new();

    for (i, (speakers_on_team, scores_of_speakers)) in
        nested_speakers.iter().zip(nested_scores.iter()).enumerate()
    {
        let actual_debate_team = &actual_teams[i];
        let actual_team_speakers =
            &debate_repr.speakers_of_team[&actual_debate_team.team_id];

        let mut scores = Vec::new();

        for (j, speaker) in speakers_on_team.iter().enumerate() {
            let speak_of_speaker = scores_of_speakers[j];

            let submitted_speaker_is_on_this_team = actual_team_speakers
                .iter()
                .any(|actual_speaker| &actual_speaker.id == speaker);

            if !submitted_speaker_is_on_this_team {
                let current_rounds =
                    Round::current_rounds(&tournament_id, &mut *conn);
                return bad_request(
                    Page::new()
                        .tournament(tournament.clone())
                        .user_opt(user)
                        .current_rounds(current_rounds)
                        .body(maud! {
                            ErrorAlert msg = "Error: data submitted incorrectly formatted (speaker not on team)";
                        })
                        .render()
                );
            }

            scores.push((
                speaker.clone(),
                // todo: when can this fail
                rust_decimal::Decimal::from_f64_retain(speak_of_speaker)
                    .unwrap(),
            ));
        }

        scoresheets.push(TeamScoresheet {
            entries: scores,
            team_id: actual_debate_team.team_id.clone(),
        });
    }

    let (all_distinct, _) = scoresheets.iter().fold(
        (false, HashSet::new()),
        |(dup, mut set): (bool, HashSet<rust_decimal::Decimal>), next| {
            let dup1 = set.insert(next.entries.iter().map(|(_, s)| s).sum());
            (dup || dup1, set)
        },
    );

    // todo: we need to add additional validation (this should be configurable
    // by the user)
    if !all_distinct {
        let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);
        return bad_request(
            Page::new()
                .tournament(tournament.clone())
                .user_opt(user)
                .current_rounds(current_rounds)
                .body(maud! {
                    ErrorAlert msg = "Error: two teams have a duplicate speech.";
                })
                .render()
        );
    }

    let ballot_id = Uuid::now_v7().to_string();

    let n = diesel::insert_into(tournament_ballots::table).values((
        tournament_ballots::id.eq(&ballot_id),
        tournament_ballots::tournament_id.eq(&debate.tournament_id),
        tournament_ballots::debate_id.eq(&debate.id),
        tournament_ballots::judge_id.eq(&judge.id),
        tournament_ballots::submitted_at.eq(diesel::dsl::now),
        // todo: fill these in
        tournament_ballots::motion_id.eq(match &form.motion {
            Some(motion) => motion.clone(),
            None => {
                tournament_round_motions::table
                    .filter(tournament_round_motions::round_id.eq(&round.id))
                    .first::<Motion>(&mut *conn)
                    .unwrap()
                    .id
            }
        }),
        tournament_ballots::version.eq({
            define_sql_function! { fn coalesce(x: Nullable<BigInt>, y: BigInt) -> BigInt; }

            let ballots_sq =
                diesel::alias!(tournament_ballots as ballots_alias);

            coalesce(ballots_sq
                .filter(
                    ballots_sq
                        .field(tournament_ballots::debate_id)
                        .eq(&debate.id)
                        .and(
                            ballots_sq
                                .field(tournament_ballots::judge_id)
                                .eq(&judge.id),
                        ),
                )
                .order_by(ballots_sq.field(tournament_ballots::version).desc())
                .select(ballots_sq.field(tournament_ballots::version))
                .single_value(), 0)
        }),
        // todo: these fields need to be added
        tournament_ballots::change.eq(None::<String>),
        tournament_ballots::editor_id.eq(None::<String>),
    )).execute(&mut *conn).unwrap();
    assert_eq!(n, 1);

    // todo: can use with_capacity as this is known statically
    let mut scoresheet_entries = Vec::new();

    for sheet in &scoresheets {
        for (i, entry) in sheet.entries.iter().enumerate() {
            scoresheet_entries.push((
                tournament_speaker_score_entries::id
                    .eq(Uuid::now_v7().to_string()),
                tournament_speaker_score_entries::ballot_id.eq(&ballot_id),
                tournament_speaker_score_entries::team_id.eq(&sheet.team_id),
                tournament_speaker_score_entries::speaker_id.eq(&entry.0),
                tournament_speaker_score_entries::speaker_position.eq(i as i64),
                tournament_speaker_score_entries::score
                    .eq(entry.1.to_f32().unwrap()),
            ))
        }
    }

    diesel::insert_into(tournament_speaker_score_entries::table)
        .values(&scoresheet_entries)
        .execute(&mut *conn)
        .unwrap();

    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .body(maud! {
                div class="container py-5" style="max-width: 800px;" {
                    header class="mb-5" {
                        h1 class="display-4 fw-bold mb-3" {
                            "Ballot Submitted"
                        }
                    }

                    div class="card mb-4" {
                        div class="card-body" {
                            h2 class="card-title h4 text-uppercase fw-bold text-success mb-3" {
                                "Success"
                            }
                            p class="card-text" {
                                "Your ballot has been submitted successfully."
                            }
                        }
                    }
                }
            })
            .render(),
    )
}

struct TeamScoresheet {
    team_id: String,
    entries: Vec<(String, rust_decimal::Decimal)>,
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
