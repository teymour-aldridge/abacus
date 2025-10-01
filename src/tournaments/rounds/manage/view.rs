use diesel::prelude::*;
use hypertext::{Renderable, maud, prelude::*};
use rocket::get;
use std::collections::HashMap;

use crate::{
    auth::User,
    schema::{tournament_rounds, tournament_teams},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        rounds::{
            Round,
            draws::manage::DrawTableRenderer,
            draws::{DebateRepr, RoundDrawRepr},
        },
        teams::Team,
    },
    util_resp::{StandardResponse, err_not_found, success},
};

#[get("/tournaments/<tid>/rounds/<rid>", rank = 2)]
pub async fn view_tournament_round_page(
    tid: &str,
    rid: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = match tournament_rounds::table
        .filter(
            tournament_rounds::tournament_id
                .eq(tid)
                .and(tournament_rounds::id.eq(rid)),
        )
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(t) => t,
        None => return err_not_found(),
    };

    let (repr, teams) = if round.draw_status != "D" {
        let repr = RoundDrawRepr::of_round(round.clone(), &mut *conn);
        let teams = tournament_teams::table
            .filter(tournament_teams::tournament_id.eq(&tournament.id))
            .load::<Team>(&mut *conn)
            .unwrap()
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect::<HashMap<_, _>>();
        (Some(repr), Some(teams))
    } else {
        (None, None)
    };

    success(
        Page::new()
            .tournament(tournament.clone())
            .user(user)
            .body(maud! {
                h1 {
                    (round.name)
                }

                ul class="list-group list-group-horizontal" {
                    li class="list-group-item" {
                        a href=(format!("/tournaments/{}/rounds/{}/edit",
                                tournament.id,
                                round.id))
                        {
                            "Edit round details"
                        }
                    }
                }
                @if round.draw_status != "D" {
                    @let renderer = DrawTableRenderer {
                        tournament: &tournament,
                        repr: &repr.as_ref().unwrap(),
                        actions: |_: &DebateRepr| maud! {
                            "TODO"
                        },
                        teams: &teams.as_ref().unwrap(),
                    };
                    (renderer)
                } @else {
                    a href=(format!("/tournaments/{}/rounds/{}/draws/create",
                            tournament.id,
                            round.id)) class="btn btn-primary" {
                        "Generate Draw"
                    }
                }

            })
            .render(),
    )
}
