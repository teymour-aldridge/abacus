use hypertext::prelude::*;
use rocket::get;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::Tournament,
    util_resp::{StandardResponse, success},
};

#[get("/tournaments/<tournament_id>")]
pub async fn public_tournament_page(
    tournament_id: &str,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;

    success(
        Page::new()
            .tournament(tournament.clone())
            .user_opt(user)
            .body(maud! {
                h1 {
                    "Tournament " (tournament.name)
                    // todo: add links based on available actions
                }
            })
            .render(),
    )
}
