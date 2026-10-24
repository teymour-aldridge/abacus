use axum::{
    Extension,
    extract::{Path, Query},
    response::Redirect,
};
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;
use tokio::{sync::broadcast::Sender, task::spawn_blocking};

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::rounds,
    state::{Conn, DbPool},
    template::Page,
    tournaments::{
        Tournament,
        rounds::{
            Round,
            draws::manage::drawalgs::{self, MakeDrawError, do_draw},
        },
    },
    util_resp::{
        StandardResponse, bad_request, err_not_found, see_other_ok, success,
    },
};

#[derive(Deserialize)]
pub struct DrawCreateQuery {
    force: Option<bool>,
    #[serde(default)]
    modal: bool,
}

fn generation_form<'a>(
    tournament_id: &'a str,
    round: &'a Round,
    modal: bool,
    error: Option<&'a str>,
) -> impl Renderable + 'a {
    let action = format!(
        "/tournaments/{tournament_id}/rounds/{}/draws/create?force={}&modal={modal}",
        round.id,
        round.draw_status != "none"
    );
    maud! {
        @if let Some(error) = error { div class="alert alert-danger" role="alert" { (error) } }
        @if round.draw_status != "none" {
            div class="alert alert-warning" role="alert" {
                "A draw already exists. Generating a new draw will delete the existing draw."
            }
        }
        @if modal {
            form method="post" action=(&action) hx-post=(&action) hx-target="#ui-modal" hx-swap="innerHTML" {
                button type="submit" class="btn btn-primary" { "Generate draw" }
            }
        } @else {
            form method="post" action=(&action) {
                button type="submit" class="btn btn-primary" { "Generate draw" }
            }
        }
    }
}

pub async fn generate_draw_page(
    Path((tournament_id, round_id)): Path<(String, String)>,
    Query(query): Query<DrawCreateQuery>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let round = rounds::table
        .filter(rounds::id.eq(&round_id))
        .filter(rounds::tournament_id.eq(&tournament.id))
        .first::<Round>(&mut *conn)?;
    generation_response(&tournament, &round, query.modal, None, user)
}

fn generation_response<const TX: bool>(
    tournament: &Tournament,
    round: &Round,
    modal: bool,
    error: Option<&str>,
    user: User<TX>,
) -> StandardResponse {
    let form = generation_form(&tournament.id, round, modal, error);
    let return_url =
        format!("/tournaments/{}/rounds/{}/draw", tournament.id, round.seq);
    if modal {
        success(
            crate::ui::presentation::modal("Generate draw", &return_url, form)
                .render(),
        )
    } else {
        let page = Page::new()
            .tournament(tournament.clone())
            .user(user)
            .body(maud! { div class="container py-4" {
                h1 { "Generate draw: " (round.name) }
                (form)
                a href=(&return_url) { "Return to draw" }
            } })
            .render();
        if error.is_some() {
            bad_request(page)
        } else {
            success(page)
        }
    }
}

pub async fn do_generate_draw(
    Path((tournament_id, round_id)): Path<(String, String)>,
    Query(query): Query<DrawCreateQuery>,
    user: User<false>,
    Extension(pool): Extension<DbPool>,
    Extension(tx): Extension<Sender<Msg>>,
) -> StandardResponse {
    let pool: DbPool = pool.clone();
    let round_id_clone = round_id.to_string();
    let tournament_id_clone = tournament_id.to_string();
    let force = query.force.unwrap_or(false);

    enum DrawResult {
        Success(Tournament, Round),
        DrawError(MakeDrawError, Tournament, Round),
        AuthError(StandardResponse),
        PriorIncompleteRoundError(Round, Tournament),
    }

    let user_id = user.id.clone();

    let generation_pool = pool.clone();
    let result = spawn_blocking(move || {
        let mut conn = match generation_pool.get() {
            Ok(conn) => conn,
            Err(_) => {
                return DrawResult::AuthError(Err(
                    crate::util_resp::FailureResponse::ServerError(()),
                ));
            }
        };

        let tournament =
            match Tournament::fetch(&tournament_id_clone, &mut conn) {
                Ok(t) => t,
                Err(e) => return DrawResult::AuthError(Err(e)),
            };
        if let Err(e) = tournament.check_user_is_superuser(&user_id, &mut conn)
        {
            return DrawResult::AuthError(Err(e));
        }

        let round = match rounds::table
            .filter(rounds::id.eq(&round_id_clone))
            .filter(rounds::tournament_id.eq(&tournament.id))
            .first::<Round>(&mut conn)
            .optional()
        {
            Ok(Some(t)) => t,
            Err(error) => return DrawResult::AuthError(Err(error.into())),
            Ok(None) => {
                return DrawResult::AuthError(err_not_found());
            }
        };

        if round.draw_status != "none" && !force {
            return DrawResult::DrawError(
                MakeDrawError::InvalidConfiguration(
                    "a draw already exists for this round".to_string(),
                ),
                tournament,
                round,
            );
        }

        if round.completed {
            return DrawResult::DrawError(
                MakeDrawError::InvalidConfiguration(
                    "completed rounds cannot have their draw regenerated"
                        .to_string(),
                ),
                tournament,
                round,
            );
        }

        if let Some(round) =
            round.preceeding_incomplete_round_of_lowest_seq(&mut conn)
        {
            return DrawResult::PriorIncompleteRoundError(round, tournament);
        }

        let draw_result = do_draw(
            tournament.clone(),
            &round,
            Box::new(drawalgs::general::make_draw),
            &mut conn,
            force,
        );

        match draw_result {
            Ok(()) => DrawResult::Success(tournament, round),
            Err(e) => DrawResult::DrawError(e, tournament, round),
        }
    })
    .await
    .map_err(|_| crate::util_resp::FailureResponse::ServerError(()))?;

    match result {
        DrawResult::Success(tournament, round) => {
            let _ = tx.send(Msg {
                tournament: tournament.clone(),
                inner: MsgContents::DrawUpdated(round.id.clone()),
            });
            let path = format!(
                "/tournaments/{tournament_id}/rounds/{}/draw",
                round.seq
            );
            if query.modal {
                crate::util_resp::hx_redirect(path)
            } else {
                see_other_ok(Redirect::to(&path))
            }
        }
        DrawResult::AuthError(response) => response,
        DrawResult::DrawError(error, tournament, round) => {
            let message = match error {
                MakeDrawError::InvalidConfiguration(message) => format!("Invalid configuration: {message}"),
                MakeDrawError::InvalidTeamCount(message) => format!("Wrong number of teams: {message}"),
                MakeDrawError::AlreadyInProgress => "Draw generation is already in progress. Please wait before retrying.".into(),
                MakeDrawError::TicketExpired => "Draw generation was cancelled.".into(),
                MakeDrawError::Panic => "Internal application error.".into(),
            };
            generation_response(
                &tournament,
                &round,
                query.modal,
                Some(&message),
                user,
            )
        }
        DrawResult::PriorIncompleteRoundError(previous, tournament) => {
            let mut conn = pool.get().map_err(|_| {
                crate::util_resp::FailureResponse::ServerError(())
            })?;
            let round = rounds::table
                .filter(rounds::id.eq(&round_id))
                .filter(rounds::tournament_id.eq(&tournament_id))
                .first::<Round>(&mut conn)?;
            let message = format!(
                "Previous round {} must be marked as completed first.",
                previous.name
            );
            generation_response(
                &tournament,
                &round,
                query.modal,
                Some(&message),
                user,
            )
        }
    }
}
