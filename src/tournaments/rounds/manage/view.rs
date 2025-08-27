use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::get;

use crate::{
    auth::User,
    permission::IsTabDirector,
    schema::tournament_rounds,
    state::LockedConn,
    template::Page,
    tournaments::{Tournament, rounds::Round},
};

#[get("/tournaments/<tid>/rounds/<rid>")]
pub async fn view_tournament_round_page(
    tid: &str,
    rid: &str,
    tournament: Tournament,
    mut conn: LockedConn<'_>,
    user: User,
    _dir: IsTabDirector,
) -> Option<Rendered<String>> {
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
        None => return None,
    };

    Some(
        Page::new()
            // todo: can remove this clone
            .tournament(tournament.clone())
            .user(user)
            .body(maud! {
                h1 {
                    "Round " (round.name)
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

            })
            .render(),
    )
}
