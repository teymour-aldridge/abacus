use std::collections::HashMap;

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use hypertext::{Renderable, maud, prelude::*};
use rocket::{FromForm, form::Form, get, post};
use uuid::Uuid;

use crate::{
    auth::User,
    schema::{
        feedback_from_judges_question_answers,
        feedback_from_teams_question_answers, feedback_of_judges,
        feedback_of_teams, feedback_questions, tournament_debate_judges,
        tournament_debates, tournament_judges, tournament_rounds,
        tournament_speakers, tournament_team_speakers,
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
        manage::sidebar::SidebarWrapper,
        participants::{Judge, Speaker},
        privateurls::ParticipantKind,
        rounds::{Round, TournamentRounds, draws::Debate},
    },
    util_resp::{FailureResponse, StandardResponse, err_not_found, success},
};

#[get(
    "/tournaments/<tournament_id>/privateurls/<private_url>/rounds/<round_id>/feedback/submit"
)]
pub async fn submit_feedback_page(
    tournament_id: &str,
    private_url: &str,
    round_id: &str,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    let round = Round::fetch(round_id, &mut *conn)?;
    let rounds = TournamentRounds::fetch(tournament_id, &mut *conn).unwrap();

    let judge = tournament_judges::table
        .filter(
            tournament_judges::private_url
                .eq(private_url)
                .and(tournament_judges::tournament_id.eq(tournament_id)),
        )
        .first::<Judge>(&mut *conn)
        .optional()
        .unwrap();

    let speaker = if judge.is_none() {
        tournament_speakers::table
            .filter(
                tournament_speakers::private_url
                    .eq(private_url)
                    .and(tournament_speakers::tournament_id.eq(tournament_id)),
            )
            .first::<Speaker>(&mut *conn)
            .optional()
            .unwrap()
    } else {
        None
    };

    if judge.is_none() && speaker.is_none() {
        return err_not_found();
    }

    let targets = if let Some(judge) = &judge {
        let debate =
            debate_of_judge_in_round(&judge.id, &round.id, &mut *conn)?;
        let targets =
            get_feedback_targets_for_judge(&judge.id, &debate.id, &mut *conn);
        targets
    } else {
        let speaker = speaker.as_ref().unwrap();
        let debate =
            debate_of_speaker_in_round(&speaker.id, &round.id, &mut *conn)?;
        let targets = get_feedback_targets_for_speaker(&debate.id, &mut *conn);
        targets
    };

    let questions = feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(tournament_id))
        .order_by(feedback_questions::seq.asc())
        .load::<FeedbackQuestion>(&mut *conn)
        .unwrap();

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .body(FeedbackFormRenderer {
                round,
                targets,
                questions,
                submitter_kind: if judge.is_some() {
                    ParticipantKind::Judge
                } else {
                    ParticipantKind::Speaker
                },
                tournament,
                rounds,
            })
            .render(),
    )
}

struct FeedbackFormRenderer {
    round: Round,
    targets: Vec<Judge>,
    questions: Vec<FeedbackQuestion>,
    submitter_kind: ParticipantKind,
    rounds: TournamentRounds,
    tournament: Tournament,
}

impl Renderable for FeedbackFormRenderer {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            SidebarWrapper rounds=(&self.rounds) tournament=(&self.tournament) {
                h1 { "Submit feedback" }
                h2 { (self.round.name) }

                form method="post" {
                    div class="mb-3" {
                        label for="target_judge_id" class="form-label" { "Feedback for:" }
                        select class="form-select" name="target_judge_id" id="target_judge_id" required {
                            option value="" disabled selected { "Select a judge" }
                            @for target in &self.targets {
                                option value=(target.id) { (target.name) }
                            }
                        }
                    }

                    @for question in &self.questions {
                        @let kind: FeedbackQuestionKind = serde_json::from_str(&question.kind).unwrap();
                        @if question.for_judges && matches!(self.submitter_kind, ParticipantKind::Judge)
                            || question.for_teams && matches!(self.submitter_kind, ParticipantKind::Speaker) {
                                div class="mb-3" {
                                    label for=(format!("answers[{}]", question.id)) class="form-label" { (question.question) }
                                    @match kind {
                                        FeedbackQuestionKind::IntegerScale { .. } => {
                                            input type="number" class="form-control" name=(format!("answers[{}]", question.id)) min="1" max="10" required;
                                        }
                                        FeedbackQuestionKind::Text => {
                                            textarea class="form-control" name=(format!("answers[{}]", question.id)) rows="3" required {}
                                        }
                                        FeedbackQuestionKind::Boolean => {
                                            input type="checkbox" class="form-check-input" name=(format!("answers[{}]", question.id)) required;
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

#[derive(FromForm)]
pub struct FeedbackSubmissionForm {
    target_judge_id: String,
    answers: HashMap<String, String>,
}

#[post(
    "/tournaments/<tournament_id>/privateurls/<private_url>/rounds/<round_id>/feedback/submit",
    data = "<form>"
)]
pub async fn do_submit_feedback(
    tournament_id: &str,
    private_url: &str,
    round_id: &str,
    user: Option<User<true>>,
    mut conn: Conn<true>,
    form: Form<FeedbackSubmissionForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    let round = Round::fetch(round_id, &mut *conn)?;

    // Identify submitter
    let judge = tournament_judges::table
        .filter(
            tournament_judges::private_url
                .eq(private_url)
                .and(tournament_judges::tournament_id.eq(tournament_id)),
        )
        .first::<Judge>(&mut *conn)
        .optional()
        .unwrap();

    let speaker = if judge.is_none() {
        tournament_speakers::table
            .filter(
                tournament_speakers::private_url
                    .eq(private_url)
                    .and(tournament_speakers::tournament_id.eq(tournament_id)),
            )
            .first::<Speaker>(&mut *conn)
            .optional()
            .unwrap()
    } else {
        None
    };

    if judge.is_none() && speaker.is_none() {
        return err_not_found();
    }

    let (debate, _targets) = if let Some(judge) = &judge {
        let debate =
            debate_of_judge_in_round(&judge.id, &round.id, &mut *conn)?;
        let targets =
            get_feedback_targets_for_judge(&judge.id, &debate.id, &mut *conn);
        (debate, targets)
    } else {
        let speaker = speaker.as_ref().unwrap();
        let debate =
            debate_of_speaker_in_round(&speaker.id, &round.id, &mut *conn)?;
        let targets = get_feedback_targets_for_speaker(&debate.id, &mut *conn);
        (debate, targets)
    };

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
                diesel::insert_into(
                    feedback_from_judges_question_answers::table,
                )
                .values(FeedbackFromJudgesQuestionAnswer {
                    id: Uuid::now_v7().to_string(),
                    feedback_id: feedback_id.clone(),
                    question_id: q_id.clone(),
                    answer: ans.clone(),
                })
                .execute(conn)?;
            }
        } else if let Some(speaker) = &speaker {
            let team_id = tournament_team_speakers::table
                .filter(tournament_team_speakers::speaker_id.eq(&speaker.id))
                .select(tournament_team_speakers::team_id)
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
                diesel::insert_into(
                    feedback_from_teams_question_answers::table,
                )
                .values(FeedbackFromTeamsQuestionAnswer {
                    id: Uuid::now_v7().to_string(),
                    feedback_id: feedback_id.clone(),
                    question_id: q_id.clone(),
                    answer: ans.clone(),
                })
                .execute(conn)?;
            }
        }

        Ok(())
    })
    .unwrap();

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament)
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
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<Debate, FailureResponse> {
    match tournament_debates::table
        .inner_join(
            tournament_rounds::table
                .on(tournament_rounds::id.eq(tournament_debates::round_id)),
        )
        .filter(tournament_rounds::id.eq(round_id))
        //.filter(tournament_rounds::draw_status.eq("R"))
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

fn debate_of_speaker_in_round(
    speaker_id: &str,
    round_id: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<Debate, FailureResponse> {
    let team_id = tournament_team_speakers::table
        .filter(tournament_team_speakers::speaker_id.eq(speaker_id))
        .select(tournament_team_speakers::team_id)
        .first::<String>(conn)
        .optional()
        .unwrap();

    if let Some(tid) = team_id {
        match tournament_debates::table
            .inner_join(
                tournament_rounds::table
                    .on(tournament_rounds::id.eq(tournament_debates::round_id)),
            )
            .filter(tournament_rounds::id.eq(round_id))
            .filter(diesel::dsl::exists(
                crate::schema::tournament_debate_teams::table
                    .filter(
                        crate::schema::tournament_debate_teams::team_id.eq(tid),
                    )
                    .filter(
                        crate::schema::tournament_debate_teams::debate_id
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
    } else {
        err_not_found().map(|_| unreachable!())
    }
}

fn get_feedback_targets_for_judge(
    judge_id: &str,
    debate_id: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Vec<Judge> {
    let status = tournament_debate_judges::table
        .filter(tournament_debate_judges::debate_id.eq(debate_id))
        .filter(tournament_debate_judges::judge_id.eq(judge_id))
        .select(tournament_debate_judges::status)
        .first::<String>(conn)
        .unwrap_or_default();

    let _target_status = if status == "c" { "w" } else { "c" };

    tournament_judges::table
        .inner_join(
            tournament_debate_judges::table
                .on(tournament_debate_judges::judge_id
                    .eq(tournament_judges::id)),
        )
        .filter(tournament_debate_judges::debate_id.eq(debate_id))
        .filter(tournament_judges::id.ne(judge_id))
        .select(tournament_judges::all_columns)
        .load::<Judge>(conn)
        .unwrap_or_default()
}

fn get_feedback_targets_for_speaker(
    debate_id: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Vec<Judge> {
    tournament_judges::table
        .inner_join(
            tournament_debate_judges::table
                .on(tournament_debate_judges::judge_id
                    .eq(tournament_judges::id)),
        )
        .filter(tournament_debate_judges::debate_id.eq(debate_id))
        .filter(tournament_debate_judges::status.eq("c"))
        .select(tournament_judges::all_columns)
        .load::<Judge>(conn)
        .unwrap_or_default()
}
