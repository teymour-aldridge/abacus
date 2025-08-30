use std::collections::HashMap;

use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::get;

use crate::{
    auth::User,
    permission::IsTabDirector,
    schema::{tournament_draws, tournament_rounds, tournament_teams},
    state::LockedConn,
    template::Page,
    tournaments::{
        Tournament,
        rounds::{
            Round,
            draws::{Draw, DrawRepr},
        },
        teams::Team,
    },
};

#[get("/tournaments/<tournament_id>/rounds/<round_id>/draw/<draw_id>")]
pub async fn view_draw(
    tournament_id: &str,
    round_id: &str,
    draw_id: &str,
    user: User,
    _dir: IsTabDirector,
    mut conn: LockedConn<'_>,
    tournament: Tournament,
) -> Option<Rendered<String>> {
    let (draw, round) = match tournament_draws::table
        .filter(
            tournament_draws::round_id
                .eq(round_id)
                .and(tournament_draws::tournament_id.eq(draw_id)),
        )
        .inner_join(tournament_rounds::table)
        .first::<(Draw, Round)>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(draw) => draw,
        None => return None,
    };

    // todo: run on background thread (?)
    let repr = DrawRepr::of_draw(draw, &mut *conn);

    let teams = tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(&tournament.id))
        .load::<Team>(&mut *conn)
        .unwrap()
        .into_iter()
        .map(|c| (c.id.clone(), c))
        .collect::<HashMap<_, _>>();

    Some(
        Page::new()
            .tournament(tournament.clone())
            .user(user)
            .body(maud! {
                h1 {
                    "Draw for round " (round.name)
                }
                table class = "table" {
                    thead {
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
                    }
                    tbody {
                        @for (i, debate) in repr.debates.iter().enumerate() {
                            th scope="row" {
                                (i)
                            }
                            @for team in &debate.teams {
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
            })
            .render(),
    )
}
