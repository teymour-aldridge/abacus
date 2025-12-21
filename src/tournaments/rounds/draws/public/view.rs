use std::collections::HashMap;

use axum::extract::Path;
use diesel::prelude::*;
use hypertext::prelude::*;

use crate::{
    auth::User,
    schema::tournament_teams,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        rounds::{Round, draws::RoundDrawRepr},
        teams::Team,
    },
    util_resp::{StandardResponse, success},
};

pub async fn view_active_draw_page(
    Path((tournament_id, round_seq)): Path<(String, i64)>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;

    let rounds = Round::of_seq(round_seq, &tournament_id, &mut *conn);
    if rounds.is_empty() {
        return crate::util_resp::err_not_found();
    }

    if rounds.iter().all(|r| r.results_published_at.is_some()) {
        return crate::util_resp::see_other_ok(axum::response::Redirect::to(
            &format!("/tournaments/{tournament_id}/rounds/{round_seq}/results"),
        ));
    }

    let draws = rounds
        .iter()
        .filter(|r| {
            r.draw_status == "released_full"
                || r.draw_status == "released_teams"
        })
        .map(|round| RoundDrawRepr::of_round(round.clone(), &mut *conn))
        .collect::<Vec<_>>();

    let teams = tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(&tournament_id))
        .load::<Team>(&mut *conn)
        .unwrap()
        .into_iter()
        .map(|t| (t.id.clone(), t))
        .collect::<HashMap<_, _>>();

    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .body(maud! {
                div class="container py-4" {
                    div class="mb-4" {
                        h1 class="h2 mb-1 fw-bold" {
                            "Draw: "
                            @for (i, round) in rounds.iter().enumerate() {
                                @if i > 0 { ", " }
                                (round.name)
                            }
                        }
                    }

                    @if draws.is_empty() {
                        div class="alert alert-info border-0 bg-light text-dark d-flex align-items-center" role="alert" {
                            span class="material-icons me-2" { "info" }
                            div { "The draw for these rounds is not yet public." }
                        }
                    } @else {
                        @if draws.len() < rounds.len() {
                            @let released_ids: std::collections::HashSet<_> = draws.iter().map(|d| &d.round.id).collect();
                            @for round in &rounds {
                                @if !released_ids.contains(&round.id) {
                                    div class="alert alert-info border-0 bg-light text-dark d-flex align-items-center mb-3" role="alert" {
                                        span class="material-icons me-2" { "info" }
                                        div { "The draw for " (round.name) " is not yet available." }
                                    }
                                }
                            }
                        }

                        @for draw in &draws {
                            @if draw.round.draw_released_at.is_some() {
                                div class="card shadow-sm mb-4" {
                                    div class="card-header bg-transparent py-3" {
                                        h3 class="h5 mb-0 fw-bold" {
                                            (draw.round.name)
                                        }
                                    }
                                    div class="table-responsive" {
                                        table class="table table-hover mb-0 align-middle" {
                                            thead class="table-light" {
                                                tr {
                                                    th scope="col" class="ps-3" { "#" }
                                                    @for i in 0..tournament.teams_per_side {
                                                        th scope="col" {
                                                            "Prop " (i + 1)
                                                        }
                                                        th scope="col" {
                                                            "Opp " (i + 1)
                                                        }
                                                    }
                                                    @if draw.round.draw_status == "released_full" {
                                                        th scope="col" class="pe-3" { "Panel" }
                                                    }
                                                }
                                            }
                                            tbody {
                                                @for (i, debate) in draw.debates.iter().enumerate() {
                                                    tr {
                                                        th scope="row" class="ps-3 text-secondary fw-normal" { (i + 1) }
                                                        @for debate_team in &debate.teams_of_debate {
                                                            td {
                                                                a href = (format!("/tournaments/{tournament_id}/teams/{}", &debate_team.team_id)) 
                                                                  class="text-decoration-none fw-semibold text-dark" {
                                                                    (teams.get(&debate_team.team_id).map(|t| t.name.as_str()).unwrap_or("Unknown Team"))
                                                                }
                                                            }
                                                        }
                                                        @if draw.round.draw_status == "released_full" {
                                                            td class="pe-3" {
                                                                div class="d-flex flex-wrap gap-1" {
                                                                    @for judge in &debate.judges_of_debate {
                                                                        span class="badge bg-light text-dark border fw-normal" {
                                                                            (debate.judges.get(&judge.judge_id).map(|j| j.name.as_str()).unwrap_or("Unknown Judge"))
                                                                            @if judge.status == "C" {
                                                                                " (C)"
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
