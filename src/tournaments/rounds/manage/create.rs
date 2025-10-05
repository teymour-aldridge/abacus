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
    tournaments::{
        Tournament, categories::BreakCategory, manage::sidebar::SidebarWrapper,
        rounds::TournamentRounds,
    },
    util_resp::{StandardResponse, err_not_found, see_other_ok, success},
};

#[get("/tournaments/<tid>/rounds/create", rank = 1)]
pub async fn create_new_round(
    tid: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = TournamentRounds::fetch(&tid, &mut *conn).unwrap();

    let cats = tournament_break_categories::table
        .filter(tournament_break_categories::tournament_id.eq(&tournament.id))
        .order_by(tournament_break_categories::priority.asc())
        .load::<BreakCategory>(&mut *conn)
        .unwrap();

    success(Page::new()
        .tournament(tournament.clone())
        .user(user)
        .body(maud! {
            SidebarWrapper tournament=(&tournament) rounds=(&rounds) {
                h1 {
                    "Please select a category in which to create this round"
                }

                ul class="list-group" {
                    li class="list-group-item" {
                        a href=(format!("/tournaments/{tid}/rounds/in_round/create")) {
                            "In round"
                        }
                    }
                    @if !cats.is_empty() {
                        @for cat in &cats {
                            li class="list-group-item" {
                                a href=(format!("/tournaments/{tid}/rounds/{}/create", cat.id)) {
                                    (cat.name)
                                }
                            }
                        }
                    } @else {
                        li class="list-group-item" {
                            p {
                                "To create elimination rounds, please first set up break categories"
                                " (e.g. open, esl, etc)"
                                // todo: link to these break categories (also
                                // always link to "create new" break category)
                            }
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
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = TournamentRounds::fetch(&tid, &mut *conn).unwrap();

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
            .tournament(tournament.clone())
            .user(user)
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds)  {
                    form method="post" {
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
                        button type="submit" class="btn btn-primary" { "Submit" }
                        // todo: break categories
                    }
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
    form: Form<CreateNewRoundForm>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let break_cat = if category_id == "in_round" {
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
            tournament_rounds::kind.eq(if category_id == "in_round" {
                "P"
            } else {
                "E"
            }),
            tournament_rounds::break_category.eq(break_cat.map(|c| c.id)),
            tournament_rounds::completed.eq(false),
        ))
        .execute(&mut *conn)
        .unwrap();

    see_other_ok(Redirect::to(format!(
        "/tournaments/{}/rounds",
        tournament.id
    )))
}
