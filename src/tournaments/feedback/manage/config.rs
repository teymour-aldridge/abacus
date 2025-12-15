use axum::{
    extract::{Form, Path},
    response::Redirect,
};
use diesel::prelude::*;
use hypertext::{Renderable, maud, prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::User,
    schema::feedback_questions,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament, feedback::FeedbackQuestion,
        manage::sidebar::SidebarWrapper, rounds::TournamentRounds,
    },
    util_resp::{
        StandardResponse, bad_request, err_not_found, see_other_ok, success,
    },
};

pub async fn manage_feedback_page(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let questions = feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .order_by(feedback_questions::seq.asc())
        .load::<FeedbackQuestion>(&mut *conn)
        .unwrap();

    let rounds = TournamentRounds::fetch(&tournament_id, &mut *conn).unwrap();

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(FeedbackConfigRenderer {
                tournament,
                questions,
                rounds,
            })
            .render(),
    )
}

struct FeedbackConfigRenderer {
    tournament: Tournament,
    questions: Vec<FeedbackQuestion>,
    rounds: TournamentRounds,
}

impl Renderable for FeedbackConfigRenderer {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            SidebarWrapper tournament=(&self.tournament) rounds=(&self.rounds) {
                div class="d-flex justify-content-between flex-wrap flex-md-nowrap align-items-center pt-3 pb-2 mb-3 border-bottom" {
                    h1 class="h2" { "Feedback Configuration" }
                }

                div class="table-responsive" {
                    table class="table table-striped table-sm" {
                        thead {
                            tr {
                                th scope="col" { "#" }
                                th scope="col" { "Question" }
                                th scope="col" { "Type" }
                                th scope="col" { "Actions" }
                            }
                        }
                        tbody {
                            @for (i, question) in self.questions.iter().enumerate() {
                                tr {
                                    td { (question.seq) }
                                    td { (question.question) }
                                    td { (question.kind) }
                                    td {
                                        div class="btn-group" role="group" {
                                            a
                                                href=(format!("/tournaments/{}/feedback/manage/{}/edit", self.tournament.id, question.id))
                                                class="btn btn-sm btn-outline-primary"
                                            {
                                                "Edit"
                                            }

                                            form method="post" action=(format!("/tournaments/{}/feedback/manage/delete", self.tournament.id)) class="d-inline" {
                                                input type="hidden" name="question_id" value=(question.id);
                                                button type="submit" class="btn btn-sm btn-outline-danger" onclick="return confirm('Are you sure?')" { "Delete" }
                                            }

                                            @if i > 0 {
                                                form method="post" action=(format!("/tournaments/{}/feedback/manage/up", self.tournament.id)) class="d-inline" {
                                                    input type="hidden" name="question_id" value=(question.id);
                                                    button type="submit" class="btn btn-sm btn-outline-secondary" { "Up" }
                                                }
                                            }

                                            @if i < self.questions.len() - 1 {
                                                form method="post" action=(format!("/tournaments/{}/feedback/manage/down", self.tournament.id)) class="d-inline" {
                                                    input type="hidden" name="question_id" value=(question.id);
                                                    button type="submit" class="btn btn-sm btn-outline-secondary" { "Down" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                h3 { "Add New Question" }
                form method="post" action=(format!("/tournaments/{}/feedback/manage/add", self.tournament.id)) {
                    div class="mb-3" {
                        label for="question" class="form-label" { "Question" }
                        input type="text" class="form-control" id="question" name="question" required;
                    }
                    div class="mb-3" {
                        label for="kind" class="form-label" { "Type" }
                        select class="form-select" id="kind" name="kind" required {
                            option value="score" { "Score (1-10)" }
                            option value="text" { "Text" }
                            option value="bool" { "Yes/No" }
                        }
                    }
                    div class="mb-3" {
                        label for="judges"  class="form-labale" {"For judges?"}
                        input type="checkbox" class="form-check-input" id="judges" name="for_judges" value="true";
                    }
                    div class="mb-3" {
                        label for="teams" class="form-labale" {"For teams?"}
                        input type="checkbox" class="form-check-input" id="teams" name="for_teams" value="true";
                    }
                    button type="submit" class="btn btn-primary" { "Add Question" }
                }
            }
        }
        .render_to(buffer);
    }
}

pub async fn edit_feedback_question_page(
    Path((tournament_id, question_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let rounds = TournamentRounds::fetch(&tournament_id, &mut *conn).unwrap();

    let question = feedback_questions::table
        .find(question_id)
        .first::<FeedbackQuestion>(&mut *conn)
        .optional()
        .unwrap();

    if let Some(question) = question {
        success(
            Page::new()
                .user(user)
                .tournament(tournament.clone())
                .body(EditFeedbackQuestionRenderer {
                    tournament,
                    question,
                    rounds,
                })
                .render(),
        )
    } else {
        err_not_found()
    }
}

struct EditFeedbackQuestionRenderer {
    tournament: Tournament,
    question: FeedbackQuestion,
    rounds: TournamentRounds,
}

impl Renderable for EditFeedbackQuestionRenderer {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            SidebarWrapper tournament=(&self.tournament) rounds=(&self.rounds) {
                h1 { "Edit Feedback Question" }
                form method="post" action=(format!("/tournaments/{}/feedback/manage/{}/edit", self.tournament.id, self.question.id)) {
                    @let kind: FeedbackQuestionKind = serde_json::from_str(&self.question.kind).unwrap();
                    input type="hidden" name="question_id" value=(self.question.id);
                    div class="mb-3" {
                        label for="question" class="form-label" { "Question" }
                        input
                            type="text"
                            class="form-control"
                            id="question"
                            name="question"
                            value=(self.question.question)
                            required;
                    }
                    div class="mb-3" {
                        label for="kind" class="form-label" { "Type" }
                        select class="form-select" id="kind" name="kind" required {
                            option value="score" selected[matches!(kind, FeedbackQuestionKind::IntegerScale { .. })] {
                                "Score (1-10)"
                            }
                            option value="text" selected[matches!(kind, FeedbackQuestionKind::Text)] {
                                "Text"
                            }
                            option value="bool" selected[matches!(kind, FeedbackQuestionKind::Boolean)] {
                                "Yes/No"
                            }
                        }
                    }
                    input type="hidden" name="seq" value=(self.question.seq);

                    button type="submit" class="btn btn-primary" { "Save Changes" }
                    a
                        href=(format!("/tournaments/{}/feedback/manage", self.tournament.id))
                        class="btn btn-secondary ms-2"
                    {
                        "Cancel"
                    }
                }

            }
        }
        .render_to(buffer);
    }
}

#[derive(Deserialize)]
pub struct AddQuestionForm {
    question: String,
    kind: String,
    #[serde(default)]
    for_judges: bool,
    #[serde(default)]
    for_teams: bool,
}

#[derive(Serialize, Deserialize)]
pub enum FeedbackQuestionKind {
    IntegerScale { min: i64, max: i64 },
    Text,
    Boolean,
}

pub async fn add_feedback_question(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<AddQuestionForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let feedback_q_kind = match form.kind.as_str() {
        "score" => FeedbackQuestionKind::IntegerScale { min: 1, max: 10 },
        "text" => FeedbackQuestionKind::Text,
        "bool" => FeedbackQuestionKind::Boolean,
        _ => {
            // todo: proper error message
            return bad_request(
                maud! {
                    "Error: invalid question kind."
                }
                .render(),
            );
        }
    };

    let max_seq = feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .select(diesel::dsl::max(feedback_questions::seq))
        .first::<Option<i64>>(&mut *conn)
        .unwrap()
        .unwrap_or(0);

    let new_question = FeedbackQuestion {
        id: Uuid::now_v7().to_string(),
        tournament_id: tournament_id.to_string(),
        question: form.question.clone(),
        kind: serde_json::to_string(&feedback_q_kind).unwrap(),
        seq: max_seq + 1,
        for_judges: form.for_judges,
        for_teams: form.for_teams,
    };

    diesel::insert_into(feedback_questions::table)
        .values(&new_question)
        .execute(&mut *conn)
        .unwrap();

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/feedback/manage",
        tournament_id
    )))
}

#[derive(Deserialize)]
pub struct EditQuestionForm {
    question_id: String,
    question: String,
    kind: String,
    seq: i64,
}

pub async fn edit_feedback_question(
    Path((tournament_id, _question_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<
        EditQuestionForm,
    >,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    diesel::update(feedback_questions::table.find(&form.question_id))
        .set((
            feedback_questions::question.eq(&form.question),
            feedback_questions::kind.eq(&form.kind),
            feedback_questions::seq.eq(&form.seq),
        ))
        .execute(&mut *conn)
        .unwrap();

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/feedback/manage",
        tournament_id
    )))
}

#[derive(Deserialize)]
pub struct DeleteQuestionForm {
    question_id: String,
}

pub async fn delete_feedback_question(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<
        DeleteQuestionForm,
    >,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    diesel::delete(feedback_questions::table.find(&form.question_id))
        .execute(&mut *conn)
        .unwrap();

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/feedback/manage",
        tournament_id
    )))
}

#[derive(Deserialize)]
pub struct ReorderQuestionForm {
    question_id: String,
}

pub async fn move_feedback_question_up(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<
        ReorderQuestionForm,
    >,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let current_question = feedback_questions::table
        .find(&form.question_id)
        .first::<FeedbackQuestion>(&mut *conn)
        .unwrap();

    let prev_question = feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .filter(feedback_questions::seq.lt(current_question.seq))
        .order_by(feedback_questions::seq.desc())
        .first::<FeedbackQuestion>(&mut *conn)
        .optional()
        .unwrap();

    if let Some(prev) = prev_question {
        let current_seq = current_question.seq;
        let prev_seq = prev.seq;

        diesel::update(feedback_questions::table.find(&current_question.id))
            .set(feedback_questions::seq.eq(prev_seq))
            .execute(&mut *conn)
            .unwrap();

        diesel::update(feedback_questions::table.find(&prev.id))
            .set(feedback_questions::seq.eq(current_seq))
            .execute(&mut *conn)
            .unwrap();
    }

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/feedback/manage",
        tournament_id
    )))
}

pub async fn move_feedback_question_down(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<ReorderQuestionForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let current_question = feedback_questions::table
        .find(&form.question_id)
        .first::<FeedbackQuestion>(&mut *conn)
        .unwrap();

    let next_question = feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .filter(feedback_questions::seq.gt(current_question.seq))
        .order_by(feedback_questions::seq.asc())
        .first::<FeedbackQuestion>(&mut *conn)
        .optional()
        .unwrap();

    if let Some(next) = next_question {
        let current_seq = current_question.seq;
        let next_seq = next.seq;

        diesel::update(feedback_questions::table.find(&current_question.id))
            .set(feedback_questions::seq.eq(next_seq))
            .execute(&mut *conn)
            .unwrap();

        diesel::update(feedback_questions::table.find(&next.id))
            .set(feedback_questions::seq.eq(current_seq))
            .execute(&mut *conn)
            .unwrap();
    }

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/feedback/manage",
        tournament_id
    )))
}
