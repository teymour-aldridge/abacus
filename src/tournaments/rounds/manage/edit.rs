use axum::{Form, extract::Path, response::Redirect};
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;

use crate::{
    auth::User,
    schema::tournament_rounds,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{Round, TournamentRounds},
        snapshots::take_snapshot,
    },
    util_resp::{
        FailureResponse, StandardResponse, SuccessResponse, err_not_found,
        see_other_ok,
    },
};

pub async fn edit_round_page(
    Path((tid, rid)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = match tournament_rounds::table
        .filter(tournament_rounds::tournament_id.eq(&tid))
        .filter(tournament_rounds::id.eq(&rid))
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(round) => round,
        None => return Err(FailureResponse::NotFound(())),
    };

    let rounds = TournamentRounds::fetch(&tid, &mut *conn).unwrap();

    let current_rounds = Round::current_rounds(&tid, &mut *conn);

    Ok(SuccessResponse::Success(
        Page::new()
            .tournament(tournament.clone())
            .user(user)
            .current_rounds(current_rounds)
            .body(maud! {
                SidebarWrapper rounds=(&rounds) tournament=(&tournament) active_page=(Some(crate::tournaments::manage::sidebar::SidebarPage::Setup)) selected_seq=(Some(round.seq)) {
                    form method="post" {
                        div class="mb-3" {
                            label for="roundName" class="form-label" {
                                "Round name"
                            }
                            input type="text"
                                  name="name"
                                  class="form-control"
                                  id="roundName"
                                  aria-describedby="roundNameHelp"
                                  value=(round.name);
                            div id="roundHelp" class="form-text" {
                                "A human-readable description of the round, for"
                                " example 'Round 1', or 'Grand final'"
                            }
                        }
                        div class="mb-3" {
                            label for="roundSeq" class="form-label" {
                                "Round sequence"
                            }
                            input type="integer"
                                  name="seq"
                                  class="form-control"
                                  id="roundSeq"
                                  aria-describedby="roundNameHelp"
                                  value=(round.seq);
                            div id="roundSeq" class="form-text" {
                                "A human-readable description of the round, for"
                                " example 'Round 1', or 'Grand final'"
                            }
                        }
                        button type="submit" class="btn btn-primary" { "Submit" }
                        // todo: break categories
                    }
                }

            })
            .render(),
    ))
}

#[derive(Deserialize)]
pub struct EditRoundForm {
    name: String,
    seq: u32,
}

pub async fn do_edit_round(
    Path((tid, rid)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<EditRoundForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    if form.name.len() < 4 || form.name.len() > 32 {
        return crate::util_resp::bad_request(
            maud! { "Round name must be between 4 and 32 characters" }.render(),
        );
    }
    if form.seq < 1 {
        return crate::util_resp::bad_request(
            maud! { "Sequence must be at least 1" }.render(),
        );
    }

    let round = match tournament_rounds::table
        .filter(tournament_rounds::tournament_id.eq(&tid))
        .filter(tournament_rounds::id.eq(&rid))
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(round) => round,
        None => return err_not_found(),
    };

    let max = tournament_rounds::table
        .filter(tournament_rounds::tournament_id.eq(&tid))
        .select(diesel::dsl::max(tournament_rounds::seq))
        .get_result::<Option<i64>>(&mut *conn)
        .unwrap()
        .unwrap_or(1i64);

    let current_rounds = Round::current_rounds(&tid, &mut *conn);

    if max + 1 < (form.seq as i64) {
        return Err(FailureResponse::BadRequest(
            Page::new()
                .user(user)
                .tournament(tournament)
                .current_rounds(current_rounds)
                .body(maud! {
                    p {
                        "Error: round index is too large. It must be at most
                         one more than the current largest index, which is "
                         (max)
                        "."
                    }
                })
                .render(),
        ));
    }

    let n = diesel::update(
        tournament_rounds::table.filter(tournament_rounds::id.eq(round.id)),
    )
    .set((
        tournament_rounds::name.eq(&form.name),
        tournament_rounds::seq.eq(&(form.seq as i64)),
    ))
    .execute(&mut *conn)
    .unwrap();
    assert_eq!(n, 1);

    take_snapshot(&tid, &mut *conn);

    see_other_ok(Redirect::to(&format!("/tournaments/{tid}/rounds")))
}
