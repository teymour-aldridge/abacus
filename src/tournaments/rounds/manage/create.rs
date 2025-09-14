//! Create new rounds.
//!
//! Note: the current behaviour is to re-number all subsequent rounds. For
//! example, suppose we have Round 1, ..., Round 5 and we then create Round 6.
//! In this case, we increase the sequence of all the out-rounds.
//!
//! todo: document this

use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::{FromForm, form::Form, get, post, response::Redirect};
use uuid::Uuid;

use crate::{
    auth::User,
    schema::{tournament_break_categories, tournament_rounds},
    state::Conn,
    template::Page,
    tournaments::{Tournament, categories::BreakCategory},
    util_resp::{StandardResponse, err_not_found, see_other_ok, success},
};

#[get("/tournaments/<tid>/rounds/create")]
pub async fn create_new_round(
    tid: &str,
    mut conn: Conn<true>,
    user: User<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let cats = tournament_break_categories::table
        .filter(tournament_break_categories::tournament_id.eq(&tournament.id))
        .order_by(tournament_break_categories::priority.asc())
        .load::<BreakCategory>(&mut *conn)
        .unwrap();

    success(Page::new()
        .tournament(tournament)
        .user(user)
        .body(maud! {
            h1 {
                "Please select a category in which to create this round"

                ul {
                    li {
                        a href=(format!("/tournaments/{tid}/rounds/in_round/create")) {
                            "In round"
                        }
                    }
                    @for cat in &cats {
                        a href=(format!("/tournaments/{tid}/rounds/{}/create", cat.id)) {
                            (cat.name)
                        }
                    }
                }
            }
        })
        .render())
}

#[get("/tournaments/<tid>/rounds/<category_id>/create")]
pub async fn create_new_round_of_specific_category_page(
    tid: &str,
    category_id: &str,
    mut conn: Conn<true>,
    user: User<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let () = {
        if category_id == "in_round" {
        } else {
            let cat_exists = diesel::dsl::select(diesel::dsl::exists(
                tournament_break_categories::table.filter(
                    tournament_break_categories::tournament_id
                        .eq(&tournament.id)
                        .and(tournament_break_categories::id.eq(category_id)),
                ),
            ))
            .get_result::<bool>(&mut *conn)
            .unwrap();
            if !cat_exists {
                return err_not_found();
            }
        }
    };

    success(
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
                              aria-describedby="roundNameHelp";
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
                              aria-describedby="roundNameHelp";
                        div id="roundSeq" class="form-text" {
                            "The sequence number of the round."
                        }
                    }
                    // todo: break categories
                }
            })
            .render(),
    )
}

#[derive(FromForm)]
pub struct CreateNewRoundForm {
    #[field(validate = len(4..=32))]
    name: String,
    #[field(validate = range(1..=100))]
    seq: u32,
}

#[post("/tournaments/<tid>/rounds/<category_id>/create", data = "<form>")]
pub async fn do_create_new_round_of_specific_category(
    tid: &str,
    category_id: &str,
    mut conn: Conn<true>,
    form: Form<CreateNewRoundForm>,
    user: User<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let break_cat = if category_id == "in_round" {
        diesel::update(
            tournament_rounds::table.filter(
                tournament_rounds::tournament_id
                    .eq(&tournament.id)
                    .and(tournament_rounds::seq.ge(form.seq as i64)),
            ),
        )
        .set(tournament_rounds::seq.eq(tournament_rounds::seq + 1))
        .execute(&mut *conn)
        .unwrap();
        None
    } else {
        let cat = match tournament_break_categories::table
            .filter(tournament_break_categories::id.eq(category_id))
            .filter(
                tournament_break_categories::tournament_id.eq(&tournament.id),
            )
            .first::<BreakCategory>(&mut *conn)
            .optional()
            .unwrap()
        {
            Some(t) => t,
            None => return err_not_found(),
        };
        Some(cat)
    };

    diesel::insert_into(tournament_rounds::table)
        .values((
            tournament_rounds::id.eq(Uuid::now_v7().to_string()),
            tournament_rounds::tournament_id.eq(&tournament.id),
            tournament_rounds::seq.eq(form.seq as i64),
            tournament_rounds::name.eq(&form.name),
            tournament_rounds::kind.eq("P"),
            tournament_rounds::break_category.eq(break_cat.map(|c| c.id)),
        ))
        .execute(&mut *conn)
        .unwrap();

    see_other_ok(Redirect::to(format!(
        "/tournament/{}/rounds",
        tournament.id
    )))
}
