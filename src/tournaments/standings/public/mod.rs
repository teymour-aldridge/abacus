use hypertext::prelude::*;
use rocket::get;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{Tournament, standings::compute::TeamStandings},
    util_resp::{StandardResponse, success, unauthorized},
};

#[get("/tournaments/<tournament_id>/tab/team")]
pub async fn public_team_tab_page(
    tournament_id: &str,
    mut conn: Conn<true>,
    user: Option<User<true>>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;

    if !tournament.team_tab_public {
        return unauthorized();
    }

    let standings = TeamStandings::recompute(tournament_id, &mut *conn);

    success(Page::new()
        .tournament(tournament)
        .user_opt(user)
        .body(maud! {
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
                    @for (i, teams) in standings.ranked.iter().enumerate() {
                        @for team in teams {
                            tr {
                                th scope="row" {
                                    @if teams.len() > 1 {
                                        "="
                                    }
                                    (i)
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
        })
        .render())
}
