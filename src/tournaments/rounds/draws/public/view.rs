use std::collections::HashMap;

use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::get;

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

#[get("/tournaments/<tournament_id>/draw")]
pub async fn view_active_draw_page(
    tournament_id: &str,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;

    let rounds = Round::current_rounds(tournament_id, &mut *conn);

    let draws = rounds
        .into_iter()
        .filter(|r| r.draw_status == "R")
        .map(|round| RoundDrawRepr::of_round(round, &mut *conn))
        .collect::<Vec<_>>();

    let teams = tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(&tournament_id))
        .load::<Team>(&mut *conn)
        .unwrap()
        .into_iter()
        .map(|t| (t.id.clone(), t))
        .collect::<HashMap<_, _>>();

    // todo: send websocket msg and subscribe when new draw released (page can
    // then subscribe and trigger a reload)
    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .body(maud! {
                h1 {
                    "Current draw"
                }

                @for draw in &draws {
                    @if draw.round.draw_released_at.is_some() {
                        h3 {
                            (draw.round.name)
                        }
                        table {
                            thead {
                                th scope="col" {
                                    "#"
                                }
                                tr {
                                    th scope="col" {
                                        "#"
                                    }
                                    @for i in 0..tournament.teams_per_side {
                                        th scope="col" {
                                            "Prop " (i)
                                        }
                                        th scope="col" {
                                            "Opp " (i)
                                        }
                                    }
                                    // todo: adjudicators
                                    th scope="col" {
                                        "Manage"
                                    }
                                }
                                th scope="col" {
                                    "Panel"
                                }
                            }
                            tbody {
                                @for (i, debate) in draw.debates.iter().enumerate() {
                                    tr {
                                        th scope="row" {
                                            (i)
                                        }
                                        @for team in &debate.teams_of_debate {
                                            td {
                                                a href = (format!("/tournaments/{tournament_id}/teams/{}", &team.id)) {
                                                    (teams.get(&team.id).unwrap().name)
                                                }
                                            }
                                        }
                                        td {
                                            "TODO"
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
