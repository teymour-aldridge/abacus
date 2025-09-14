use hypertext::prelude::*;
use rocket::get;

use crate::{auth::User, template::Page, tournaments::Tournament};

#[get("/tournaments/<_tournament_id>")]
pub async fn public_tournament_page(
    _tournament_id: &str,
    tournament: Tournament,
    user: Option<User<true>>,
) -> Rendered<String> {
    Page::new()
        .tournament(tournament.clone())
        .user_opt(user)
        .body(maud! {
            h1 {
                "Tournament " (tournament.name)
                // todo: add links based on available actions
            }
        })
        .render()
}
