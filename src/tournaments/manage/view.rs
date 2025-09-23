use hypertext::prelude::*;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::Tournament,
    util_resp::{StandardResponse, success},
    widgets::actions::Actions,
};

/// Returns the tournament view for tab directors (i.e. superusers).
///
/// TODO: in future we probably want to unify the separate functions into a
/// single entity (which shows appropriate actions).
pub async fn admin_view_tournament(
    tid: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    success(Page::new()
        .user(user)
        .tournament(tournament.clone())
        .body(maud! {
            h1 {
                "Overview"
            }

            Actions options=(&[
                (format!("/tournaments/{}/participants", tournament.id).as_str(), "Manage participants"),
                (format!("/tournaments/{}/rounds", tournament.id).as_str(), "Manage rounds")
            ]);
        })
        .render())
}
