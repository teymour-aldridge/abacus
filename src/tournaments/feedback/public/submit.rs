use axum::extract::{Form, Path};
use std::collections::HashMap;

use diesel::{connection::LoadConnection, prelude::*};
use hypertext::{Renderable, maud, prelude::*};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::User,
    schema::{
        answers_of_feedback_from_judges, answers_of_feedback_from_teams,
        debates, feedback_of_judges, feedback_of_teams, feedback_questions,
        judges, judges_of_debate, rounds, speakers, speakers_of_team,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        feedback::{
            FeedbackFromJudgesQuestionAnswer, FeedbackFromTeamsQuestionAnswer,
            FeedbackOfJudge, FeedbackOfTeam, FeedbackQuestion,
            manage::config::FeedbackQuestionKind,
        },
        participants::{Judge, Speaker},
        privateurls::ParticipantKind,
        rounds::{Round, draws::Debate},
    },
    util_resp::{FailureResponse, StandardResponse, err_not_found, success},
};

pub async fn submit_feedback_page(
    Path((tournament_id, private_url, round_id)): Path<(
        String,
        String,
        String,
    )>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let round = Round::fetch(&tournament_id, &round_id, &mut *conn)?;

    let judge = judges::table
        .filter(
            judges::private_url
                .eq(&private_url)
                .and(judges::tournament_id.eq(&tournament_id)),
        )
        .first::<Judge>(&mut *conn)
        .optional()?;

    let speaker = if judge.is_none() {
        speakers::table
            .filter(
                speakers::private_url
                    .eq(&private_url)
                    .and(speakers::tournament_id.eq(&tournament_id)),
            )
            .first::<Speaker>(&mut *conn)
            .optional()?
    } else {
        None
    };

    if judge.is_none() && speaker.is_none() {
        return err_not_found();
    }

    if round.draw_status != "released_full" {
        let current_rounds = Round::current_rounds(&tournament_id, &mut *conn)
            .into_iter()
            .filter(|r| r.draw_status == "released_full")
            .collect();
        return crate::util_resp::bad_request(
            Page::new()
                .tournament(tournament.clone())
                .user_opt(user)
                .current_rounds(current_rounds)
                .body(maud! {
                    div class="alert alert-danger" { "Error: feedback submission is not yet possible for this round as the judge list has not been published." }
                })
                .render()
        );
    }

    let targets = if let Some(judge) = &judge {
        let debate =
            debate_of_judge_in_round(&judge.id, &round.id, &mut *conn)?;
        let targets =
            get_feedback_targets_for_judge(&judge.id, &debate.id, &mut *conn)?;
        targets
    } else {
        let speaker = speaker.as_ref().unwrap();
        let debate =
            debate_of_speaker_in_round(&speaker.id, &round.id, &mut *conn)?;
        let targets = get_feedback_targets_for_speaker(&debate.id, &mut *conn)?;
        targets
    };

    let questions = feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .order_by(feedback_questions::seq.asc())
        .load::<FeedbackQuestion>(&mut *conn)
        .unwrap();

    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn)
        .into_iter()
        .filter(|r| r.draw_status == "released_full")
        .collect();

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .body(FeedbackFormRenderer {
                round,
                targets,
                questions,
                submitter_kind: if judge.is_some() {
                    ParticipantKind::Judge
                } else {
                    ParticipantKind::Speaker
                },
                submitted: None,
                error: None,
            })
            .render(),
    )
}

struct FeedbackTarget {
    id: String,
    name: String,
}

struct FeedbackFormRenderer {
    round: Round,
    targets: Vec<FeedbackTarget>,
    questions: Vec<FeedbackQuestion>,
    submitter_kind: ParticipantKind,
    submitted: Option<FeedbackSubmissionForm>,
    error: Option<String>,
}

impl Renderable for FeedbackFormRenderer {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            div class="container py-5 px-4" {
                h1 { "Submit feedback" }
                span class="badge bg-secondary" { "Restricted: Participant and tournament organisers" }
                @if let Some(error) = &self.error { div class="alert alert-danger" { (error) } }
                h2 { (self.round.name) }

                form method="post" {
                    div class="mb-3" {
                        label for="target_judge_id" class="form-label" { "Feedback for:" }
                        select class="form-select" name="target_judge_id" id="target_judge_id" required {
                            option value="" disabled selected { "Select a judge" }
                            @for target in &self.targets {
                                option value=(target.id) selected[self.submitted.as_ref().is_some_and(|f| f.target_judge_id == target.id)] { (target.name) }
                            }
                        }
                    }

                    @for question in &self.questions {
                        @let kind: FeedbackQuestionKind = serde_json::from_str(&question.kind).unwrap();
                        @if (question.for_judges && matches!(self.submitter_kind, ParticipantKind::Judge))
                            || (question.for_teams && matches!(self.submitter_kind, ParticipantKind::Speaker)) {
                                div class="mb-3" {
                                    label for=(question.id) class="form-label" { (question.question) }
                                    @match kind {
                                        FeedbackQuestionKind::IntegerScale { min, max } => {
                                            input type="number" class="form-control" name=(question.id) min=(min) max=(max) value=(self.submitted.as_ref().and_then(|f| f.answers.get(&question.id))) required;
                                        }
                                        FeedbackQuestionKind::Text { .. } => {
                                            textarea class="form-control" name=(question.id) rows="3" required { (self.submitted.as_ref().and_then(|f| f.answers.get(&question.id))) }
                                        }
                                        FeedbackQuestionKind::Boolean { .. } => {
                                            input type="checkbox" class="form-check-input" name=(question.id) checked[self.submitted.as_ref().is_some_and(|f| f.answers.contains_key(&question.id))];
                                        }
                                    }
                                }
                            }
                    }

                    button type="submit" class="btn btn-primary" { "Submit Feedback" }
                }
            }
        }
        .render_to(buffer);
    }
}

#[derive(Deserialize)]
pub struct FeedbackSubmissionForm {
    target_judge_id: String,
    #[serde(flatten)]
    answers: HashMap<String, String>,
}

pub async fn do_submit_feedback(
    Path((tournament_id, private_url, round_id)): Path<(
        String,
        String,
        String,
    )>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
    Form(form): Form<FeedbackSubmissionForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let round = Round::fetch(&tournament_id, &round_id, &mut *conn)?;

    // Identify submitter
    let judge = judges::table
        .filter(
            judges::private_url
                .eq(&private_url)
                .and(judges::tournament_id.eq(&tournament_id)),
        )
        .first::<Judge>(&mut *conn)
        .optional()?;

    let speaker = if judge.is_none() {
        speakers::table
            .filter(
                speakers::private_url
                    .eq(&private_url)
                    .and(speakers::tournament_id.eq(&tournament_id)),
            )
            .first::<Speaker>(&mut *conn)
            .optional()?
    } else {
        None
    };

    if judge.is_none() && speaker.is_none() {
        return err_not_found();
    }

    if round.draw_status != "released_full" {
        let current_rounds = Round::current_rounds(&tournament_id, &mut *conn)
            .into_iter()
            .filter(|r| r.draw_status == "released_full")
            .collect();
        return crate::util_resp::bad_request(
            Page::new()
                .tournament(tournament.clone())
                .user_opt(user)
                .current_rounds(current_rounds)
                .body(maud! {
                    div class="alert alert-danger" { "Error: feedback submission is not yet possible for this round as the judge list has not been published." }
                })
                .render()
        );
    }

    let (debate, targets) = if let Some(judge) = &judge {
        let debate =
            debate_of_judge_in_round(&judge.id, &round.id, &mut *conn)?;
        let targets =
            get_feedback_targets_for_judge(&judge.id, &debate.id, &mut *conn)?;
        (debate, targets)
    } else {
        let speaker = speaker.as_ref().unwrap();
        let debate =
            debate_of_speaker_in_round(&speaker.id, &round.id, &mut *conn)?;
        let targets = get_feedback_targets_for_speaker(&debate.id, &mut *conn)?;
        (debate, targets)
    };

    let questions = feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .order_by(feedback_questions::seq.asc())
        .load::<FeedbackQuestion>(&mut *conn)?;
    let submitter_kind = if judge.is_some() {
        ParticipantKind::Judge
    } else {
        ParticipantKind::Speaker
    };
    if let Err(error) =
        validate_submission(&form, &targets, &questions, &submitter_kind)
    {
        return crate::util_resp::bad_request(
            Page::new()
                .user_opt(user)
                .tournament(tournament)
                .body(FeedbackFormRenderer {
                    round,
                    targets,
                    questions,
                    submitter_kind,
                    submitted: Some(form),
                    error: Some(error),
                })
                .render(),
        );
    }

    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        let feedback_id = Uuid::now_v7().to_string();

        if let Some(judge) = &judge {
            diesel::insert_into(feedback_of_judges::table)
                .values(FeedbackOfJudge {
                    id: feedback_id.clone(),
                    tournament_id: tournament.id.clone(),
                    debate_id: debate.id.clone(),
                    judge_id: judge.id.clone(),
                    target_judge_id: form.target_judge_id.clone(),
                })
                .execute(conn)?;

            for (q_id, ans) in &form.answers {
                diesel::insert_into(answers_of_feedback_from_judges::table)
                    .values(FeedbackFromJudgesQuestionAnswer {
                        id: Uuid::now_v7().to_string(),
                        tournament_id: tournament.id.clone(),
                        feedback_id: feedback_id.clone(),
                        question_id: q_id.clone(),
                        answer: ans.clone(),
                    })
                    .execute(conn)?;
            }
        } else if let Some(speaker) = &speaker {
            let team_id = speakers_of_team::table
                .filter(speakers_of_team::speaker_id.eq(&speaker.id))
                .filter(
                    speakers_of_team::team_id.eq_any(
                        crate::schema::teams_of_debate::table
                            .filter(
                                crate::schema::teams_of_debate::debate_id
                                    .eq(&debate.id),
                            )
                            .select(crate::schema::teams_of_debate::team_id),
                    ),
                )
                .select(speakers_of_team::team_id)
                .first::<String>(conn)?;

            diesel::insert_into(feedback_of_teams::table)
                .values(FeedbackOfTeam {
                    id: feedback_id.clone(),
                    tournament_id: tournament.id.clone(),
                    debate_id: debate.id.clone(),
                    team_id: team_id,
                    target_judge_id: form.target_judge_id.clone(),
                })
                .execute(conn)?;

            for (q_id, ans) in &form.answers {
                diesel::insert_into(answers_of_feedback_from_teams::table)
                    .values(FeedbackFromTeamsQuestionAnswer {
                        id: Uuid::now_v7().to_string(),
                        tournament_id: tournament.id.clone(),
                        feedback_id: feedback_id.clone(),
                        question_id: q_id.clone(),
                        answer: ans.clone(),
                    })
                    .execute(conn)?;
            }
        }

        Ok(())
    })?;

    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn)
        .into_iter()
        .filter(|r| r.draw_status == "released_full")
        .collect();

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament)
            .current_rounds(current_rounds)
            .body(maud! {
                div class="alert alert-success" { "Feedback submitted successfully!" }
                a href=(format!("/tournaments/{}/privateurls/{}/rounds/{}/feedback/submit", tournament_id, private_url, round_id)) { "Submit another" }
            })
            .render(),
    )
}

fn debate_of_judge_in_round(
    judge_id: &str,
    round_id: &str,
    conn: &mut impl LoadConnection<Backend = diesel::sqlite::Sqlite>,
) -> Result<Debate, FailureResponse> {
    match debates::table
        .inner_join(rounds::table.on(rounds::id.eq(debates::round_id)))
        .filter(rounds::id.eq(round_id))
        .filter(rounds::draw_status.eq("released_full"))
        .filter(diesel::dsl::exists(
            judges_of_debate::table
                .filter(judges_of_debate::judge_id.eq(judge_id))
                .filter(judges_of_debate::debate_id.eq(debates::id)),
        ))
        .select(debates::all_columns)
        .first::<Debate>(conn)
        .optional()?
    {
        Some(debate) => Ok(debate),
        None => err_not_found().map(|_| unreachable!()),
    }
}

fn debate_of_speaker_in_round(
    speaker_id: &str,
    round_id: &str,
    conn: &mut impl LoadConnection<Backend = diesel::sqlite::Sqlite>,
) -> Result<Debate, FailureResponse> {
    debates::table
        .inner_join(rounds::table.on(rounds::id.eq(debates::round_id)))
        .filter(rounds::id.eq(round_id))
        .filter(rounds::draw_status.eq("released_full"))
        .filter(diesel::dsl::exists(
            crate::schema::teams_of_debate::table
                .filter(
                    crate::schema::teams_of_debate::team_id.eq_any(
                        speakers_of_team::table
                            .filter(speakers_of_team::speaker_id.eq(speaker_id))
                            .select(speakers_of_team::team_id),
                    ),
                )
                .filter(
                    crate::schema::teams_of_debate::debate_id.eq(debates::id),
                ),
        ))
        .select(debates::all_columns)
        .first::<Debate>(conn)
        .map_err(Into::into)
}

fn get_feedback_targets_for_judge(
    judge_id: &str,
    debate_id: &str,
    conn: &mut impl LoadConnection<Backend = diesel::sqlite::Sqlite>,
) -> Result<Vec<FeedbackTarget>, FailureResponse> {
    judges::table
        .inner_join(
            judges_of_debate::table
                .on(judges_of_debate::judge_id.eq(judges::id)),
        )
        .filter(judges_of_debate::debate_id.eq(debate_id))
        .filter(judges::id.ne(judge_id))
        .select((judges::id, judges::name))
        .load::<(String, String)>(conn)
        .map(|rows| {
            rows.into_iter()
                .map(|(id, name)| FeedbackTarget { id, name })
                .collect()
        })
        .map_err(Into::into)
}

fn get_feedback_targets_for_speaker(
    debate_id: &str,
    conn: &mut impl LoadConnection<Backend = diesel::sqlite::Sqlite>,
) -> Result<Vec<FeedbackTarget>, FailureResponse> {
    judges::table
        .inner_join(
            judges_of_debate::table
                .on(judges_of_debate::judge_id.eq(judges::id)),
        )
        .filter(judges_of_debate::debate_id.eq(debate_id))
        .filter(judges_of_debate::status.eq("C"))
        .select((judges::id, judges::name))
        .load::<(String, String)>(conn)
        .map(|rows| {
            rows.into_iter()
                .map(|(id, name)| FeedbackTarget { id, name })
                .collect()
        })
        .map_err(Into::into)
}

fn validate_submission(
    form: &FeedbackSubmissionForm,
    targets: &[FeedbackTarget],
    questions: &[FeedbackQuestion],
    kind: &ParticipantKind,
) -> Result<(), String> {
    if !targets
        .iter()
        .any(|target| target.id == form.target_judge_id)
    {
        return Err("Choose a judge from your debate.".into());
    }
    let applies = |q: &&FeedbackQuestion| match kind {
        ParticipantKind::Judge => q.for_judges,
        ParticipantKind::Speaker => q.for_teams,
    };
    let allowed: Vec<_> = questions.iter().filter(applies).collect();
    if form
        .answers
        .keys()
        .any(|id| !allowed.iter().any(|q| &q.id == id))
    {
        return Err("An answer refers to an unavailable question.".into());
    }
    for question in allowed {
        let answer = form
            .answers
            .get(&question.id)
            .map(String::as_str)
            .unwrap_or("");
        let valid = match serde_json::from_str::<FeedbackQuestionKind>(
            &question.kind,
        ) {
            Ok(FeedbackQuestionKind::IntegerScale { min, max }) => answer
                .parse::<i64>()
                .is_ok_and(|value| (min..=max).contains(&value)),
            Ok(FeedbackQuestionKind::Text {}) => !answer.trim().is_empty(),
            Ok(FeedbackQuestionKind::Boolean {}) => {
                answer.is_empty() || matches!(answer, "on" | "true" | "false")
            }
            Err(_) => false,
        };
        if !valid {
            return Err(format!("Check the answer to: {}", question.question));
        }
    }
    Ok(())
}
