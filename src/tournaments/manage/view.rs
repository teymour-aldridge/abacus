use hypertext::prelude::*;
use rocket::get;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::Tournament,
    util_resp::{StandardResponse, success},
};

#[get("/tournaments/<tid>", rank = 1)]
/// Returns the tournament view for an administrator. We use lower-ranking
/// routes to handle other cases.
pub async fn admin_view_tournament(
    tid: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_tab_dir(&user.id, &mut *conn)?;

    success(Page::new()
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
        .render())
}
