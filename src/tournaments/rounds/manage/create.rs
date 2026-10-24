use super::edit::{EditRoundForm, round_form};
use crate::{
    auth::User,
    schema::{break_categories, rounds},
    state::Conn,
    tournaments::{
        Tournament, categories::BreakCategory, rounds::TournamentRounds,
    },
    ui::{
        View,
        presentation::action,
        response::{ModalQuery, after_edit, form_page},
    },
    util_resp::StandardResponse,
};
use axum::{
    Form,
    extract::{Path, Query},
};
use diesel::{prelude::*, result::DatabaseErrorKind};
use hypertext::prelude::*;

pub async fn create_new_round(
    Path(tid): Path<String>,
    Query(query): Query<ModalQuery>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let cats = break_categories::table
        .filter(break_categories::tournament_id.eq(&tid))
        .order(break_categories::priority.asc())
        .load::<BreakCategory>(&mut *conn)?;
    let tid = tid.as_str();
    let body = maud! {
        p { "Select the category for the new round." }
        div class="d-flex flex-wrap gap-2" {
            (action(|_: &()|true,&format!("/tournaments/{tid}/rounds/in_round/create"),"Preliminary round",true).bind(&()))
            @for cat in &cats {
                (action(|_: &()|true,&format!("/tournaments/{tid}/rounds/{}/create",cat.id),&cat.name,true).bind(&()))
            }
        }
        @if cats.is_empty() { p class="mt-3" { "Set up break categories before creating elimination rounds." } }
    };
    form_page(
        query,
        "Create round",
        &format!("/tournaments/{tid}/rounds"),
        &tournament,
        user,
        body,
        false,
    )
}

fn validate_category(
    tid: &str,
    category_id: &str,
    conn: &mut diesel::SqliteConnection,
) -> Result<(), crate::util_resp::FailureResponse> {
    if category_id != "in_round" {
        break_categories::table
            .filter(break_categories::tournament_id.eq(tid))
            .filter(break_categories::id.eq(category_id))
            .select(break_categories::id)
            .first::<String>(conn)?;
    }
    Ok(())
}

pub async fn create_new_round_of_specific_category_page(
    Path((tid, category_id)): Path<(String, String)>,
    Query(query): Query<ModalQuery>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    validate_category(&tid, &category_id, &mut conn)?;
    let form = EditRoundForm::default();
    let path = format!("/tournaments/{tid}/rounds/{category_id}/create");
    form_page(
        query,
        "Create round",
        &format!("/tournaments/{tid}/rounds"),
        &tournament,
        user,
        round_form(&path, &form, None),
        false,
    )
}

pub async fn do_create_new_round_of_specific_category(
    Path((tid, category_id)): Path<(String, String)>,
    Query(query): Query<ModalQuery>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<EditRoundForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    validate_category(&tid, &category_id, &mut conn)?;
    let existing = TournamentRounds::fetch(&tid, &mut *conn)?;
    let seq = form.seq.parse::<u32>().ok();
    let mut error = if !(4..=32).contains(&form.name.len()) {
        Some("Round name must be between 4 and 32 characters.")
    } else if seq.is_none_or(|s| !(1..=100).contains(&s)) {
        Some("Sequence must be between 1 and 100.")
    } else if category_id == "in_round"
        && existing
            .elim
            .iter()
            .any(|r| r.seq <= i64::from(seq.unwrap()))
    {
        Some("Preliminary rounds must precede elimination rounds.")
    } else if category_id != "in_round"
        && existing
            .prelim
            .iter()
            .any(|r| r.seq >= i64::from(seq.unwrap()))
    {
        Some("Elimination rounds must follow preliminary rounds.")
    } else {
        None
    };
    if error.is_none() {
        match diesel::insert_into(rounds::table)
            .values((
                rounds::id.eq(uuid::Uuid::now_v7().to_string()),
                rounds::tournament_id.eq(&tid),
                rounds::seq.eq(i64::from(seq.unwrap())),
                rounds::name.eq(&form.name),
                rounds::kind.eq(if category_id == "in_round" {
                    "P"
                } else {
                    "E"
                }),
                rounds::break_category
                    .eq((category_id != "in_round").then_some(&category_id)),
                rounds::completed.eq(false),
            ))
            .execute(&mut *conn)
        {
            Ok(_) => {
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
    let path = format!("/tournaments/{tid}/rounds/{category_id}/create");
    form_page(
        query,
        "Create round",
        &format!("/tournaments/{tid}/rounds"),
        &tournament,
        user,
        round_form(&path, &form, error),
        true,
    )
}
