use std::collections::HashSet;

use diesel::{
    connection::LoadConnection,
    prelude::*,
    sql_types::{BigInt, Nullable},
    sqlite::Sqlite,
};
use hypertext::{Renderable, maud, prelude::*};
use rocket::{FromForm, form::Form, get, post};
use rust_decimal::prelude::ToPrimitive;
use uuid::Uuid;

use crate::{
    auth::User,
    schema::{
        tournament_ballots, tournament_debate_judges, tournament_debates,
        tournament_draws, tournament_round_motions,
        tournament_speaker_score_entries,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        participants::Speaker,
        privateurls::{Participant, ParticipantKind},
        rounds::{
            Motion, Round,
            draws::{Debate, DebateRepr, Draw},
        },
    },
    util_resp::{
        FailureResponse, StandardResponse, bad_request, err_not_found, success,
        unauthorized,
    },
    widgets::alert::ErrorAlert,
};

#[get(
    "/tournaments/<tournament_id>/privateurls/<private_url>/rounds/<round_id>/submit"
)]
pub async fn submit_ballot_page(
    tournament_id: &str,
    private_url: &str,
    round_id: &str,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    let private_url = Participant::fetch(private_url, &mut *conn)?;

    let judge = match private_url.kind {
        ParticipantKind::Judge(judge) => judge,
        _ => return unauthorized(),
    };

    let round = Round::fetch(round_id, &mut *conn)?;

    match tournament_draws::table
        .filter(tournament_draws::round_id.eq(&round.id))
        .order_by(tournament_draws::released_at.desc())
        .first::<Draw>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(draw) => draw,
        None => {
            return bad_request(
                Page::new()
                    .tournament(tournament.clone())
                    .user_opt(user)
                    .body(maud! {
                        ErrorAlert msg = "Error: draw not released.";
                    })
                    .render(),
            );
        }
    };

    let debate = debate_of_judge_in_round(&judge.id, &round.id, &mut *conn)?;

    let debate_repr = DebateRepr::fetch(&debate.id, &mut *conn);

    let motions: Vec<Motion> = tournament_round_motions::table
        .filter(tournament_round_motions::round_id.eq(&round.id))
        .load::<Motion>(&mut *conn)
        .unwrap();

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
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
            form method="post" {
                @if self.motions.len() > 1 {
                    select class = "form-select" name="motion" {
                        option selected value = "-----" {
                            "-- Select motion --"
                        }
                        @for motion in &self.motions {
                            option value = (motion.id) {
                                (motion.motion)
                            }
                        }
                    }
                }

                @for seq in 0..self.tournament.teams_per_side {
                    @for row in 0..(self.tournament.substantive_speakers as usize) {
                        div class = "row" {
                            div class = "col" {
                                @let team = &self.debate.teams_of_debate[2 * (seq as usize)];
                                @let speakers = self.debate.speakers_of_team.get(&team.id).unwrap();
                                (TeamSpeakerChoice {
                                    team_pos: 2*(seq as usize),
                                    speaker_pos: row,
                                    speakers_on_team: speakers
                                })
                            }
                            div class = "col" {
                                @let team = &self.debate.teams_of_debate[2 * (seq as usize) + 1];
                                @let speakers = self.debate.speakers_of_team.get(&team.id).unwrap();
                                (TeamSpeakerChoice {
                                    team_pos: 2*(seq as usize) + 1,
                                    speaker_pos: row,
                                    speakers_on_team: speakers
                                })
                            }
                        }
                    }
                }
            }
        }
        .render_to(buffer);
    }
}

struct TeamSpeakerChoice<'r> {
    team_pos: usize,
    /// Position on the team
    speaker_pos: usize,
    speakers_on_team: &'r Vec<Speaker>,
    // TODO: reply speakers
}

impl Renderable for TeamSpeakerChoice<'_> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            select class="form-select"
                name=(format!("speakers[{}][{}]", self.team_pos, self.speaker_pos)) {
                option selected value="-----" {
                    "-----"
                }
                @for speaker in self.speakers_on_team {
                    option selected value=(speaker.id) {
                        (speaker.name)
                    }
                }
            }

            input
                required
                  name=(format!("scores[{}][{}]", self.team_pos, self.speaker_pos))
                  type="number"
                  // todo: adjust per tournament
                  min="50" max="99" step="1" placeholder="Speaker score";
        }
        .render_to(buffer);
    }
}

#[derive(FromForm)]
/// A BP ballot might look like
///
/// ```ignore
/// speakers: [
///     [s1, s2],
///     [s3, s4],
///     [s5, s6],
///     [s7, s8]
/// ]
/// scores: [
///     [85, 86],
///     [87, 88],
///     [89, 90],
///     [91, 92]
/// ]
/// ```
pub struct BallotSubmissionForm {
    speakers: Vec<Vec<String>>,
    scores: Vec<Vec<f64>>,
    motion: Option<String>,
}

#[post(
    "/tournaments/<tournament_id>/privateurls/<private_url>/rounds/<round_id>/submit",
    data = "<form>"
)]
pub async fn do_submit_ballot(
    tournament_id: &str,
    private_url: &str,
    round_id: &str,
    user: Option<User<true>>,
    mut conn: Conn<true>,
    form: Form<BallotSubmissionForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    let private_url = Participant::fetch(private_url, &mut *conn)?;
    let judge = match private_url.kind {
        ParticipantKind::Judge(judge) => judge,
        _ => return unauthorized(),
    };
    let round = Round::fetch(round_id, &mut *conn)?;
    match tournament_draws::table
        .filter(tournament_draws::round_id.eq(&round.id))
        .order_by(tournament_draws::released_at.desc())
        .first::<Draw>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(draw) => draw,
        None => {
            return bad_request(
                Page::new()
                    .tournament(tournament.clone())
                    .user_opt(user)
                    .body(maud! {
                        ErrorAlert msg = "Error: draw not released.";
                    })
                    .render(),
            );
        }
    };
    let debate = debate_of_judge_in_round(&judge.id, &round.id, &mut *conn)?;
    let debate_repr = DebateRepr::fetch(&debate.id, &mut *conn);

    // TODO: how would we handle teams who are given a bye: presumably it should
    // not be possible to submit a ballot on them (i.e. the debate will be set
    // up as `bye` and will have no adjudicators).
    let actual_teams = &debate_repr.teams_of_debate;

    {
        if form.speakers.len() != actual_teams.len()
            || form.scores.len() != actual_teams.len()
        {
            return bad_request(
                Page::new()
                    .tournament(tournament.clone())
                    .user_opt(user)
                    .body(maud! {
                        ErrorAlert msg = "Error: data submitted incorrectly formatted";
                    })
                    .render()
            );
        }

        for each in &form.speakers {
            if each.len() != (tournament.substantive_speakers as usize) {
                return bad_request(
                    Page::new()
                        .tournament(tournament.clone())
                        .user_opt(user)
                        .body(maud! {
                            ErrorAlert msg = "Error: data submitted incorrectly formatted";
                        })
                        .render()
                );
            }
        }

        for each in &form.scores {
            if each.len() != (tournament.substantive_speakers as usize) {
                return bad_request(
                    Page::new()
                        .tournament(tournament.clone())
                        .user_opt(user)
                        .body(maud! {
                            ErrorAlert msg = "Error: data submitted incorrectly formatted";
                        })
                        .render()
                );
            }
        }
    };

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
            return bad_request(
                Page::new()
                    .tournament(tournament.clone())
                    .user_opt(user)
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
            return bad_request(
                Page::new()
                    .tournament(tournament.clone())
                    .user_opt(user)
                    .body(maud! {
                        ErrorAlert msg = "Error: motion must be specified where there is more than one motion for the given round.";
                    })
                    .render(),
            );
        }
    };

    let mut scoresheets = Vec::new();

    for ((i, speakers_on_team), scores_of_speakers) in
        form.speakers.iter().enumerate().zip(&form.scores)
    {
        let actual_team = &actual_teams[i];
        let actual_team_speakers =
            &debate_repr.speakers_of_team[&actual_team.id];

        let mut scores = Vec::new();

        for (j, speaker) in speakers_on_team.iter().enumerate() {
            let speak_of_speaker = scores_of_speakers[j];

            let submitted_speaker_is_on_this_team = actual_team_speakers
                .iter()
                .any(|actual_speaker| &actual_speaker.id == speaker);

            if !submitted_speaker_is_on_this_team {
                return bad_request(
                    Page::new()
                        .tournament(tournament.clone())
                        .user_opt(user)
                        .body(maud! {
                            ErrorAlert msg = "Error: data submitted incorrectly formatted";
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
            team_id: actual_team.id.clone(),
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
        return bad_request(
            Page::new()
                .tournament(tournament.clone())
                .user_opt(user)
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

    todo!()
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
    match tournament_draws::table
        .filter({
            let draws_subquery = diesel::alias!(tournament_draws as draws);

            tournament_draws::released_at.ge(draws_subquery
                .filter(
                    draws_subquery
                        .field(tournament_draws::round_id)
                        .eq(&round_id),
                )
                .select(diesel::dsl::max(
                    draws_subquery.field(tournament_draws::released_at),
                ))
                .single_value())
        })
        .inner_join(
            tournament_debates::table.on(diesel::dsl::exists(
                tournament_debate_judges::table
                    .filter(tournament_debate_judges::judge_id.eq(&judge_id))
                    .filter(
                        tournament_debate_judges::debate_id
                            .eq(tournament_debates::id),
                    ),
            )),
        )
        .select(tournament_debates::all_columns)
        .first::<Debate>(conn)
        .optional()
        .unwrap()
    {
        Some(debate) => Ok(debate),
        None => err_not_found().map(|_| unreachable!()),
    }
}
