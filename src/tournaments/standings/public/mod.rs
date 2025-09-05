use hypertext::prelude::*;
use rocket::get;

use crate::{
    state::LockedConn,
    template::Page,
    tournaments::{Tournament, standings::compute::TournamentTeamStandings},
    widgets::alert::ErrorAlert,
};

#[get("/tournaments/<tournament_id>/tab/team")]
pub async fn public_team_tab_page(
    tournament_id: &str,
    tournament: Tournament,
    mut conn: LockedConn<'_>,
    user: Option<crate::auth::User>,
) -> Result<Rendered<String>, Rendered<String>> {
    if !tournament.team_tab_public {
        return Err(Page::new()
            .tournament(tournament)
            .user_opt(user)
            .body(ErrorAlert {
                msg: "The team tab is not public.",
            })
            .render());
    }

    let standings = TournamentTeamStandings::fetch(tournament_id, &mut *conn);

    Ok(Page::new()
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
                    @for (i, team) in standings.sorted.iter().enumerate() {
                        tr {
                            th scope="row" {
                                (i)
                            }
                            td {
                                (team.name)
                            }
                            @for metric in standings.metrics_of_team.get(&team.id).unwrap() {
                                th scope = "col" {
                                    (metric.to_string())
                                }
                            }
                        }
                    }
                }
            }
        })
        .render())
}
