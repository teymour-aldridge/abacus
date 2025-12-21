use hypertext::prelude::*;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{Tournament, rounds::Round},
    util_resp::{StandardResponse, success},
};

pub async fn public_tournament_page(
    tournament_id: &str,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    let all_rounds = crate::tournaments::rounds::TournamentRounds::fetch(
        tournament_id,
        &mut *conn,
    )
    .unwrap();
    let current_rounds = Round::current_rounds(tournament_id, &mut *conn);

    let grouped_rounds = all_rounds.all_grouped_by_seq();

    success(
        Page::new()
            .tournament(tournament.clone())
            .user_opt(user)
            .current_rounds(current_rounds)
            .body(maud! {
                div class="container py-4" {
                    div class="d-flex align-items-center justify-content-between mb-4" {
                        div {
                            h1 class="h2 mb-1 fw-bold" {
                                (tournament.name)
                            }
                            p class="text-secondary mb-0" {
                                "Public Portal"
                            }
                        }
                    }

                    div class="card shadow-sm" {
                        div class="card-header bg-transparent py-3" {
                            h2 class="h5 mb-0 fw-bold" { "Overview" }
                        }
                        div class="list-group list-group-flush" {
                            @for level in grouped_rounds.iter().rev() {
                                @let is_draw_pub = level.iter().any(|r| r.is_draw_public());
                                @let is_results_pub = level.iter().all(|r| r.is_results_public());

                                @if is_results_pub {
                                    a href=(format!("/tournaments/{}/rounds/{}/results", tournament.id, level[0].seq))
                                      class="list-group-item list-group-item-action py-3 d-flex align-items-center" {
                                        span class="material-icons text-muted me-3" { "assessment" }
                                        div class="flex-grow-1" {
                                            div class="fw-semibold text-dark" { "Results" }
                                            div class="small text-muted" {
                                                @if level.len() == 1 {
                                                    (level[0].name)
                                                } @else {
                                                    @for (i, round) in level.iter().enumerate() {
                                                        @if i > 0 { ", " }
                                                        (round.name)
                                                    }
                                                }
                                            }
                                        }
                                        span class="material-icons text-muted fs-6" { "chevron_right" }
                                    }
                                } @else if is_draw_pub && tournament.show_draws {
                                    a href=(format!("/tournaments/{}/rounds/{}/draw", tournament.id, level[0].seq))
                                      class="list-group-item list-group-item-action py-3 d-flex align-items-center" {
                                        span class="material-icons text-muted me-3" { "grid_view" }
                                        div class="flex-grow-1" {
                                            div class="fw-semibold text-dark" { "Current Draw" }
                                            div class="small text-muted" {
                                                @if level.len() == 1 {
                                                    (level[0].name)
                                                } @else {
                                                    @for (i, round) in level.iter().enumerate() {
                                                        @if i > 0 { ", " }
                                                        (round.name)
                                                    }
                                                }
                                            }
                                        }
                                        span class="material-icons text-muted fs-6" { "chevron_right" }
                                    }
                                }
                            }

                            a href=(format!("/tournaments/{}/participants", tournament.id))
                              class="list-group-item list-group-item-action py-3 d-flex align-items-center" {
                                span class="material-icons text-muted me-3" { "groups" }
                                div class="flex-grow-1" {
                                    div class="fw-semibold text-dark" { "Participants" }
                                    div class="small text-muted" { "View all teams and adjudicators" }
                                }
                                span class="material-icons text-muted fs-6" { "chevron_right" }
                            }

                            @if tournament.standings_public || tournament.team_tab_public {
                                a href=(format!("/tournaments/{}/tab/team", tournament.id))
                                  class="list-group-item list-group-item-action py-3 d-flex align-items-center" {
                                    span class="material-icons text-muted me-3" { "leaderboard" }
                                    div class="flex-grow-1" {
                                        div class="fw-semibold text-dark" { "Standings" }
                                        div class="small text-muted" { "Team and speaker tab" }
                                    }
                                    span class="material-icons text-muted fs-6" { "chevron_right" }
                                }
                            }

                            a href=(format!("/tournaments/{}/motions", tournament.id))
                              class="list-group-item list-group-item-action py-3 d-flex align-items-center" {
                                span class="material-icons text-muted me-3" { "article" }
                                div class="flex-grow-1" {
                                    div class="fw-semibold text-dark" { "Motions" }
                                    div class="small text-muted" { "Motions for each round" }
                                }
                                span class="material-icons text-muted fs-6" { "chevron_right" }
                            }
                        }
                    }
                }
            })
            .render(),
    )
}
