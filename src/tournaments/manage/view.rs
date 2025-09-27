use hypertext::prelude::*;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{Tournament, rounds::Round},
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

    let active_rounds = Round::current_rounds(&tournament.id, &mut *conn);

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

            @if active_rounds.is_empty() {
                p {
                    "Currently, there are no active rounds"
                }
            } @else {
                @for round in &active_rounds {
                    (round.name)
                }
            }
        })
        .render())
}
