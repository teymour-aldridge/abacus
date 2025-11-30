use diesel::prelude::*;
use hypertext::{Renderable, maud, prelude::*};
use rocket::{FromForm, get};

use crate::{
    auth::User,
    schema::{
        feedback_of_judges, feedback_of_teams, tournament_debates,
        tournament_judges, tournament_rounds, tournament_teams,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament, manage::sidebar::SidebarWrapper, rounds::TournamentRounds,
    },
    util_resp::{StandardResponse, success},
};

#[derive(FromForm)]
pub struct FeedbackTableQuery {
    page: Option<i64>,
}

#[get("/tournaments/<tournament_id>/feedback/table?<query..>")]
pub async fn feedback_table_page(
    tournament_id: &str,
    user: User<true>,
    mut conn: Conn<true>,
    query: FeedbackTableQuery,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = TournamentRounds::fetch(tournament_id, &mut *conn).unwrap();

    let page = query.page.unwrap_or(1).max(1);
    let per_page = 128;
    let offset = (page - 1) * per_page;

    let feedback_judges = feedback_of_judges::table
        .inner_join(
            tournament_judges::table
                .on(tournament_judges::id.eq(feedback_of_judges::judge_id)),
        )
        .inner_join(
            tournament_debates::table
                .on(tournament_debates::id.eq(feedback_of_judges::debate_id)),
        )
        .inner_join(
            tournament_rounds::table
                .on(tournament_rounds::id.eq(tournament_debates::round_id)),
        )
        .filter(feedback_of_judges::tournament_id.eq(tournament_id))
        .select((
            feedback_of_judges::id,
            tournament_rounds::name,
            tournament_judges::name,
            feedback_of_judges::target_judge_id,
        ))
        .limit(per_page)
        .offset(offset)
        .load::<(String, String, String, String)>(&mut *conn)
        .unwrap();

    let mut feedback_judges_display = Vec::new();
    for (id, round_name, source_name, target_judge_id) in feedback_judges {
        let target_name = tournament_judges::table
            .find(&target_judge_id)
            .select(tournament_judges::name)
            .first::<String>(&mut *conn)
            .unwrap_or_else(|_| "Unknown".to_string());
        feedback_judges_display.push(FeedbackDisplayItem {
            id,
            round_name,
            source_name,
            target_name,
        });
    }

    let feedback_teams = feedback_of_teams::table
        .inner_join(
            tournament_teams::table
                .on(tournament_teams::id.eq(feedback_of_teams::team_id)),
        )
        .inner_join(
            tournament_debates::table
                .on(tournament_debates::id.eq(feedback_of_teams::debate_id)),
        )
        .inner_join(
            tournament_rounds::table
                .on(tournament_rounds::id.eq(tournament_debates::round_id)),
        )
        .filter(feedback_of_teams::tournament_id.eq(tournament_id))
        .select((
            feedback_of_teams::id,
            tournament_rounds::name,
            tournament_teams::name,
            feedback_of_teams::target_judge_id,
        ))
        .limit(per_page)
        .offset(offset)
        .load::<(String, String, String, String)>(&mut *conn)
        .unwrap();

    let mut feedback_teams_display = Vec::new();
    for (id, round_name, source_name, target_judge_id) in feedback_teams {
        let target_name = tournament_judges::table
            .find(&target_judge_id)
            .select(tournament_judges::name)
            .first::<String>(&mut *conn)
            .unwrap_or_else(|_| "Unknown".to_string());
        feedback_teams_display.push(FeedbackDisplayItem {
            id,
            round_name,
            source_name,
            target_name,
        });
    }

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(FeedbackTableRenderer {
                tournament,
                rounds,
                feedback_judges: feedback_judges_display,
                feedback_teams: feedback_teams_display,
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
}

struct FeedbackTableRenderer {
    tournament: Tournament,
    rounds: TournamentRounds,
    feedback_judges: Vec<FeedbackDisplayItem>,
    feedback_teams: Vec<FeedbackDisplayItem>,
    page: i64,
}

impl Renderable for FeedbackTableRenderer {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            SidebarWrapper tournament=(&self.tournament) rounds=(&self.rounds) {
                div class="d-flex justify-content-between flex-wrap flex-md-nowrap align-items-center pt-3 pb-2 mb-3 border-bottom" {
                    h1 class="h2" { "Feedback Submissions" }
                }

            h3 { "From Judges" }
            div class="table-responsive mb-4" {
                table class="table table-striped table-sm" {
                    thead {
                        tr {
                            th scope="col" { "Round" }
                            th scope="col" { "From (Judge)" }
                            th scope="col" { "To (Judge)" }
                            th scope="col" { "ID" }
                        }
                    }
                    tbody {
                        @for item in &self.feedback_judges {
                            tr {
                                td { (item.round_name) }
                                td { (item.source_name) }
                                td { (item.target_name) }
                                td { code { (item.id) } }
                            }
                        }
                        @if self.feedback_judges.is_empty() {
                            tr {
                                td colspan="4" class="text-center" { "No feedback from judges found." }
                            }
                        }
                    }
                }
            }

            h3 { "From Teams" }
            div class="table-responsive" {
                table class="table table-striped table-sm" {
                    thead {
                        tr {
                            th scope="col" { "Round" }
                            th scope="col" { "From (Team)" }
                            th scope="col" { "To (Judge)" }
                            th scope="col" { "ID" }
                        }
                    }
                    tbody {
                        @for item in &self.feedback_teams {
                            tr {
                                td { (item.round_name) }
                                td { (item.source_name) }
                                td { (item.target_name) }
                                td { code { (item.id) } }
                            }
                        }
                        @if self.feedback_teams.is_empty() {
                            tr {
                                td colspan="4" class="text-center" { "No feedback from teams found." }
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
