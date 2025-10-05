use std::collections::HashMap;

use diesel::prelude::*;
use hypertext::prelude::*;
use itertools::Itertools;
use rocket::get;

use crate::{
    auth::User,
    schema::tournament_break_categories,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        categories::BreakCategory,
        manage::sidebar::SidebarWrapper,
        rounds::{Round, TournamentRounds},
    },
    util_resp::{StandardResponse, success},
    widgets::actions::Actions,
};

pub mod availability;
pub mod create;
pub mod draw_edit;
pub mod edit;
pub mod view;

#[get("/tournaments/<tid>/rounds")]
pub async fn manage_rounds_page(
    tid: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = TournamentRounds::fetch(tid, &mut *conn)
        .expect("failed to retrieve rounds");
    let categories2rounds = rounds.categories();
    let categories = tournament_break_categories::table
        .filter(tournament_break_categories::tournament_id.eq(&tid))
        .load::<BreakCategory>(&mut *conn)
        .unwrap()
        .into_iter()
        .map(|c| (c.id.clone(), c))
        .collect::<HashMap<_, _>>();

    let get_seq = |round: &Round| round.seq;
    let min_in_round_seq = rounds.prelim.iter().map(get_seq).min();
    let max_in_round_seq = rounds.prelim.iter().map(get_seq).max();

    // todo: case with no outrounds
    let min_outround_seq = rounds.elim.iter().map(get_seq).min();
    let max_outround_seq = rounds.elim.iter().map(get_seq).max();

    assert!(min_outround_seq.is_none() || max_outround_seq.is_some());

    success(Page::new()
        .tournament(tournament.clone())
        .user(user)
        .body(maud! {
            SidebarWrapper tournament=(&tournament) rounds=(&rounds) {
                h1 {
                    "Rounds for " (tournament.name)
                }

                Actions options = (&[
                    (format!("/tournaments/{}/rounds/create", tournament.id).as_str(), "Create round")
                ]);

                p {
                    "Rounds which take place concurrently should share the same"
                    "sequence number -- where two rounds have the same sequence"
                    "number, it will be possible to generate and edit the draw"
                    "for both rounds simultaneously."
                }

                div class = "container" {
                    h3 {
                        "Preliminary rounds"
                    }

                    @if let Some(min_in_round_seq) = min_in_round_seq {
                        @let max_in_round_seq = max_in_round_seq.unwrap();
                            @for seq in min_in_round_seq..=max_in_round_seq {
                                div class = "row p-3" {
                                    @for prelim in rounds.prelim.iter().filter(|round| round.seq == seq) {
                                        div class="col" {
                                            div class = "card" {
                                                div class="card-body" {
                                                    h5 class="card-title" {
                                                        span class="badge text-bg-secondary" {
                                                            "Seq " (seq)
                                                        }

                                                        " "
                                                        (prelim.name)
                                                    }

                                                    p class="card-text" {
                                                        "Status:"
                                                        @match rounds.statuses.get(&prelim.id).unwrap() {
                                                            crate::tournaments::rounds::RoundStatus::NotStarted => {
                                                                span class="m-1 badge rounded-pill text-bg-secondary" {
                                                                    "Not started"
                                                                }
                                                            },
                                                            crate::tournaments::rounds::RoundStatus::InProgress => {
                                                                span class="m-1 badge rounded-pill text-bg-warning" {
                                                                    "In progress"
                                                                }
                                                            },
                                                            crate::tournaments::rounds::RoundStatus::Completed => {
                                                                span class="m-1 badge rounded-pill text-bg-success" {
                                                                    "Completed"
                                                                }
                                                            },
                                                            crate::tournaments::rounds::RoundStatus::Draft => {
                                                                span class="m-1 badge rounded-pill text-bg-dark" {
                                                                    "Draft"
                                                                }
                                                            },
                                                        }
                                                    }

                                                    a class="btn btn-primary" href=(format!("/tournaments/{tid}/rounds/{}/edit", prelim.id))
                                                    {
                                                        "Edit"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                    } @else {
                        "Note: there are no preliminary rounds in this tournament."
                    }
                }
                div class = "container" {
                    h3 {
                        "Elimination rounds"
                    }

                    @if let Some(min_outround_seq) = min_outround_seq {
                        @let max_outround_seq = max_outround_seq.unwrap();
                        @for i in min_outround_seq..=max_outround_seq {
                            div class = "row" {
                                @for (_, rounds) in categories2rounds.iter().sorted_by_key(
                                    |(cat_id, _)| {
                                        categories.get(*cat_id).unwrap().priority
                                    }
                                ) {
                                    div class = "col" {
                                        @if let Some(round) =
                                            rounds.iter().find(|round| round.seq == i) {
                                            (round.name)
                                            a href=(format!("/tournaments/{tid}/rounds/{}", round.id))
                                            {
                                                " (edit)"
                                            }
                                        } @else {
                                            "---"
                                        }
                                    }
                                }
                            }
                        }
                    } @else {
                        "Note: there are no elimination rounds in this tournament."
                    }
                }
            }
        })
        .render())
}
