use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::{State, get, post, response::Redirect};
use tokio::task::spawn_blocking;

use crate::{
    auth::User,
    schema::{tournament_draws, tournament_rounds},
    state::{Conn, DbPool},
    template::Page,
    tournaments::{
        Tournament,
        rounds::{
            Round,
            draws::{
                Draw,
                manage::drawalgs::{self, MakeDrawError, do_draw},
            },
        },
    },
    util_resp::{
        StandardResponse, bad_request, err_not_found, see_other_ok, success,
    },
    widgets::alert::ErrorAlert,
};

#[get("/tournaments/<tournament_id>/rounds/<round_id>/draw/create", rank = 1)]
pub async fn generate_draw_page(
    tournament_id: &str,
    round_id: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
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

    let draw = tournament_draws::table
        .filter(tournament_draws::round_id.eq(&round.id))
        .first::<Draw>(&mut *conn)
        .optional()
        .unwrap();

    success(
        Page::new()
            .tournament(tournament)
            .user(user)
            .body(maud! {
                @if draw.is_some() {
                    ErrorAlert
                        msg = "Warning: a draw already exists for this round. Creating
                         a new draw will delete the old draw!";
                }

                form {
                    button type="submit" class="btn btn-primary" {
                        "Generate draw"
                    }
                }
            })
            .render(),
    )
}

#[post("/tournaments/<tournament_id>/rounds/<round_id>/draw/create")]
pub async fn do_generate_draw(
    tournament_id: &str,
    round_id: &str,
    pool: &State<DbPool>,
) -> StandardResponse {
    let pool: DbPool = pool.inner().clone();
    let round_id = round_id.to_string();
    let tournament_id = tournament_id.to_string();
    spawn_blocking(move || {
        let mut conn = pool.get().unwrap();

        let tournament = Tournament::fetch(&tournament_id, &mut conn)?;

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
            tournament,
            round,
            Box::new(drawalgs::random::gen_random),
            &mut conn,
            false,
        );

        match draw_result {
            Ok(draw_id) => see_other_ok(Redirect::to(format!(
                "/tournaments/{tournament_id}/rounds/{round_id}/draw/{draw_id}",
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
                        "Draw generation already in progress".to_string()
                    }
                    MakeDrawError::TicketExpired => {
                        "Draw generation was cancelled.".to_string()
                    }
                };

                bad_request(
                    maud! {
                        p {
                            "We encountered the following error: " (msg)
                        }
                    }
                    .render(),
                )
            }
        }
    })
    .await
    .unwrap()
}
