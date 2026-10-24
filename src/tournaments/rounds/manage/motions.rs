use axum::{
    Form,
    extract::{Path, Query},
};
use diesel::prelude::*;
use serde::Deserialize;

use crate::{
    auth::User,
    schema::{motions_of_round, rounds},
    state::Conn,
    tournaments::Tournament,
    ui::response::{ModalQuery, after_edit, form_page},
    util_resp::StandardResponse,
};
use hypertext::prelude::*;

#[derive(Deserialize, Default)]
pub struct CreateMotionForm {
    motion: String,
    infoslide: Option<String>,
}

fn motion_form<'a>(
    action: &'a str,
    form: &'a CreateMotionForm,
    error: Option<&'a str>,
) -> impl Renderable + 'a {
    maud! {
        @if let Some(error) = error { div class="alert alert-danger" role="alert" { (error) } }
        form method="post" action=(action) hx-post=(format!("{action}?modal=true")) hx-target="#ui-modal" hx-swap="innerHTML" {
            label class="form-label" for="motion-text" { "Motion" }
            textarea class="form-control mb-3" id="motion-text" name="motion" required { (form.motion) }
            label class="form-label" for="motion-infoslide" { "Infoslide" }
            textarea class="form-control mb-3" id="motion-infoslide" name="infoslide" { (form.infoslide.as_deref().unwrap_or("")) }
            button class="btn btn-primary" type="submit" { "Add unpublished motion" }
        }
    }
}

pub async fn create_motion_page(
    Path((tid, rid)): Path<(String, String)>,
    Query(query): Query<ModalQuery>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let round =
        crate::tournaments::rounds::Round::fetch(&tid, &rid, &mut *conn)?;
    let form = CreateMotionForm::default();
    let action = format!("/tournaments/{tid}/rounds/{rid}/motions/create");
    form_page(
        query,
        "Add motion",
        &format!("/tournaments/{tid}/rounds/{}", round.seq),
        &tournament,
        user,
        motion_form(&action, &form, None),
        false,
    )
}

pub async fn create_motion(
    Path((tid, rid)): Path<(String, String)>,
    Query(query): Query<ModalQuery>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateMotionForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let round =
        crate::tournaments::rounds::Round::fetch(&tid, &rid, &mut *conn)?;
    let return_url = format!("/tournaments/{tid}/rounds/{}", round.seq);
    let motion = form.motion.trim();
    if motion.is_empty() {
        let action = format!("/tournaments/{tid}/rounds/{rid}/motions/create");
        return form_page(
            query,
            "Add motion",
            &return_url,
            &tournament,
            user,
            motion_form(&action, &form, Some("Motion must not be empty.")),
            true,
        );
    }
    let infoslide = form
        .infoslide
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    diesel::insert_into(motions_of_round::table)
        .values((
            motions_of_round::id.eq(uuid::Uuid::now_v7().to_string()),
            motions_of_round::tournament_id.eq(&tid),
            motions_of_round::round_id.eq(&rid),
            motions_of_round::motion.eq(motion),
            motions_of_round::infoslide.eq(infoslide),
            motions_of_round::published_at.eq(None::<chrono::NaiveDateTime>),
        ))
        .execute(&mut *conn)?;
    after_edit(query, return_url)
}

pub async fn publish_motions(
    Query(query): Query<ModalQuery>,
    Path((tid, rid)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round_seq: i64 = rounds::table
        .filter(rounds::id.eq(&rid))
        .filter(rounds::tournament_id.eq(&tid))
        .select(rounds::seq)
        .first(&mut *conn)?;

    diesel::update(
        motions_of_round::table
            .filter(motions_of_round::tournament_id.eq(&tid))
            .filter(motions_of_round::round_id.eq(&rid)),
    )
    .set(motions_of_round::published_at.eq(diesel::dsl::now))
    .execute(&mut *conn)?;

    after_edit(query, format!("/tournaments/{tid}/rounds/{round_seq}"))
}
