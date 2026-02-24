pub mod manage;
pub mod public;

use diesel::prelude::*;
use serde::Serialize;

use crate::schema::{
    answers_of_feedback_from_judges, answers_of_feedback_from_teams,
    feedback_of_judges, feedback_of_teams, feedback_questions,
};

#[derive(
    Queryable, Selectable, Identifiable, Insertable, Debug, Clone, Serialize,
)]
#[diesel(table_name = feedback_questions)]
pub struct FeedbackQuestion {
    pub id: String,
    pub tournament_id: String,
    pub question: String,
    pub kind: String,
    pub seq: i64,
    pub for_judges: bool,
    pub for_teams: bool,
}

#[derive(
    Queryable, Selectable, Identifiable, Insertable, Debug, Clone, Serialize,
)]
#[diesel(table_name = feedback_of_judges)]
pub struct FeedbackOfJudge {
    pub id: String,
    pub tournament_id: String,
    pub debate_id: String,
    pub judge_id: String,
    pub target_judge_id: String,
}

#[derive(
    Queryable, Selectable, Identifiable, Insertable, Debug, Clone, Serialize,
)]
#[diesel(table_name = feedback_of_teams)]
pub struct FeedbackOfTeam {
    pub id: String,
    pub tournament_id: String,
    pub debate_id: String,
    pub team_id: String,
    pub target_judge_id: String,
}

#[derive(
    Queryable, Selectable, Identifiable, Insertable, Debug, Clone, Serialize,
)]
#[diesel(table_name = answers_of_feedback_from_judges)]
pub struct FeedbackFromJudgesQuestionAnswer {
    pub id: String,
    pub feedback_id: String,
    pub question_id: String,
    pub answer: String,
}

#[derive(
    Queryable, Selectable, Identifiable, Insertable, Debug, Clone, Serialize,
)]
#[diesel(table_name = answers_of_feedback_from_teams)]
pub struct FeedbackFromTeamsQuestionAnswer {
    pub id: String,
    pub feedback_id: String,
    pub question_id: String,
    pub answer: String,
}
