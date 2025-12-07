use axum::{
    Extension,
    extract::{Path, Query},
    response::Redirect,
};
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;
use tokio::task::spawn_blocking;

use crate::{
    auth::User,
    schema::tournament_rounds,
    state::{Conn, DbPool},
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{
            Round, TournamentRounds,
            draws::manage::drawalgs::{self, MakeDrawError, do_draw},
        },
    },
    util_resp::{
        StandardResponse, bad_request, err_not_found, see_other_ok, success,
    },
    widgets::alert::ErrorAlert,
};

#[derive(Deserialize)]
pub struct DrawCreateQuery {
    force: Option<bool>,
}

pub async fn generate_draw_page(
    Path((tournament_id, round_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = match tournament_rounds::table
        .filter(tournament_rounds::id.eq(&round_id))
        .filter(tournament_rounds::tournament_id.eq(&tournament.id))
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(r) => r,
        None => return err_not_found(),
    };

    let debates_exist = round.draw_status != "N";

    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    success(
        Page::new()
            .tournament(tournament.clone())
            .user(user)
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) {
                    @if debates_exist {
                        ErrorAlert
                            msg = "Warning: a draw already exists for this round. Creating
                             a new draw will delete the old draw!";

                        form method="post" action=(format!("/tournaments/{}/rounds/{}/draws/create?force=true", tournament_id, round_id)) {
                            button type="submit" class="btn btn-danger" {
                                "Delete existing draw and generate a new one"
                            }
                        }
                    } @else {
                        form method="post" {
                            button type="submit" class="btn btn-primary" {
                                "Generate draw"
                            }
                        }
                    }
                }
            })
            .render(),
    )
}

pub async fn do_generate_draw(
    Path((tournament_id, round_id)): Path<(String, String)>,
    Query(query): Query<DrawCreateQuery>,
    user: User<false>,
    Extension(pool): Extension<DbPool>,
) -> StandardResponse {
    let pool: DbPool = pool.clone();
    let round_id = round_id.to_string();
    let tournament_id = tournament_id.to_string();
    let force = query.force.unwrap_or(false);

    spawn_blocking(move || {
        let mut conn = pool.get().unwrap();

        let tournament = Tournament::fetch(&tournament_id, &mut conn)?;
        tournament.check_user_is_superuser(&user.id, &mut conn)?;

        let round = match tournament_rounds::table
            .filter(tournament_rounds::id.eq(&round_id))
            .first::<Round>(&mut conn)
            .optional()
            .unwrap()
        {
            Some(t) => t,
            None => {
                return err_not_found();
            }
        };

        let draw_result = do_draw(
            tournament.clone(),
            &round,
            Box::new(drawalgs::general::make_draw),
            &mut conn,
            force,
        );

        match draw_result {
            Ok(()) => see_other_ok(Redirect::to(&format!(
                "/tournaments/{tournament_id}/rounds/{}", round.seq
            ))),
            Err(e) => {
                let msg = match e {
                    MakeDrawError::InvalidConfiguration(str) => {
                        format!("Invalid configuration: {str}")
                    }
                    MakeDrawError::InvalidTeamCount(str) => {
                        format!("Wrong number of teams: {str}")
                    }
                    MakeDrawError::AlreadyInProgress => {
                        return bad_request(
                            Page::new()
                                .user(user)
                                .tournament(tournament)
                                .body(maud! {
                                    p {
                                        "Draw generation already in progress."
                                    }
                                    form method="post" action=(format!("/tournaments/{}/rounds/{}/draws/create?force=true", tournament_id, round_id)) {
                                        button type="submit" class="btn btn-danger" {
                                            "Override and generate new draw"
                                        }
                                    }
                                })
                                .render(),
                        );
                    }
                    MakeDrawError::TicketExpired => {
                        "Draw generation was cancelled.".to_string()
                    }
                    MakeDrawError::Panic => {
                        "Internal application error.".to_string()
                    }
                };

                bad_request(
                    Page::new()
                        .user(user)
                        .tournament(tournament)
                        .body(maud! {
                            p {
                                "We encountered the following error: " (msg)
                            }
                        })
                        .render(),
                )
            }
        }
    })
    .await
    .unwrap()
}
