use hypertext::prelude::*;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{Round, TournamentRounds},
    },
    util_resp::{StandardResponse, success},
    widgets::actions::Actions,
};

/// Returns the tournament view for tab directors (i.e. superusers).
///
/// TODO: in future we probably want to unify the separate functions into a
/// single entity (which shows appropriate actions).
use axum::extract::Path;

pub async fn admin_view_tournament(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = TournamentRounds::fetch(&tid, &mut *conn).unwrap();
    let active_rounds = Round::current_rounds(&tournament.id, &mut *conn);

    success(Page::new()
        .user(user)
        .tournament(tournament.clone())
        .current_rounds(active_rounds.clone())
        .body(maud! {
            SidebarWrapper tournament=(&tournament) rounds=(&rounds) {
                h1 {
                    "Overview"
                }

                Actions options=(&[
                    (format!("/tournaments/{}/configuration", tournament.id).as_str(), "Configure tournament"),
                    (format!("/tournaments/{}/participants", tournament.id).as_str(), "Manage participants"),
                    (format!("/tournaments/{}/participants/privateurls", tournament.id).as_str(), "View private URLs"),
                    (format!("/tournaments/{}/rounds", tournament.id).as_str(), "Manage rounds"),
                    (format!("/tournaments/{}/feedback/manage", tournament.id).as_str(), "Manage feedback questions")
                ]);

                @if !active_rounds.is_empty() {
                    h1 {
                        "Currently active rounds"
                    }

                    a class="btn btn-primary" href=(format!("/tournaments/{}/rounds/{}", &tournament.id, active_rounds[0].seq)) {
                        "Manage current round"
                    }
                } @else {
                    p {
                        "Currently, there are no active rounds"
                    }
                }
            }
        })
        .render())
}
