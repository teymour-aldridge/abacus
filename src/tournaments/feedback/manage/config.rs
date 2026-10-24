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
    tournaments::{
        Tournament, feedback::FeedbackQuestion,
        manage::sidebar::SidebarWrapper, rounds::TournamentRounds,
    },
    util_resp::{StandardResponse, err_not_found},
};

pub async fn manage_feedback_page(
    Path(tournament_id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<
        crate::tournaments::manage::workspace::WorkspaceQuery,
    >,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let access = crate::permission::TournamentAccess::resolve(
        &tournament_id,
        user.as_ref().map(|u| u.id.as_str()),
        &mut *conn,
    )?;
    if !access.is_superuser() {
        if query.editing() {
            return Err(crate::util_resp::FailureResponse::Unauthorized(()));
        }
        return crate::tournaments::manage::workspace::unavailable(
            &tournament,
            user,
            "Feedback questions",
        );
    }
    let user = user.expect("superuser is authenticated");
    let questions = feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .order_by(feedback_questions::seq.asc())
        .load::<FeedbackQuestion>(&mut *conn)?;
    let canonical = format!("/tournaments/{tournament_id}/feedback/manage");
    if !query.editing() {
        return crate::tournaments::manage::workspace::read_page(
            &tournament,
            user,
            "Feedback questions",
            &canonical,
            &access,
            maud! { ol { @for question in &questions { li { (question.question) } } } },
        );
    }
    let rounds = TournamentRounds::fetch(&tournament_id, &mut *conn)?;
    crate::tournaments::manage::workspace::editor_page(
        query.form(),
        "Feedback questions",
        &canonical,
        &tournament.clone(),
        user,
        FeedbackConfigRenderer {
            tournament,
            questions,
            rounds,
        },
        false,
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
            SidebarWrapper tournament=(&self.tournament) rounds=(&self.rounds) active_page=(None) selected_seq=(None) {
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
                                                href=(format!("/tournaments/{}/feedback/manage/{}/edit", self.tournament.id, question.id)) hx-get=(format!("/tournaments/{}/feedback/manage/{}/edit?modal=true", self.tournament.id, question.id)) hx-target="#ui-modal"
                                                class="btn btn-sm btn-outline-primary"
                                            {
                                                "Edit"
                                            }

                                            form method="post" action=(format!("/tournaments/{}/feedback/manage/delete", self.tournament.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/feedback/manage/delete", self.tournament.id), if format!("/tournaments/{}/feedback/manage/delete", self.tournament.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" class="d-inline" {
                                                input type="hidden" name="question_id" value=(question.id);
                                                button type="submit" class="btn btn-sm btn-outline-danger" onclick="return confirm('Are you sure?')" { "Delete" }
                                            }

                                            @if i > 0 {
                                                form method="post" action=(format!("/tournaments/{}/feedback/manage/up", self.tournament.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/feedback/manage/up", self.tournament.id), if format!("/tournaments/{}/feedback/manage/up", self.tournament.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" class="d-inline" {
                                                    input type="hidden" name="question_id" value=(question.id);
                                                    button type="submit" class="btn btn-sm btn-outline-secondary" { "Up" }
                                                }
                                            }

                                            @if i < self.questions.len() - 1 {
                                                form method="post" action=(format!("/tournaments/{}/feedback/manage/down", self.tournament.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/feedback/manage/down", self.tournament.id), if format!("/tournaments/{}/feedback/manage/down", self.tournament.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" class="d-inline" {
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
                (question_form(&format!("/tournaments/{}/feedback/manage/add",self.tournament.id),None,"","score",None,Some((false,false)),None))
            }
        }
        .render_to(buffer);
    }
}

pub async fn edit_feedback_question_page(
    Path((tournament_id, question_id)): Path<(String, String)>,
    axum::extract::Query(query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let rounds = TournamentRounds::fetch(&tournament_id, &mut *conn).unwrap();

    let question = feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .filter(feedback_questions::id.eq(question_id))
        .first::<FeedbackQuestion>(&mut *conn)
        .optional()
        .unwrap();

    if let Some(question) = question {
        crate::tournaments::manage::workspace::editor_page(
            query,
            "Edit feedback question",
            &format!("/tournaments/{tournament_id}/feedback/manage"),
            &tournament.clone(),
            user,
            EditFeedbackQuestionRenderer {
                tournament,
                question,
                rounds,
            },
            false,
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
            SidebarWrapper tournament=(&self.tournament) rounds=(&self.rounds) active_page=(None) selected_seq=(None) {
                h1 { "Edit Feedback Question" }
                (question_form(&format!("/tournaments/{}/feedback/manage/{}/edit",self.tournament.id,self.question.id),Some(&self.question.id),&self.question.question,kind_name(&self.question.kind),Some(&self.question.seq.to_string()),None,None))

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
    Text {},
    Boolean {},
}

fn feedback_question_kind_from_form(
    kind: &str,
) -> Option<FeedbackQuestionKind> {
    match kind {
        // todo: should be possible to add non 1-10 scales
        "score" => Some(FeedbackQuestionKind::IntegerScale { min: 1, max: 10 }),
        "text" => Some(FeedbackQuestionKind::Text {}),
        "bool" => Some(FeedbackQuestionKind::Boolean {}),
        _ => None,
    }
}

pub async fn add_feedback_question(
    Path(tournament_id): Path<String>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<AddQuestionForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let feedback_q_kind = feedback_question_kind_from_form(&form.kind);
    if feedback_q_kind.is_none() || form.question.trim().is_empty() {
        return invalid_question(
            modal_query,
            &tournament_id,
            &tournament,
            user,
            None,
            &form.question,
            &form.kind,
            None,
            Some((form.for_judges, form.for_teams)),
        );
    }
    let feedback_q_kind = feedback_q_kind.unwrap();

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

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!(
            "/tournaments/{}/feedback/manage",
            tournament_id
        )),
    )
}

#[derive(Deserialize)]
pub struct EditQuestionForm {
    question_id: String,
    question: String,
    kind: String,
    seq: String,
}

pub async fn edit_feedback_question(
    Path((tournament_id, question_id)): Path<(String, String)>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<
        EditQuestionForm,
    >,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .filter(feedback_questions::id.eq(&form.question_id))
        .select(feedback_questions::id)
        .first::<String>(&mut *conn)
        .optional()?
        .ok_or(crate::util_resp::FailureResponse::NotFound(()))?;

    if question_id != form.question_id {
        return err_not_found();
    }
    let seq = form.seq.parse::<i64>().ok().filter(|seq| *seq >= 0);
    let feedback_q_kind = feedback_question_kind_from_form(&form.kind);
    if feedback_q_kind.is_none()
        || form.question.trim().is_empty()
        || seq.is_none()
    {
        return invalid_question(
            modal_query,
            &tournament_id,
            &tournament,
            user,
            Some(&form.question_id),
            &form.question,
            &form.kind,
            Some(&form.seq),
            None,
        );
    }
    let feedback_q_kind = feedback_q_kind.unwrap();

    diesel::update(feedback_questions::table.find(&form.question_id))
        .set((
            feedback_questions::question.eq(&form.question),
            feedback_questions::kind
                .eq(serde_json::to_string(&feedback_q_kind).unwrap()),
            feedback_questions::seq.eq(seq.unwrap()),
        ))
        .execute(&mut *conn)
        .unwrap();

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!(
            "/tournaments/{}/feedback/manage",
            tournament_id
        )),
    )
}

#[derive(Deserialize)]
pub struct DeleteQuestionForm {
    question_id: String,
}

pub async fn delete_feedback_question(
    Path(tournament_id): Path<String>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<
        DeleteQuestionForm,
    >,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .filter(feedback_questions::id.eq(&form.question_id))
        .select(feedback_questions::id)
        .first::<String>(&mut *conn)
        .optional()?
        .ok_or(crate::util_resp::FailureResponse::NotFound(()))?;

    diesel::delete(feedback_questions::table.find(&form.question_id))
        .execute(&mut *conn)
        .unwrap();

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!(
            "/tournaments/{}/feedback/manage",
            tournament_id
        )),
    )
}

#[derive(Deserialize)]
pub struct ReorderQuestionForm {
    question_id: String,
}

pub async fn move_feedback_question_up(
    Path(tournament_id): Path<String>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    axum_extra::extract::Form(form): axum_extra::extract::Form<
        ReorderQuestionForm,
    >,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .filter(feedback_questions::id.eq(&form.question_id))
        .select(feedback_questions::id)
        .first::<String>(&mut *conn)
        .optional()?
        .ok_or(crate::util_resp::FailureResponse::NotFound(()))?;

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

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!(
            "/tournaments/{}/feedback/manage",
            tournament_id
        )),
    )
}

pub async fn move_feedback_question_down(
    Path(tournament_id): Path<String>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<ReorderQuestionForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .filter(feedback_questions::id.eq(&form.question_id))
        .select(feedback_questions::id)
        .first::<String>(&mut *conn)
        .optional()?
        .ok_or(crate::util_resp::FailureResponse::NotFound(()))?;

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

    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!(
            "/tournaments/{}/feedback/manage",
            tournament_id
        )),
    )
}

fn kind_name(raw: &str) -> &str {
    if raw.contains("IntegerScale") {
        "score"
    } else if raw.contains("Boolean") {
        "bool"
    } else {
        "text"
    }
}
fn question_form<'a>(
    action: &'a str,
    id: Option<&'a str>,
    question: &'a str,
    kind: &'a str,
    seq: Option<&'a str>,
    audience: Option<(bool, bool)>,
    error: Option<&'a str>,
) -> impl Renderable + 'a {
    crate::tournaments::manage::workspace::submitted_form(
        action,
        maud! {
            @if let Some(id)=id { input type="hidden" name="question_id" value=(id); }
            @if let Some(seq)=seq { input type="hidden" name="seq" value=(seq); }
            label class="form-label d-block" { "Question" input class="form-control" name="question" value=(question) required; }
            label class="form-label d-block" { "Answer type"
                select class="form-select" name="kind" {
                    @for (value, title) in [("score","Score (1–10)"),("text","Text"),("bool","Yes or no")] {
                        option value=(value) selected[kind==value] { (title) }
                    }
                }
            }
            @if let Some((judges, teams))=audience {
                label class="form-check-label d-block" { input type="checkbox" name="for_judges" value="true" checked[judges]; " For judges" }
                label class="form-check-label d-block" { input type="checkbox" name="for_teams" value="true" checked[teams]; " For teams" }
            }
        },
        error,
    )
}
fn invalid_question(
    query: crate::ui::response::ModalQuery,
    tid: &str,
    tournament: &Tournament,
    user: User<true>,
    id: Option<&str>,
    question: &str,
    kind: &str,
    seq: Option<&str>,
    audience: Option<(bool, bool)>,
) -> StandardResponse {
    let action = match id {
        Some(id) => format!("/tournaments/{tid}/feedback/manage/{id}/edit"),
        None => format!("/tournaments/{tid}/feedback/manage/add"),
    };
    crate::tournaments::manage::workspace::editor_page(
        query,
        "Feedback question",
        &format!("/tournaments/{tid}/feedback/manage"),
        tournament,
        user,
        question_form(
            &action,
            id,
            question,
            kind,
            seq,
            audience,
            Some(
                "Enter a question and choose an answer type. Display order must be a non-negative whole number.",
            ),
        ),
        true,
    )
}
