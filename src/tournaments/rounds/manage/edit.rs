use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::{FromForm, form::Form, get, post, response::Redirect};

use crate::{
    auth::User,
    schema::tournament_rounds,
    state::Conn,
    template::Page,
    tournaments::{Tournament, rounds::Round, snapshots::take_snapshot},
    util_resp::{
        FailureResponse, StandardResponse, SuccessResponse, err_not_found,
        see_other_ok,
    },
};

#[get("/tournaments/<tid>/rounds/<rid>/edit")]
pub async fn edit_round_page(
    tid: &str,
    rid: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = match tournament_rounds::table
        .filter(tournament_rounds::tournament_id.eq(tid))
        .filter(tournament_rounds::id.eq(rid))
        .first::<Round>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(round) => round,
        None => return Err(FailureResponse::NotFound(())),
    };

    Ok(SuccessResponse::Success(
        Page::new()
            .tournament(tournament)
            .user(user)
            .body(maud! {
                form {
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
                    // todo: break categories
                }
            })
            .render(),
    ))
}

#[derive(FromForm)]
pub struct EditRoundForm {
    #[field(validate = len(4..=32))]
    name: String,
    #[field(validate = range(1..))]
    seq: u32,
}

#[post("/tournaments/<tid>/rounds/<rid>/edit", data = "<form>")]
pub async fn do_edit_round(
    tid: &str,
    rid: &str,
    user: User<true>,
    form: Form<EditRoundForm>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round = match tournament_rounds::table
        .filter(tournament_rounds::id.eq(&tid))
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

    if max + 1 < (form.seq as i64) {
        return Err(FailureResponse::BadRequest(
            Page::new()
                .user(user)
                .tournament(tournament)
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

    take_snapshot(tid, &mut *conn);

    see_other_ok(Redirect::to(format!("/tournaments/{tid}/rounds")))
}
