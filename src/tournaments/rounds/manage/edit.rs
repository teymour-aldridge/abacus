use crate::{
    auth::User,
    schema::rounds,
    state::Conn,
    tournaments::{Tournament, rounds::Round, snapshots::take_snapshot},
    ui::response::{ModalQuery, after_edit, form_page},
    util_resp::StandardResponse,
};
use axum::{
    Form,
    extract::{Path, Query},
};
use diesel::{prelude::*, result::DatabaseErrorKind};
use hypertext::prelude::*;
use serde::Deserialize;

#[derive(Deserialize, Default)]
pub struct EditRoundForm {
    pub(super) name: String,
    pub(super) seq: String,
}

pub(super) fn round_form<'a>(
    action: &'a str,
    form: &'a EditRoundForm,
    error: Option<&'a str>,
) -> impl Renderable + 'a {
    maud! {
        @if let Some(error) = error { div class="alert alert-danger" role="alert" { (error) } }
        form method="post" action=(action) hx-post=(format!("{action}?modal=true")) hx-target="#ui-modal" hx-swap="innerHTML" {
            div class="mb-3" {
                label for="round-name" class="form-label" { "Round name" }
                input id="round-name" class="form-control" name="name" value=(&form.name) required;
            }
            div class="mb-3" {
                label for="round-seq" class="form-label" { "Sequence" }
                input id="round-seq" class="form-control" type="number" name="seq" value=(&form.seq) min="1" required;
                p class="form-text" { "Rounds with the same sequence run concurrently." }
            }
            button type="submit" class="btn btn-primary" { "Save round" }
        }
    }
}

pub async fn edit_round_page(
    Path((tid, rid)): Path<(String, String)>,
    Query(query): Query<ModalQuery>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let round = Round::fetch(&tid, &rid, &mut *conn)?;
    let form = EditRoundForm {
        name: round.name,
        seq: round.seq.to_string(),
    };
    let action = format!("/tournaments/{tid}/rounds/{rid}/edit");
    form_page(
        query,
        "Edit round",
        &format!("/tournaments/{tid}/rounds/{}", round.seq),
        &tournament,
        user,
        round_form(&action, &form, None),
        false,
    )
}

pub async fn do_edit_round(
    Path((tid, rid)): Path<(String, String)>,
    Query(query): Query<ModalQuery>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<EditRoundForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let round = Round::fetch(&tid, &rid, &mut *conn)?;
    let max = rounds::table
        .filter(rounds::tournament_id.eq(&tid))
        .select(diesel::dsl::max(rounds::seq))
        .get_result::<Option<i64>>(&mut *conn)?
        .unwrap_or(1);
    let seq = form.seq.parse::<u32>().ok().map(i64::from);
    let first_elim = rounds::table
        .filter(rounds::tournament_id.eq(&tid))
        .filter(rounds::kind.eq("E"))
        .select(diesel::dsl::min(rounds::seq))
        .first::<Option<i64>>(&mut *conn)?;
    let last_prelim = rounds::table
        .filter(rounds::tournament_id.eq(&tid))
        .filter(rounds::kind.eq("P"))
        .select(diesel::dsl::max(rounds::seq))
        .first::<Option<i64>>(&mut *conn)?;
    let mut error = if !(4..=32).contains(&form.name.len()) {
        Some("Round name must be between 4 and 32 characters.")
    } else if seq.is_none_or(|s| s < 1) {
        Some("Sequence must be a positive integer.")
    } else if seq.is_some_and(|s| s > max.saturating_add(1)) {
        Some(
            "Sequence must be at most one greater than the largest existing sequence.",
        )
    } else if round.kind == "P"
        && seq.is_some_and(|s| first_elim.is_some_and(|e| s >= e))
    {
        Some("Preliminary rounds must precede elimination rounds.")
    } else if round.kind == "E"
        && seq.is_some_and(|s| last_prelim.is_some_and(|p| s <= p))
    {
        Some("Elimination rounds must follow preliminary rounds.")
    } else {
        None
    };
    if error.is_none() {
        match diesel::update(
            rounds::table
                .filter(rounds::id.eq(&rid))
                .filter(rounds::tournament_id.eq(&tid)),
        )
        .set((rounds::name.eq(&form.name), rounds::seq.eq(seq.unwrap())))
        .execute(&mut *conn)
        {
            Ok(_) => {
                take_snapshot(&tid, &mut *conn);
                return after_edit(
                    query,
                    format!("/tournaments/{tid}/rounds/{}", seq.unwrap()),
                );
            }
            Err(diesel::result::Error::DatabaseError(
                DatabaseErrorKind::UniqueViolation,
                _,
            )) => {
                error = Some(
                    "A round with this name already exists in this tournament.",
                )
            }
            Err(e) => return Err(e.into()),
        }
    }
    let action = format!("/tournaments/{tid}/rounds/{rid}/edit");
    form_page(
        query,
        "Edit round",
        &format!("/tournaments/{tid}/rounds/{}", round.seq),
        &tournament,
        user,
        round_form(&action, &form, error),
        true,
    )
}
