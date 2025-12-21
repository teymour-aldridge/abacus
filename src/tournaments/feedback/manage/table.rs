use axum::extract::{Path, Query};
use diesel::prelude::*;
use diesel::sql_types::{Bool, Text};
use hypertext::{Renderable, maud, prelude::*};
use serde::Deserialize;
use std::collections::HashMap;

use crate::{
    auth::User,
    schema::{
        feedback_from_judges_question_answers,
        feedback_from_teams_question_answers, feedback_questions,
        tournament_judges,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament, manage::sidebar::SidebarWrapper, rounds::TournamentRounds,
    },
    util_resp::{StandardResponse, success},
};

#[derive(Deserialize)]
pub struct FeedbackTableQuery {
    page: Option<i64>,
}

#[derive(QueryableByName)]
struct FeedbackRow {
    #[diesel(sql_type = Text)]
    id: String,
    #[diesel(sql_type = Text)]
    round_name: String,
    #[diesel(sql_type = Text)]
    source_name: String,
    #[diesel(sql_type = Text)]
    target_judge_id: String,
    #[diesel(sql_type = Bool)]
    is_latest: bool,
}

pub async fn feedback_table_page(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Query(query): Query<FeedbackTableQuery>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = TournamentRounds::fetch(&tournament_id, &mut *conn).unwrap();

    let questions = feedback_questions::table
        .filter(feedback_questions::tournament_id.eq(&tournament_id))
        .order_by(feedback_questions::seq.asc())
        .select((feedback_questions::id, feedback_questions::question))
        .load::<(String, String)>(&mut *conn)
        .unwrap();

    let page = query.page.unwrap_or(1).max(1);
    let per_page = 128;
    let offset = (page - 1) * per_page;

    let latest_feedback_sql = format!(
        r#"
        WITH latest_judge_feedback AS (
            SELECT
                foj.id,
                tr.name as round_name,
                tj.name || ' (Judge)' as source_name,
                foj.target_judge_id,
                true as is_latest,
                ROW_NUMBER() OVER (
                    PARTITION BY foj.judge_id, foj.debate_id, foj.target_judge_id
                    ORDER BY foj.id DESC
                ) as rn
            FROM feedback_of_judges foj
            INNER JOIN tournament_judges tj ON tj.id = foj.judge_id
            INNER JOIN tournament_debates td ON td.id = foj.debate_id
            INNER JOIN tournament_rounds tr ON tr.id = td.round_id
            WHERE foj.tournament_id = $1
        ),
        latest_team_feedback AS (
            SELECT
                fot.id,
                tr.name as round_name,
                tt.name || ' (Team)' as source_name,
                fot.target_judge_id,
                true as is_latest,
                ROW_NUMBER() OVER (
                    PARTITION BY fot.team_id, fot.debate_id, fot.target_judge_id
                    ORDER BY fot.id DESC
                ) as rn
            FROM feedback_of_teams fot
            INNER JOIN tournament_teams tt ON tt.id = fot.team_id
            INNER JOIN tournament_debates td ON td.id = fot.debate_id
            INNER JOIN tournament_rounds tr ON tr.id = td.round_id
            WHERE fot.tournament_id = $1
        ),
        all_judge_feedback AS (
            SELECT
                foj.id,
                tr.name as round_name,
                tj.name || ' (Judge)' as source_name,
                foj.target_judge_id,
                CASE
                    WHEN ljf.id IS NOT NULL THEN true
                    ELSE false
                END as is_latest
            FROM feedback_of_judges foj
            INNER JOIN tournament_judges tj ON tj.id = foj.judge_id
            INNER JOIN tournament_debates td ON td.id = foj.debate_id
            INNER JOIN tournament_rounds tr ON tr.id = td.round_id
            LEFT JOIN latest_judge_feedback ljf ON ljf.id = foj.id AND ljf.rn = 1
            WHERE foj.tournament_id = $1
        ),
        all_team_feedback AS (
            SELECT
                fot.id,
                tr.name as round_name,
                tt.name || ' (Team)' as source_name,
                fot.target_judge_id,
                CASE
                    WHEN ltf.id IS NOT NULL THEN true
                    ELSE false
                END as is_latest
            FROM feedback_of_teams fot
            INNER JOIN tournament_teams tt ON tt.id = fot.team_id
            INNER JOIN tournament_debates td ON td.id = fot.debate_id
            INNER JOIN tournament_rounds tr ON tr.id = td.round_id
            LEFT JOIN latest_team_feedback ltf ON ltf.id = fot.id AND ltf.rn = 1
            WHERE fot.tournament_id = $1
        ),
        combined_feedback AS (
            SELECT * FROM all_judge_feedback
            UNION ALL
            SELECT * FROM all_team_feedback
        )
        SELECT id, round_name, source_name, target_judge_id, is_latest
        FROM combined_feedback
        ORDER BY is_latest DESC, id DESC
        LIMIT $2 OFFSET $3
        "#
    );

    let feedback_page = diesel::sql_query(latest_feedback_sql)
        .bind::<Text, _>(&tournament_id)
        .bind::<diesel::sql_types::BigInt, _>(per_page)
        .bind::<diesel::sql_types::BigInt, _>(offset)
        .load::<FeedbackRow>(&mut *conn)
        .unwrap();

    let feedback_ids: Vec<String> =
        feedback_page.iter().map(|item| item.id.clone()).collect();

    let judge_answers = feedback_from_judges_question_answers::table
        .filter(
            feedback_from_judges_question_answers::feedback_id
                .eq_any(&feedback_ids),
        )
        .select((
            feedback_from_judges_question_answers::feedback_id,
            feedback_from_judges_question_answers::question_id,
            feedback_from_judges_question_answers::answer,
        ))
        .load::<(String, String, String)>(&mut *conn)
        .unwrap();

    let team_answers = feedback_from_teams_question_answers::table
        .filter(
            feedback_from_teams_question_answers::feedback_id
                .eq_any(&feedback_ids),
        )
        .select((
            feedback_from_teams_question_answers::feedback_id,
            feedback_from_teams_question_answers::question_id,
            feedback_from_teams_question_answers::answer,
        ))
        .load::<(String, String, String)>(&mut *conn)
        .unwrap();

    let mut all_answers: HashMap<String, HashMap<String, String>> =
        HashMap::new();
    for (feedback_id, question_id, answer) in
        judge_answers.into_iter().chain(team_answers.into_iter())
    {
        all_answers
            .entry(feedback_id)
            .or_insert_with(HashMap::new)
            .insert(question_id, answer);
    }

    let mut all_feedback = Vec::new();

    for item in feedback_page {
        let target_name = tournament_judges::table
            .find(&item.target_judge_id)
            .select(tournament_judges::name)
            .first::<String>(&mut *conn)
            .unwrap_or_else(|_| "Unknown".to_string());
        let answers = all_answers.get(&item.id).cloned().unwrap_or_default();
        all_feedback.push(FeedbackDisplayItem {
            id: item.id,
            round_name: item.round_name,
            source_name: item.source_name,
            target_name,
            answers,
            is_latest: item.is_latest,
        });
    }

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(FeedbackTableRenderer {
                tournament,
                rounds,
                all_feedback,
                questions,
                page,
            })
            .render(),
    )
}

struct FeedbackDisplayItem {
    id: String,
    round_name: String,
    source_name: String,
    target_name: String,
    answers: HashMap<String, String>,
    is_latest: bool,
}

struct FeedbackTableRenderer {
    tournament: Tournament,
    rounds: TournamentRounds,
    all_feedback: Vec<FeedbackDisplayItem>,
    questions: Vec<(String, String)>,
    page: i64,
}

impl Renderable for FeedbackTableRenderer {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            SidebarWrapper tournament=(&self.tournament) rounds=(&self.rounds) active_page=(None) selected_seq=(None) {
                div class="d-flex justify-content-between flex-wrap flex-md-nowrap align-items-center pt-3 pb-2 mb-3 border-bottom" {
                    h1 class="h2" { "Feedback Submissions" }
                }

            div class="table-responsive" {
                table class="table table-striped table-sm" {
                    thead {
                        tr {
                            th scope="col" { "Round" }
                            th scope="col" { "From" }
                            th scope="col" { "To (Judge)" }
                            @for (_, question_text) in &self.questions {
                                th scope="col" { (question_text) }
                            }
                            th scope="col" { "ID" }
                        }
                    }
                    tbody {
                        @for item in &self.all_feedback {
                            @let row_class = if item.is_latest { "table-info" } else { "table-secondary text-muted" };
                            tr class=(row_class) {
                                td { (item.round_name) }
                                td { (item.source_name) }
                                td { (item.target_name) }
                                @for (question_id, _) in &self.questions {
                                    td {
                                        @if let Some(answer) = item.answers.get(question_id) {
                                            (answer)
                                        } @else {
                                            span class="text-muted" { "No answer" }
                                        }
                                    }
                                }
                                td { code { (item.id) } }
                            }
                        }
                        @if self.all_feedback.is_empty() {
                            tr {
                                td colspan=(4 + self.questions.len()) class="text-center" { "No feedback found." }
                            }
                        }
                    }
                }
            }

            nav aria-label="Page navigation" {
                ul class="pagination justify-content-center" {
                    @if self.page > 1 {
                        li class="page-item" {
                            a class="page-link" href=(format!("/tournaments/{}/feedback/table?page={}", self.tournament.id, self.page - 1)) { "Previous" }
                        }
                    }
                    li class="page-item disabled" {
                        span class="page-link" { (format!("Page {}", self.page)) }
                    }
                    li class="page-item" {
                        a class="page-link" href=(format!("/tournaments/{}/feedback/table?page={}", self.tournament.id, self.page + 1)) { "Next" }
                    }
                }
            }
            }
        }
        .render_to(buffer);
    }
}
