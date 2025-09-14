use std::collections::HashMap;

use diesel::prelude::*;
use hypertext::prelude::*;
use itertools::Itertools;
use rocket::get;

use crate::{
    auth::User,
    permission::IsTabDirector,
    schema::tournament_break_categories,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        categories::BreakCategory,
        rounds::{Round, TournamentRounds},
    },
};

pub mod create;
pub mod edit;
pub mod view;

#[get("/tournaments/<tid>/rounds")]
pub async fn manage_rounds_page(
    tid: &str,
    user: User<true>,
    tournament: Tournament,
    _tab: IsTabDirector<true>,
    mut conn: Conn<true>,
) -> Rendered<String> {
    let rounds = TournamentRounds::fetch(tid, &mut *conn)
        .expect("failed to retrieve rounds");
    let categories2rounds = rounds.categories();
    let categories = tournament_break_categories::table
        .filter(tournament_break_categories::tournament_id.eq(&tid))
        .load::<BreakCategory>(&mut *conn)
        .unwrap()
        .into_iter()
        .map(|c| (c.id.clone(), c))
        .collect::<HashMap<_, _>>();

    let get_seq = |round: &Round| round.seq;
    // todo: case with no outrounds
    let min_outround_seq = rounds.elim.iter().map(get_seq).min().unwrap();
    let max_outround_seq = rounds.elim.iter().map(get_seq).max().unwrap();

    Page::new()
        .tournament(tournament)
        .user(user)
        .body(maud! {
            div class = "container" {
                @for prelim in &rounds.prelim {
                    div class = "m-2" {
                        (prelim.name)

                        a href=(format!("/tournaments/{tid}/rounds/{}", prelim.id))
                        {
                            " (edit)"
                        }
                    }
                }
            }
            div class = "container" {
                @for i in min_outround_seq..max_outround_seq {
                    div class = "row" {
                        @for (_, rounds) in categories2rounds.iter().sorted_by_key(
                            |(cat_id, _)| {
                                categories.get(*cat_id).unwrap().priority
                            }
                        ) {
                            div class = "col" {
                                @if let Some(round) =
                                    rounds.iter().find(|round| round.seq == i) {
                                    (round.name)
                                    a href=(format!("/tournaments/{tid}/rounds/{}", round.id))
                                    {
                                        " (edit)"
                                    }
                                } @else {
                                    "---"
                                }
                            }
                        }
                    }
                }
            }
        })
        .render()
}
