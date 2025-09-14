use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::get;

use crate::{
    auth::User,
    schema::tournament_rounds,
    state::Conn,
    template::Page,
    tournaments::{Tournament, rounds::Round},
    util_resp::{StandardResponse, err_not_found, success},
};

#[get("/tournaments/<tid>/rounds/<rid>")]
pub async fn view_tournament_round_page(
    tid: &str,
    rid: &str,
    mut conn: Conn<true>,
    user: User<true>,
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

    success(
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
