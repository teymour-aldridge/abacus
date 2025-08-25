use hypertext::prelude::*;
use rocket::{Responder, get};

use crate::{
    auth::User, permission::IsTabDirector, template::Page,
    tournaments::Tournament,
};

#[derive(Responder)]
pub enum ViewTournamentResponse {
    Page(Rendered<String>),
}

#[get("/tournaments/<_tid>", rank = 1)]
/// Returns the tournament view for an administrator. We use lower-ranking
/// routes to handle other cases.
pub async fn admin_view_tournament(
    _tid: &str,
    tournament: Tournament,
    _tab: IsTabDirector,
    user: User,
) -> Rendered<String> {
    Page::new()
        .user(user)
        .tournament(tournament.clone())
        .body(maud! {
            h1 {
                "Overview"
            }

            ul {
                li {
                    a href=(format!("/tournaments/{}/participants", tournament.id)) {
                        "Manage participants"
                    }
                }
            }
        })
        .render()
}
