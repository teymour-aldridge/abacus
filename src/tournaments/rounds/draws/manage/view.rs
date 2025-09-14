use std::collections::HashMap;

use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::get;

use crate::{
    auth::User,
    permission::IsTabDirector,
    schema::{tournament_draws, tournament_rounds, tournament_teams},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        rounds::{
            Round,
            draws::{DebateRepr, Draw, DrawRepr, manage::DrawTableRenderer},
        },
        teams::Team,
    },
};

#[get("/tournaments/<_tournament_id>/rounds/<round_id>/draw/<draw_id>")]
pub async fn view_draw(
    _tournament_id: &str,
    round_id: &str,
    draw_id: &str,
    user: User<true>,
    _dir: IsTabDirector<true>,
    mut conn: Conn<true>,
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

    fn make_actions(_: &DebateRepr) -> impl Renderable {
        maud! {
            "TODO"
        }
    }

    let renderer = DrawTableRenderer {
        tournament: &tournament,
        repr: &repr,
        actions: make_actions,
        teams: &teams,
    };

    Some(
        Page::new()
            .tournament(tournament.clone())
            .user(user)
            .body(maud! {
                h1 {
                    "Draw for round " (round.name)
                }
                (renderer)
            })
            .render(),
    )
}
