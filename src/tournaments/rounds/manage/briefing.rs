use axum::{
    Form,
    extract::{Path, Query},
    response::Redirect,
};
use chrono::Utc;
use diesel::{prelude::*, sqlite::Sqlite};
use hypertext::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;

use crate::{
    auth::User,
    schema::{motions_of_round, rounds},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{Round, TournamentRounds},
    },
    util_resp::{StandardResponse, bad_request, see_other_ok, success},
};

const VALID_DRAW_STATUSES: &[&str] = &[
    "none",
    "draft",
    "confirmed",
    "released_teams",
    "released_full",
];

#[derive(Clone)]
struct BriefingRoomView {
    tournament: Tournament,
    rounds: Vec<Round>,
    can_publish_motions: HashMap<String, bool>,
    can_publish_full_draw: HashMap<String, bool>,
    error: Option<String>,
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

                @if let Some(error) = &self.error { div class="alert alert-danger" role="alert" { (error) } }

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
                                        @if !self.tournament.show_draws {
                                            span class="badge bg-danger" { "Unpublished" }
                                            " Public draw viewing is disabled for this tournament."
                                        } @else if round.draw_status == "released_full" {
                                            span class="badge bg-success" {"Public (Teams & Judges)"}
                                            " The full draw is currently public."
                                        } @else if round.draw_status == "released_teams" {
                                            span class="badge bg-info" {"Public (Teams Only)"}
                                            " Only the team list is currently public."
                                        } @else {
                                            span class="badge bg-danger" {"Unpublished"}
                                            " The draw is not currently public."
                                        }
                                    }

                                    h6 class="card-subtitle mb-2 text-muted" { "Draw Release" }
                                    div class="mt-auto" {
                                        form action=(format!("/tournaments/{}/rounds/{}/draws/setreleased", self.tournament.id, round.id)) method="post"
                                            hx-post=(format!("/tournaments/{}/rounds/{}/draws/setreleased?modal=true", self.tournament.id, round.id))
                                            hx-target="#ui-modal" hx-swap="innerHTML" {
                                            @let can_publish_full_draw =
                                                *self.can_publish_full_draw.get(&round.id).unwrap_or(&false);
                                            div class="d-grid gap-2" {
                                                @if round.draw_status != "released_teams" && round.draw_status != "released_full" {
                                                    button class="btn btn-warning" type="submit" name="status" value="released_teams" {
                                                        "Publish Teams Only"
                                                    }
                                                    @if can_publish_full_draw {
                                                        button class="btn btn-danger" type="submit" name="status" value="released_full" {
                                                            "Publish Teams & Judges"
                                                        }
                                                    } @else {
                                                        button class="btn btn-danger" type="submit" name="status" value="released_full" disabled title="Add a motion before publishing the full draw" {
                                                            "Publish Teams & Judges"
                                                        }
                                                    }
                                                } @else if round.draw_status == "released_teams" {
                                                    @if can_publish_full_draw {
                                                        button class="btn btn-danger" type="submit" name="status" value="released_full" {
                                                            "Publish Judges Too"
                                                        }
                                                    } @else {
                                                        button class="btn btn-danger" type="submit" name="status" value="released_full" disabled title="Add a motion before publishing the full draw" {
                                                            "Publish Judges Too"
                                                        }
                                                    }
                                                    button class="btn btn-secondary" type="submit" name="status" value="confirmed" {
                                                        "Hide Draw"
                                                    }
                                                } @else if round.draw_status == "released_full" {
                                                    button class="btn btn-secondary" type="submit" name="status" value="released_teams" {
                                                        "Hide Judges (show teams only)"
                                                    }
                                                    button class="btn btn-secondary" type="submit" name="status" value="confirmed" {
                                                        "Hide Draw"
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    @if *self.can_publish_motions.get(&round.id).unwrap_or(&false) {
                                        hr class="my-3";
                                        h6 class="card-subtitle mb-2 text-muted" { "Motions Release" }
                                        p class="card-text" { "The motions for this round are not yet public." }
                                        form action=(format!("/tournaments/{}/rounds/{}/motions/publish", self.tournament.id, round.id)) method="post" hx-post=(format!("/tournaments/{}/rounds/{}/motions/publish?modal=true", self.tournament.id, round.id)) hx-target="#ui-modal" {
                                            div class="d-grid gap-2" {
                                                button class="btn btn-warning" type="submit" { "Publish Motions" }
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

#[derive(Deserialize, Default, Debug)]
pub struct BriefingQuery {
    #[serde(default)]
    modal: bool,
    error: Option<String>,
}

pub async fn get_briefing_room(
    Path((tournament_id, round_seq)): Path<(String, i64)>,
    Query(query): Query<BriefingQuery>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let all_rounds = TournamentRounds::fetch(&tournament_id, &mut *conn)?;
    let rounds = rounds::table
        .filter(rounds::tournament_id.eq(&tournament_id))
        .filter(rounds::seq.eq(round_seq))
        .load::<Round>(&mut *conn)?;

    if rounds.is_empty() {
        return crate::util_resp::err_not_found();
    }
    let mut can_publish_motions = HashMap::new();
    let mut can_publish_full_draw = HashMap::new();
    for round in &rounds {
        let has_motion = round_has_motion(&round.id, &mut *conn)?;
        let unpublished = motions_of_round::table
            .filter(motions_of_round::tournament_id.eq(&tournament_id))
            .filter(motions_of_round::round_id.eq(&round.id))
            .filter(motions_of_round::published_at.is_null())
            .count()
            .get_result::<i64>(&mut *conn)?
            > 0;
        can_publish_motions.insert(round.id.clone(), unpublished);
        can_publish_full_draw.insert(round.id.clone(), has_motion);
    }

    let view = BriefingRoomView {
        tournament: tournament.clone(),
        rounds,
        can_publish_motions,
        can_publish_full_draw,
        error: query.error,
    };
    if query.modal {
        let return_url =
            format!("/tournaments/{tournament_id}/rounds/{round_seq}/draw");
        return success(
            crate::ui::presentation::modal(
                "Publication and briefing",
                &return_url,
                view,
            )
            .render(),
        );
    }

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
    Query(query): Query<BriefingQuery>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): axum::Form<SetDrawPublished>,
) -> StandardResponse {
    tracing::info!("Publishing draw");
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = rounds::table
        .filter(rounds::id.eq(&round_id))
        .filter(rounds::tournament_id.eq(&tournament_id))
        .first::<Round>(&mut *conn)?;
    let error = if !VALID_DRAW_STATUSES.contains(&form.status.as_str())
        || form.status == "none"
    {
        Some("Invalid draw status.")
    } else if round.draw_status == "none" {
        Some("Generate a draw before publishing it.")
    } else if form.status == "released_full"
        && !round_has_motion(&round.id, &mut *conn)?
    {
        Some("Add a motion before publishing the full draw.")
    } else {
        None
    };
    if let Some(error) = error {
        if query.modal {
            return get_briefing_room(
                Path((tournament_id, round.seq)),
                Query(BriefingQuery {
                    modal: true,
                    error: Some(error.into()),
                }),
                user,
                conn,
            )
            .await;
        }
        return bad_request(
            maud! { div class="alert alert-danger" { (error) } }.render(),
        );
    }

    diesel::update(rounds::table.find(round.id.clone()))
        .set((
            rounds::draw_status.eq(&form.status),
            rounds::draw_released_at.eq(
                if form.status == "released_teams"
                    || form.status == "released_full"
                {
                    Some(Utc::now().naive_utc())
                } else {
                    None
                },
            ),
        ))
        .execute(&mut *conn)?;

    let path =
        format!("/tournaments/{tournament_id}/rounds/{}/draw", round.seq);
    if query.modal {
        crate::util_resp::hx_redirect(path)
    } else {
        see_other_ok(Redirect::to(&path))
    }
}

fn round_has_motion(
    round_id: &str,
    conn: &mut impl diesel::connection::LoadConnection<Backend = Sqlite>,
) -> Result<bool, diesel::result::Error> {
    Ok(motions_of_round::table
        .filter(motions_of_round::round_id.eq(round_id))
        .count()
        .get_result::<i64>(conn)?
        > 0)
}
