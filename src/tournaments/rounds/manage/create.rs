//! Create new rounds.
//!
//! Note: the current behaviour is to re-number all subsequent rounds. For
//! example, suppose we have Round 1, ..., Round 5 and we then create Round 6.
//! In this case, we increase the sequence of all the out-rounds.
//!
//! todo: document this

use axum::{Form, extract::Path, response::Redirect};
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::User,
    schema::{break_categories, rounds},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament, categories::BreakCategory, manage::sidebar::SidebarWrapper,
        rounds::TournamentRounds,
    },
    util_resp::{StandardResponse, err_not_found, see_other_ok, success},
};

pub async fn create_new_round(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = TournamentRounds::fetch(&tid, &mut *conn).unwrap();

    let cats = break_categories::table
        .filter(break_categories::tournament_id.eq(&tournament.id))
        .order_by(break_categories::priority.asc())
        .load::<BreakCategory>(&mut *conn)
        .unwrap();

    let current_rounds =
        crate::tournaments::rounds::Round::current_rounds(&tid, &mut *conn);

    success(Page::new()
        .tournament(tournament.clone())
        .user(user)
        .current_rounds(current_rounds)
        .body(maud! {
            SidebarWrapper tournament=(&tournament) rounds=(&rounds) active_page=(None) selected_seq=(None) {
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

pub async fn create_new_round_of_specific_category_page(
    Path((tid, category_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = TournamentRounds::fetch(&tid, &mut *conn).unwrap();

    let () = {
        if category_id == "in_round" {
        } else {
            let cat_exists = diesel::dsl::select(diesel::dsl::exists(
                break_categories::table.filter(
                    break_categories::tournament_id
                        .eq(&tournament.id)
                        .and(break_categories::id.eq(&category_id)),
                ),
            ))
            .get_result::<bool>(&mut *conn)
            .unwrap();
            if !cat_exists {
                return err_not_found();
            }
        }
    };

    let current_rounds =
        crate::tournaments::rounds::Round::current_rounds(&tid, &mut *conn);

    success(
        Page::new()
            .tournament(tournament.clone())
            .user(user)
            .current_rounds(current_rounds)
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) active_page=(None) selected_seq=(None) {
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

#[derive(Deserialize)]
pub struct CreateNewRoundForm {
    name: String,
    seq: u32,
}

pub async fn do_create_new_round_of_specific_category(
    Path((tid, category_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateNewRoundForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    if form.name.len() < 4 || form.name.len() > 32 {
        return crate::util_resp::bad_request(
            maud! { "Round name must be between 4 and 32 characters" }.render(),
        );
    }
    if form.seq < 1 || form.seq > 100 {
        return crate::util_resp::bad_request(
            maud! { "Sequence must be between 1 and 100" }.render(),
        );
    }

    let break_cat = if category_id == "in_round" {
        None
    } else {
        let cat = match break_categories::table
            .filter(break_categories::id.eq(&category_id))
            .filter(break_categories::tournament_id.eq(&tournament.id))
            .first::<BreakCategory>(&mut *conn)
            .optional()
            .unwrap()
        {
            Some(t) => t,
            None => return err_not_found(),
        };
        Some(cat)
    };

    diesel::insert_into(rounds::table)
        .values((
            rounds::id.eq(Uuid::now_v7().to_string()),
            rounds::tournament_id.eq(&tournament.id),
            rounds::seq.eq(form.seq as i64),
            rounds::name.eq(&form.name),
            rounds::kind.eq(if category_id == "in_round" { "P" } else { "E" }),
            rounds::break_category.eq(break_cat.map(|c| c.id)),
            rounds::completed.eq(false),
        ))
        .execute(&mut *conn)
        .unwrap();

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/rounds",
        tournament.id
    )))
}
