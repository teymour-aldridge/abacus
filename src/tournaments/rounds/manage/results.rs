use axum::{Form, extract::Path, response::Redirect};
use chrono::Utc;
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;

use crate::{
    auth::User,
    schema::tournament_rounds,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{Round, TournamentRounds},
    },
    util_resp::{
        StandardResponse, bad_request, err_not_found, see_other_ok, success,
    },
};

pub async fn manage_results_page(
    Path((tid, round_seq)): Path<(String, i64)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let all_rounds = TournamentRounds::fetch(&tid, &mut *conn).unwrap();
    let rounds_in_seq = Round::of_seq(round_seq, &tid, &mut *conn);

    if rounds_in_seq.is_empty() {
        return err_not_found();
    }

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&all_rounds) {
                    div class="container" {
                        h1 {
                            "Results"
                        }

                        p class="lead" {
                            "For rounds "
                            @for (i, round) in rounds_in_seq.iter().enumerate() {
                                @if i > 0 {
                                    ", "
                                }
                                (round.name)
                            }
                        }

                        div class="row" {
                            @for round in &rounds_in_seq {
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
                                                    form action=(format!("/tournaments/{}/rounds/{}/complete", tid, round.id)) method="post" class="mb-2" {
                                                        input type="hidden" name="completed" value="false";
                                                        button class="btn btn-outline-secondary w-100" type="submit" {
                                                            "Mark Incomplete"
                                                        }
                                                    }
                                                } @else {
                                                    form action=(format!("/tournaments/{}/rounds/{}/complete", tid, round.id)) method="post" class="mb-2" {
                                                        input type="hidden" name="completed" value="true";
                                                        button class="btn btn-primary w-100" type="submit" {
                                                            "Mark Complete"
                                                        }
                                                    }
                                                }

                                                @if round.completed {
                                                    @if round.results_published_at.is_some() {
                                                        form action=(format!("/tournaments/{}/rounds/{}/results/publish", tid, round.id)) method="post" {
                                                            input type="hidden" name="published" value="false";
                                                            button class="btn btn-danger w-100" type="submit" {
                                                                "Unpublish Results"
                                                            }
                                                        }
                                                    } @else {
                                                        form action=(format!("/tournaments/{}/rounds/{}/results/publish", tid, round.id)) method="post" {
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
                }
            })
            .render(),
    )
}

#[derive(Deserialize, Debug)]
pub struct SetRoundCompleted {
    completed: bool,
}

pub async fn set_round_completed(
    Path((tournament_id, round_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<SetRoundCompleted>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = Round::fetch(&round_id, &mut *conn)?;

    // If marking as incomplete, also unpublish results (maintain invariant)
    if !form.completed {
        diesel::update(tournament_rounds::table.find(&round.id))
            .set((
                tournament_rounds::completed.eq(false),
                tournament_rounds::results_published_at
                    .eq(None::<chrono::NaiveDateTime>),
            ))
            .execute(&mut *conn)
            .unwrap();
    } else {
        diesel::update(tournament_rounds::table.find(&round.id))
            .set(tournament_rounds::completed.eq(true))
            .execute(&mut *conn)
            .unwrap();
    }

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/rounds/{}/results/manage",
        tournament_id, round.seq
    )))
}

#[derive(Deserialize, Debug)]
pub struct SetResultsPublished {
    published: bool,
}

pub async fn set_results_published(
    Path((tournament_id, round_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<SetResultsPublished>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = Round::fetch(&round_id, &mut *conn)?;

    // Enforce invariant: can only publish if round is complete
    if form.published && !round.completed {
        return bad_request(
            maud! {
                div class="alert alert-danger" {
                    "Cannot publish results for an incomplete round."
                }
            }
            .render(),
        );
    }

    diesel::update(tournament_rounds::table.find(&round.id))
        .set(
            tournament_rounds::results_published_at.eq(if form.published {
                Some(Utc::now().naive_utc())
            } else {
                None
            }),
        )
        .execute(&mut *conn)
        .unwrap();

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/rounds/{}/results/manage",
        tournament_id, round.seq
    )))
}
