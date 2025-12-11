use hypertext::prelude::*;
use itertools::Itertools;

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

                @if active_rounds.is_empty() {
                    p {
                        "Currently, there are no active rounds"
                    }
                } @else {
                    h1 {
                        "Currently active rounds"
                    }
                    Actions options=(&[
                        (
                            format!(
                                "/tournaments/{}/rounds/{}/availability",
                                &tournament.id,
                                active_rounds[0].seq
                            )
                            .as_str(),
                            "Manage availability for current rounds"
                        ),
                        (
                            format!(
                                "/tournaments/{}/rounds/draws/edit?{}",
                                &tournament.id,
                                active_rounds
                                    .iter()
                                    .enumerate()
                                    .map(|(i, r)| format!("rounds[{}]={}", i, r.id))
                                    .join("&")
                            )
                            .as_str(),
                            "Edit draws for all active rounds concurrently."
                        )
                    ]);
                    div class = "row" {
                        @for round in &active_rounds {
                            div class = "col" {
                                div class="card" {
                                    div class="card-body" {
                                        h5 class="card-title" {
                                            (round.name)
                                        }
                                    }
                                    div class="card-body" {
                                        @let status = &rounds.statuses[&round.id];
                                        @match status {
                                            crate::tournaments::rounds::RoundStatus::NotStarted => {
                                                a class="btn btn-primary" href = (format!("/tournaments/{tid}/rounds/{}/draws/create", round.id)) {
                                                    "Create draw"
                                                }
                                            },
                                            crate::tournaments::rounds::RoundStatus::InProgress => {
                                                a class="btn btn-primary" href = (format!("/tournaments/{tid}/rounds/{}", round.id)) {
                                                    "View draw"
                                                }
                                            },
                                            crate::tournaments::rounds::RoundStatus::Completed => {
                                                a class="btn btn-primary" href = (format!("/tournaments/{tid}/rounds/{}/draws/create", round.id)) {
                                                    "View draw"
                                                }
                                            },
                                            crate::tournaments::rounds::RoundStatus::Draft => {
                                                a class="btn btn-primary" href = (format!("/tournaments/{tid}/rounds/draws/edit?rounds={}", round.id)) {
                                                    "Edit draw"
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        })
        .render())
}
