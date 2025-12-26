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
            div {
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
                                        @if round.draw_status == "released_full" {
                                            span class="badge bg-success" {"Public (Teams & Judges)"}
                                            " The full draw is currently public."
                                        } @else if round.draw_status == "released_teams" {
                                            span class="badge bg-info" {"Public (Teams Only)"}
                                            " Only the team list is currently public."
                                        } @else {
                                            span class="badge bg-secondary" {"Private"}
                                            " The draw is not currently public."
                                        }
                                    }

                                    div class="mt-auto" {
                                        form action=(format!("/tournaments/{}/rounds/{}/draws/setreleased", self.tournament.id, round.id)) method="post" {
                                            div class="d-grid gap-2" {
                                                @if round.draw_status != "released_teams" && round.draw_status != "released_full" {
                                                    button class="btn btn-primary" type="submit" name="status" value="released_teams" {
                                                        "Publish Teams Only"
                                                    }
                                                    button class="btn btn-success" type="submit" name="status" value="released_full" {
                                                        "Publish Teams & Judges"
                                                    }
                                                } @else if round.draw_status == "released_teams" {
                                                    button class="btn btn-success" type="submit" name="status" value="released_full" {
                                                        "Publish Judges Too"
                                                    }
                                                    button class="btn btn-danger" type="submit" name="status" value="confirmed" {
                                                        "Hide Draw"
                                                    }
                                                } @else if round.draw_status == "released_full" {
                                                    button class="btn btn-warning" type="submit" name="status" value="released_teams" {
                                                        "Hide Judges (Teams Only)"
                                                    }
                                                    button class="btn btn-danger" type="submit" name="status" value="confirmed" {
                                                        "Hide Draw"
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
                SidebarWrapper tournament=(&tournament) rounds=(&all_rounds) active_page=(Some(crate::tournaments::manage::sidebar::SidebarPage::Briefing)) selected_seq=(Some(round_seq)) {
                    (view)
                }
            })
            .render(),
    )
}

#[derive(Deserialize, Debug)]
pub struct SetDrawPublished {
    status: String,
}

#[tracing::instrument(skip(conn))]
pub async fn set_draw_published(
    Path((tournament_id, round_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): axum::Form<SetDrawPublished>,
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
            tournament_rounds::draw_status.eq(&form.status),
            tournament_rounds::draw_released_at.eq(
                if form.status == "released_teams"
                    || form.status == "released_full"
                {
                    Some(Utc::now().naive_utc())
                } else {
                    None
                },
            ),
        ))
        .execute(&mut *conn)
        .unwrap();

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/rounds/{}/briefing",
        tournament_id, round.seq
    )))
}
