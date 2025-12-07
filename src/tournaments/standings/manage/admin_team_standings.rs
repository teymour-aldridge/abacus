use axum::extract::Path;
use hypertext::{Renderable, maud, prelude::*};

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament, manage::sidebar::SidebarWrapper, rounds::TournamentRounds,
        standings::compute::TeamStandings,
    },
    util_resp::{StandardResponse, success},
};

pub async fn admin_view_team_standings(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    let standings = TeamStandings::recompute(&tournament_id, &mut *conn);

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) {
                    table class="table" {
                        thead {
                            tr {
                                th scope="col" {
                                    "Rank"
                                }
                                td {
                                    "Team name"
                                }
                                @for metric in &standings.metrics {
                                    td {
                                        (metric.to_string())
                                    }
                                }
                            }
                        }
                        tbody {
                            @for (i, rank) in standings.teams_in_rank_order.iter().enumerate() {
                                @for team in rank {
                                    tr {
                                        th scope="col" {
                                            @if rank.len() > 1 {
                                                 "="
                                            }
                                            (i + 1)
                                        }
                                        td {
                                            (team.name)
                                        }
                                        @for (_, metric_value) in &standings.ranked_metrics_of_team[&team.id] {
                                            td {
                                                (metric_value.to_string())
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
