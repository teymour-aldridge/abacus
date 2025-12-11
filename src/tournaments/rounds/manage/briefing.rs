use axum::{
    Form,
    extract::{Path, State},
    response::Redirect,
};
use chrono::Utc;
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;

use crate::{
    auth::User,
    schema::tournament_rounds,
    state::{AppState, Conn},
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{Round, TournamentRounds},
    },
    util_resp::{StandardResponse, err_not_found, see_other_ok, success},
};

#[derive(Clone)]
struct BriefingRoomView {
    tournament: Tournament,
    rounds: Vec<Round>,
}

impl Renderable for BriefingRoomView {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        let round_name = self
            .rounds
            .first()
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        maud! {
            div class="container" {
                h1 {
                    "Briefing Room for "
                    (round_name)
                }

                p class="lead" {
                    "This is the briefing room for "
                    (round_name)
                    " in "
                    (self.tournament.name)
                    "."
                }

                div class="row" {
                    @for round in &self.rounds {
                        div class="col-md-6 col-lg-4 mb-4 p-2" {
                            div class="card h-100" {
                                div class="card-body d-flex flex-column" {
                                    h5 class="card-title" { (round.name) }

                                    p class="card-text" {
                                        @if round.draw_status == "R" {
                                            span class="badge bg-success" {"Public"}
                                            " The draw is currently public."
                                        } @else {
                                            span class="badge bg-secondary" {"Private"}
                                            " The draw is not currently public."
                                        }
                                    }

                                    div class="mt-auto" {
                                        @if round.draw_status != "R" {
                                            form action=(format!("/tournaments/{}/rounds/{}/draws/setreleased", self.tournament.id, round.id)) method="post" {
                                                input type="text" value="true" hidden name="released";
                                                button class="btn btn-primary w-100" type="submit" {
                                                    "Publish Draw"
                                                }
                                            }
                                        } @else {
                                            form action=(format!("/tournaments/{}/rounds/{}/draws/setreleased", self.tournament.id, round.id)) method="post" {
                                                input type="text" value="false" hidden name="released";
                                                button class="btn btn-danger w-100" type="submit" {
                                                    "Unpublish Draw"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        .render_to(buffer)
    }
}

pub async fn get_briefing_room(
    State(state): State<AppState>,
    user: User<true>,
    Path((tournament_id, round_seq)): Path<(String, i64)>,
) -> impl axum::response::IntoResponse {
    let mut conn = state.pool.get().unwrap();
    let tournament = Tournament::fetch(&tournament_id, &mut conn).unwrap();
    let all_rounds =
        TournamentRounds::fetch(&tournament_id, &mut conn).unwrap();
    let rounds = Round::of_seq(round_seq, &tournament_id, &mut conn);

    let view = BriefingRoomView {
        tournament: tournament.clone(),
        rounds,
    };

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&all_rounds) {
                    (view)
                }
            })
            .render(),
    )
}

#[derive(Deserialize, Debug)]
pub struct SetDrawPublished {
    released: bool,
}

#[tracing::instrument(skip(conn))]
pub async fn set_draw_published(
    Path((tournament_id, round_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(released): axum::Form<SetDrawPublished>,
) -> StandardResponse {
    tracing::info!("Publishing draw");
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = match tournament_rounds::table
        .filter(tournament_rounds::id.eq(round_id))
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(round) => round,
        None => return err_not_found(),
    };

    diesel::update(tournament_rounds::table.find(round.id.clone()))
        .set((
            tournament_rounds::draw_status.eq(match released.released {
                true => "R",
                false => "C", // todo: the false => "C" might be wrong
            }),
            tournament_rounds::draw_released_at.eq(match released.released {
                true => Some(Utc::now().naive_utc()),
                false => None,
            }),
        ))
        .execute(&mut *conn)
        .unwrap();

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/rounds/{}/briefing",
        tournament_id, round.seq
    )))
}
