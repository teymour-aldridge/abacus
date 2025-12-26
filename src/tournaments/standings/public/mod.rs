use axum::extract::Path;
use hypertext::{Renderable, maud, prelude::*};
use itertools::Itertools;
use rust_decimal::prelude::ToPrimitive;

use crate::{
    auth::User,
    state::Conn,
    template::{ActiveNav, Page},
    tournaments::{
        Tournament, config::RankableTeamMetric,
        standings::compute::TeamStandings,
    },
    util_resp::{StandardResponse, success, unauthorized},
};

pub async fn public_team_tab_page(
    Path(tournament_id): Path<String>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;

    let is_superuser = if let Some(ref user) = user {
        tournament
            .check_user_is_superuser(&user.id, &mut *conn)
            .is_ok()
    } else {
        false
    };

    let show_full_tab = is_superuser || tournament.team_tab_public;
    let show_standings_only = !show_full_tab && tournament.standings_public;

    if !show_full_tab && !show_standings_only {
        return unauthorized();
    }

    let current_rounds = crate::tournaments::rounds::Round::current_rounds(
        &tournament_id,
        &mut *conn,
    );
    let standings = TeamStandings::recompute(&tournament_id, &mut *conn);

    if show_full_tab {
        success(Page::new()
            .active_nav(ActiveNav::Standings)
            .tournament(tournament)
            .user_opt(user)
            .current_rounds(current_rounds)
            .body(maud! {
                div class="container py-5 px-4" {
                    table class = "table" {
                        thead {
                            tr {
                                th scope = "col" { "#" }
                                th scope = "col" {
                                    "Team name"
                                }
                                @for metric in &standings.metrics {
                                    th scope = "col" {
                                        (metric.to_string())
                                    }
                                }
                            }
                        }
                        tbody {
                            @for (i, teams) in standings.teams_in_rank_order.iter().enumerate() {
                                @for team in teams {
                                    tr {
                                        th scope="row" {
                                            @if teams.len() > 1 {
                                                "="
                                            }
                                            (i + 1)
                                        }
                                        td {
                                            (team.name)
                                        }
                                        @for metric in standings.ranked_metrics_of_team.get(&team.id).unwrap() {
                                            td {
                                                (metric.1.to_string())
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .render())
    } else {
        let teams_by_points: Vec<(i64, Vec<&crate::tournaments::teams::Team>)> =
            standings
                .teams_in_rank_order
                .iter()
                .flatten()
                .filter_map(|team| {
                    standings.ranked_metrics_of_team.get(&team.id).and_then(
                        |metrics| {
                            metrics.iter().find_map(|(metric, value)| {
                                if matches!(metric, RankableTeamMetric::Wins) {
                                    value.to_i64().map(|pts| (pts, team))
                                } else {
                                    None
                                }
                            })
                        },
                    )
                })
                .sorted_by(|a, b| b.0.cmp(&a.0))
                .chunk_by(|t| t.0)
                .into_iter()
                .map(|(points, group)| {
                    let mut teams: Vec<_> =
                        group.map(|(_, team)| team).collect();
                    // todo: correctly add institutional prefix
                    teams.sort_by(|a, b| a.name.cmp(&b.name));
                    (points, teams)
                })
                .collect();

        let mut ranked_teams: Vec<(
            i64,
            i64,
            &crate::tournaments::teams::Team,
        )> = Vec::new();
        let mut current_rank: i64 = 1;
        for (points, teams) in &teams_by_points {
            for team in teams {
                ranked_teams.push((current_rank, *points, *team));
            }
            current_rank += teams.len() as i64;
        }

        success(
            Page::new()
                .active_nav(ActiveNav::Standings)
                .tournament(tournament)
                .user_opt(user)
                .current_rounds(current_rounds)
                .body(maud! {
                    div class="container py-5 px-4" {
                        table class = "table" {
                            thead {
                                tr {
                                    th scope = "col" { "#" }
                                    th scope = "col" { "Team name" }
                                    th scope = "col" { "Points" }
                                }
                            }
                            tbody {
                                @for (rank, points, team) in &ranked_teams {
                                    @let is_tied = teams_by_points.iter()
                                        .find(|(p, _)| p == points)
                                        .map(|(_, teams)| teams.len() > 1)
                                        .unwrap_or(false);
                                    tr {
                                        th scope="row" {
                                            @if is_tied {
                                                "="
                                            }
                                            (rank)
                                        }
                                        td {
                                            (team.name)
                                        }
                                        td {
                                            (points)
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
}
