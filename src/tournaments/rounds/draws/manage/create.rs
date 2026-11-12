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
        config::EliminationDrawAlgorithm,
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

#[derive(Default, Deserialize)]
struct DrawCreateForm {
    elimination_draw_algorithm: Option<EliminationDrawAlgorithm>,
}

fn algorithm_picker(
    selected: EliminationDrawAlgorithm,
    opening_elimination: bool,
) -> impl Renderable {
    maud! {
        @if opening_elimination {
            div class="mb-3" {
                label for="elimination-draw-algorithm" class="form-label" {
                    "Opening-round pairing algorithm"
                }
                select id="elimination-draw-algorithm" name="elimination_draw_algorithm" class="form-select" {
                    @for algorithm in EliminationDrawAlgorithm::ALL {
                        option value=(algorithm.as_str()) selected[algorithm == selected] {
                            (algorithm.label())
                        }
                    }
                }
                div class="form-text" {
                    "Fold reverses alternating seed bands; Slide pairs corresponding seeds "
                    "across bands; Adjacent groups consecutive seeds; Random shuffles qualifiers. "
                    "The choice is saved as the tournament default after successful generation. "
                    "Changing it later does not reseed existing brackets."
                }
            }
        }
    }
}

fn generation_form<'a>(
    tournament: &'a Tournament,
    round: &'a Round,
    opening_elimination: bool,
    modal: bool,
    error: Option<&'a str>,
) -> impl Renderable + 'a {
    let tournament_id = &tournament.id;
    let selected =
        EliminationDrawAlgorithm::parse(&tournament.elimination_draw_algorithm)
            .unwrap_or_default();
    let action = format!(
        "/tournaments/{tournament_id}/rounds/{}/draws/create?force={}&modal={modal}",
        round.id,
        round.draw_status != "none"
    );
    maud! {
        @if let Some(error) = error { div class="alert alert-danger" role="alert" { (error) } }
        @if round.is_elim() {
            p {
                "The first elimination round uses the chosen pairing algorithm and break ranks. Later rounds preserve "
                "the bracket by pairing advancing teams according to previous debate numbers, "
                "without reseeding. All qualifying teams must be marked available."
            }
            p class="text-muted" {
                "Partial breaks and byes are not supported."
            }
        }
        @if round.draw_status != "none" {
            div class="alert alert-warning" role="alert" {
                "A draw already exists. Generating a new draw will delete the existing draw."
            }
        }
        @if modal {
            form method="post" action=(&action) hx-post=(&action) hx-target="#ui-modal" hx-swap="innerHTML" {
                (algorithm_picker(selected, opening_elimination))
                button type="submit" class="btn btn-primary" { "Generate draw" }
            }
        } @else {
            form method="post" action=(&action) {
                (algorithm_picker(selected, opening_elimination))
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
    let opening = round.is_elim()
        && drawalgs::previous_elimination_round_in_break_category(
            &round, &mut *conn,
        )
        .is_none();
    generation_response(&tournament, &round, opening, query.modal, None, user)
}

fn generation_response<const TX: bool>(
    tournament: &Tournament,
    round: &Round,
    opening_elimination: bool,
    modal: bool,
    error: Option<&str>,
    user: User<TX>,
) -> StandardResponse {
    let form =
        generation_form(tournament, round, opening_elimination, modal, error);
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
    body: axum::body::Bytes,
) -> StandardResponse {
    let pool: DbPool = pool.clone();
    let round_id_clone = round_id.to_string();
    let tournament_id_clone = tournament_id.to_string();
    let force = query.force.unwrap_or(false);
    // Preserve support for legacy empty-body POSTs as well as HTML forms.
    // Unlike an optional Form extractor, malformed choices are not ignored.
    let form = serde_urlencoded::from_bytes::<DrawCreateForm>(&body);

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

        let mut tournament =
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

        let form = match form {
            Ok(form) => form,
            Err(error) => {
                return DrawResult::DrawError(
                    MakeDrawError::InvalidConfiguration(format!(
                        "Invalid opening-round algorithm selection: {error}"
                    )),
                    tournament,
                    round,
                );
            }
        };
        if let Some(algorithm) = form.elimination_draw_algorithm {
            if !round.is_elim()
                || drawalgs::previous_elimination_round_in_break_category(
                    &round, &mut conn,
                )
                .is_some()
            {
                return DrawResult::DrawError(
                    MakeDrawError::InvalidConfiguration(
                        "The pairing algorithm can only be selected for the \
                         first elimination round in a break category."
                            .to_string(),
                    ),
                    tournament,
                    round,
                );
            }
            tournament.elimination_draw_algorithm =
                algorithm.as_str().to_string();
        }

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
            Box::new(drawalgs::general::make_inround_draw),
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
            let mut conn = pool.get().map_err(|_| {
                crate::util_resp::FailureResponse::ServerError(())
            })?;
            let opening = round.is_elim()
                && drawalgs::previous_elimination_round_in_break_category(
                    &round, &mut conn,
                )
                .is_none();
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
                opening,
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
            let opening = round.is_elim()
                && drawalgs::previous_elimination_round_in_break_category(
                    &round, &mut conn,
                )
                .is_none();
            let message = format!(
                "Previous round {} must be marked as completed first.",
                previous.name
            );
            generation_response(
                &tournament,
                &round,
                opening,
                query.modal,
                Some(&message),
                user,
            )
        }
    }
}
