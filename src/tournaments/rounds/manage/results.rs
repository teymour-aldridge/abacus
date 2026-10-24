use axum::{
    Form,
    extract::{Path, Query},
};
use chrono::Utc;
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;

use crate::{
    auth::User,
    schema::{
        agg_speaker_results_of_debate, agg_team_results_of_debate, debates,
        motions_of_round, rounds,
    },
    state::Conn,
    tournaments::{
        Tournament,
        rounds::{
            Round,
            ballots::{BallotRepr, aggregate::aggregate_ballot_set},
            draws::{DebateRepr, RoundDrawRepr},
        },
        standings::compute::refresh_saved_team_standings,
    },
    ui::response::{ModalQuery, after_edit, form_page},
    util_resp::{StandardResponse, err_not_found},
};

struct ResultsView {
    tournament: Tournament,
    rounds_in_seq: Vec<Round>,
    error: Option<String>,
}

impl Renderable for ResultsView {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            div {
                @if let Some(error)=&self.error { div class="alert alert-danger" role="alert" { (error) } }
                h1 {
                    "Results"
                }

                p class="lead" {
                    "For rounds "
                    @for (i, round) in self.rounds_in_seq.iter().enumerate() {
                        @if i > 0 {
                            ", "
                        }
                        (round.name)
                    }
                }

                div class="row" {
                    @for round in &self.rounds_in_seq {
                        div class="col-md-6 col-lg-4 mb-4 p-2" {
                            div class="card h-100" {
                                div class="card-body d-flex flex-column" {
                                    h5 class="card-title" { (round.name) }

                                    div class="mb-3" {
                                        h6 class="text-uppercase small fw-bold text-muted" { "Completion Status" }
                                        @if round.completed {
                                            span class="badge bg-success" { "Complete" }
                                        } @else {
                                            span class="badge bg-secondary" { "In Progress" }
                                        }
                                    }

                                    div class="mb-3" {
                                        h6 class="text-uppercase small fw-bold text-muted" { "Results Publication" }
                                        @if round.results_published_at.is_some() {
                                            span class="badge bg-success" { "Published" }
                                        } @else {
                                            span class="badge bg-secondary" { "Not Published" }
                                        }
                                    }

                                    div class="mt-auto" {
                                        @if round.completed {
                                            form action=(format!("/tournaments/{}/rounds/{}/complete", self.tournament.id, round.id)) method="post" hx-post=(format!("/tournaments/{}/rounds/{}/complete?modal=true", self.tournament.id, round.id)) hx-target="#ui-modal" class="mb-2" {
                                                input type="hidden" name="completed" value="false";
                                                button class="btn btn-outline-secondary w-100" type="submit" {
                                                    "Mark Incomplete"
                                                }
                                            }
                                        } @else {
                                            form action=(format!("/tournaments/{}/rounds/{}/complete", self.tournament.id, round.id)) method="post" hx-post=(format!("/tournaments/{}/rounds/{}/complete?modal=true", self.tournament.id, round.id)) hx-target="#ui-modal" class="mb-2" {
                                                input type="hidden" name="completed" value="true";
                                                button class="btn btn-primary w-100" type="submit" {
                                                    "Mark Complete"
                                                }
                                            }
                                        }

                                        @if round.completed {
                                            @if round.results_published_at.is_some() {
                                                form action=(format!("/tournaments/{}/rounds/{}/results/publish", self.tournament.id, round.id)) method="post" hx-post=(format!("/tournaments/{}/rounds/{}/results/publish?modal=true", self.tournament.id, round.id)) hx-target="#ui-modal" {
                                                    input type="hidden" name="published" value="false";
                                                    button class="btn btn-danger w-100" type="submit" {
                                                        "Unpublish Results"
                                                    }
                                                }
                                            } @else {
                                                form action=(format!("/tournaments/{}/rounds/{}/results/publish", self.tournament.id, round.id)) method="post" hx-post=(format!("/tournaments/{}/rounds/{}/results/publish?modal=true", self.tournament.id, round.id)) hx-target="#ui-modal" {
                                                    input type="hidden" name="published" value="true";
                                                    button class="btn btn-success w-100" type="submit" {
                                                        "Publish Results"
                                                    }
                                                }
                                            }
                                        } @else {
                                            button class="btn btn-secondary w-100" type="button" disabled {
                                                "Publish Results"
                                            }
                                            small class="text-muted d-block mt-1" {
                                                "Round must be marked complete before publishing results."
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }.render_to(buffer)
    }
}

pub async fn manage_results_page(
    Path((tid, round_seq)): Path<(String, i64)>,
    Query(query): Query<ModalQuery>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    result_controls(&tid, round_seq, query, user, &mut conn, None)
}
fn result_controls(
    tid: &str,
    seq: i64,
    query: ModalQuery,
    user: User<true>,
    conn: &mut diesel::SqliteConnection,
    error: Option<&str>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, conn)?;
    tournament.check_user_is_superuser(&user.id, conn)?;
    let rounds_in_seq = rounds::table
        .filter(rounds::tournament_id.eq(tid))
        .filter(rounds::seq.eq(seq))
        .load::<Round>(conn)?;
    if rounds_in_seq.is_empty() {
        return err_not_found();
    }
    let view = ResultsView {
        tournament: tournament.clone(),
        rounds_in_seq,
        error: error.map(str::to_owned),
    };
    form_page(
        query,
        "Result controls",
        &format!("/tournaments/{tid}/rounds/{seq}/results"),
        &tournament,
        user,
        view,
        error.is_some(),
    )
}

#[derive(Deserialize, Debug)]
pub struct SetRoundCompleted {
    completed: bool,
}

pub async fn set_round_completed(
    Query(query): Query<ModalQuery>,
    Path((tournament_id, round_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<SetRoundCompleted>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = Round::fetch(&tournament_id, &round_id, &mut *conn)?;

    // If marking as incomplete, also unpublish results (maintain invariant)
    if !form.completed {
        diesel::update(rounds::table.find(&round.id))
            .set((
                rounds::completed.eq(false),
                rounds::results_published_at.eq(None::<chrono::NaiveDateTime>),
            ))
            .execute(&mut *conn)?;
    } else {
        if round.draw_status != "released_full" {
            return result_controls(
                &tournament_id,
                round.seq,
                query,
                user,
                &mut conn,
                Some(
                    "Cannot mark round as complete before publishing the full draw.",
                ),
            );
        }
        if !round_has_motion(&round.id, &mut *conn)? {
            return result_controls(
                &tournament_id,
                round.seq,
                query,
                user,
                &mut conn,
                Some("Add a motion before completing this round."),
            );
        }

        // Enforce invariant: can only complete if all non-trainee judges have submitted ballots
        // and there are no conflicts.
        let draw = RoundDrawRepr::of_round(round.clone(), &mut *conn);
        for debate in draw.debates {
            let ballots = debate.latest_ballots(&mut *conn);
            let non_trainee_judges: Vec<_> = debate
                .judges_of_debate
                .iter()
                .filter(|j| j.status != "T")
                .collect();

            let all_non_trainees_submitted =
                non_trainee_judges.iter().all(|judge| {
                    ballots
                        .iter()
                        .any(|b| b.metadata.judge_id == judge.judge_id)
                });

            if !all_non_trainees_submitted {
                return result_controls(
                    &tournament_id,
                    round.seq,
                    query,
                    user,
                    &mut conn,
                    Some("Cannot mark round as complete: missing ballots."),
                );
            }

            let problems =
                BallotRepr::problems_of_set(&ballots, &tournament, &debate);
            if !problems.is_empty() {
                return result_controls(
                    &tournament_id,
                    round.seq,
                    query,
                    user,
                    &mut conn,
                    Some("Cannot mark round as complete: conflicting ballots."),
                );
            }
        }

        diesel::update(rounds::table.find(&round.id))
            .set(rounds::completed.eq(true))
            .execute(&mut *conn)?;

        // Aggregate ballot results for all debates in this round.
        // First, clear any existing aggregated data for these debates.
        let debate_ids: Vec<String> = debates::table
            .filter(debates::round_id.eq(&round.id))
            .select(debates::id)
            .load(&mut *conn)?;

        diesel::delete(agg_speaker_results_of_debate::table.filter(
            agg_speaker_results_of_debate::debate_id.eq_any(&debate_ids),
        ))
        .execute(&mut *conn)?;

        diesel::delete(
            agg_team_results_of_debate::table.filter(
                agg_team_results_of_debate::debate_id.eq_any(&debate_ids),
            ),
        )
        .execute(&mut *conn)?;

        // Re-fetch and aggregate each debate's ballots.
        // (The validation loop above consumed `draw.debates`.)
        for debate_id in &debate_ids {
            let debate_repr = DebateRepr::fetch(debate_id, &mut *conn);
            let ballots = debate_repr.latest_ballots(&mut *conn);
            if ballots.is_empty() {
                continue;
            }
            aggregate_ballot_set(
                &ballots,
                &tournament,
                &debate_repr,
                &mut *conn,
            );
        }
    }

    refresh_saved_team_standings(&tournament_id, &mut *conn)?;

    after_edit(
        query,
        format!("/tournaments/{tournament_id}/rounds/{}/results", round.seq),
    )
}

fn round_has_motion(
    round_id: &str,
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> Result<bool, diesel::result::Error> {
    Ok(motions_of_round::table
        .filter(motions_of_round::round_id.eq(round_id))
        .count()
        .get_result::<i64>(conn)?
        > 0)
}

#[derive(Deserialize, Debug)]
pub struct SetResultsPublished {
    published: bool,
}

pub async fn set_results_published(
    Query(query): Query<ModalQuery>,
    Path((tournament_id, round_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<SetResultsPublished>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = Round::fetch(&tournament_id, &round_id, &mut *conn)?;

    // Enforce invariant: can only publish if round is complete
    if form.published && !round.completed {
        return result_controls(
            &tournament_id,
            round.seq,
            query,
            user,
            &mut conn,
            Some("Cannot publish results for an incomplete round."),
        );
    }

    diesel::update(rounds::table.find(&round.id))
        .set(rounds::results_published_at.eq(if form.published {
            Some(Utc::now().naive_utc())
        } else {
            None
        }))
        .execute(&mut *conn)?;

    after_edit(
        query,
        format!("/tournaments/{tournament_id}/rounds/{}/results", round.seq),
    )
}
