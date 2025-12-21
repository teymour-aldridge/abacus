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
                div class="container py-5 px-4" {
                    header class="mb-5 pb-4 border-bottom border-4 border-dark" {
                        h1 class="display-3 fw-bold text-uppercase mb-1" {
                            (tournament.name)
                        }
                        p class="h4 text-uppercase fw-bold text-secondary mb-0" {
                            "Public Portal"
                        }
                    }

                    div class="row" {
                        div class="col-12" {
                            div class="list-group list-group-flush border-top border-dark" {
                                @for level in grouped_rounds.iter().rev() {
                                    @let is_draw_pub = level.iter().any(|r| r.is_draw_public());
                                    @let is_results_pub = level.iter().all(|r| r.is_results_public());

                                    @if is_results_pub {
                                        a href=(format!("/tournaments/{}/rounds/{}/results", tournament.id, level[0].seq))
                                          class="list-group-item list-group-item-action py-4 px-0 border-bottom border-dark d-flex align-items-center" {
                                            span class="material-icons me-4 fs-1" { "assessment" }
                                            div class="flex-grow-1" {
                                                div class="h2 mb-1 text-uppercase fw-bold" { "Results" }
                                                div class="small text-uppercase fw-bold text-secondary" {
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
                                            span class="material-icons fs-1" { "chevron_right" }
                                        }
                                    } @else if is_draw_pub && tournament.show_draws {
                                        a href=(format!("/tournaments/{}/rounds/{}/draw", tournament.id, level[0].seq))
                                          class="list-group-item list-group-item-action py-4 px-0 border-bottom border-dark d-flex align-items-center" {
                                            span class="material-icons me-4 fs-1" { "grid_view" }
                                            div class="flex-grow-1" {
                                                div class="h2 mb-1 text-uppercase fw-bold" { "Current Draw" }
                                                div class="small text-uppercase fw-bold text-secondary" {
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
                                            span class="material-icons fs-1" { "chevron_right" }
                                        }
                                    }
                                }

                                a href=(format!("/tournaments/{}/participants/public", tournament.id))
                                  class="list-group-item list-group-item-action py-4 px-0 border-bottom border-dark d-flex align-items-center" {
                                    span class="material-icons me-4 fs-1" { "groups" }
                                    div class="flex-grow-1" {
                                        div class="h2 mb-0 text-uppercase fw-bold" { "Participants" }
                                    }
                                    span class="material-icons fs-1" { "chevron_right" }
                                }

                                @if tournament.standings_public || tournament.team_tab_public {
                                    a href=(format!("/tournaments/{}/tab/team", tournament.id))
                                      class="list-group-item list-group-item-action py-4 px-0 border-bottom border-dark d-flex align-items-center" {
                                        span class="material-icons me-4 fs-1" { "leaderboard" }
                                        div class="flex-grow-1" {
                                            div class="h2 mb-0 text-uppercase fw-bold" { "Team Standings" }
                                        }
                                        span class="material-icons fs-1" { "chevron_right" }
                                    }
                                }

                                a href=(format!("/tournaments/{}/motions", tournament.id))
                                  class="list-group-item list-group-item-action py-4 px-0 border-bottom border-dark d-flex align-items-center" {
                                    span class="material-icons me-4 fs-1" { "article" }
                                    div class="flex-grow-1" {
                                        div class="h2 mb-0 text-uppercase fw-bold" { "Motions" }
                                    }
                                    span class="material-icons fs-1" { "chevron_right" }
                                }
                            }
                        }
                    }
                }
            })
            .render(),
    )
}
