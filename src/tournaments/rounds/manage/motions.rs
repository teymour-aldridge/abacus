use axum::{Form, extract::Path, response::Redirect};
use diesel::prelude::*;
use serde::Deserialize;

use crate::{
    auth::User,
    schema::{motions_of_round, rounds},
    state::Conn,
    template::Page,
    tournaments::Tournament,
    util_resp::{StandardResponse, bad_request, see_other_ok},
    widgets::alert::ErrorAlert,
};
use hypertext::{Renderable, maud};

#[derive(Deserialize)]
pub struct CreateMotionForm {
    motion: String,
    infoslide: Option<String>,
}

pub async fn create_motion(
    Path((tid, rid)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateMotionForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let motion = form.motion.trim();
    if motion.is_empty() {
        return bad_request(maud! { "Motion must not be empty." }.render());
    }

    let infoslide = form
        .infoslide
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let round_seq: i64 = rounds::table
        .filter(rounds::id.eq(&rid))
        .filter(rounds::tournament_id.eq(&tid))
        .select(rounds::seq)
        .first(&mut *conn)?;

    diesel::insert_into(motions_of_round::table)
        .values((
            motions_of_round::id.eq(uuid::Uuid::now_v7().to_string()),
            motions_of_round::tournament_id.eq(&tid),
            motions_of_round::round_id.eq(&rid),
            motions_of_round::motion.eq(motion),
            motions_of_round::infoslide.eq(infoslide),
            motions_of_round::published_at.eq(None::<chrono::NaiveDateTime>),
        ))
        .execute(&mut *conn)
        .unwrap();

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/rounds/{}/briefing",
        tid, round_seq
    )))
}

pub async fn publish_motions(
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

    match diesel::update(
        motions_of_round::table
            .filter(motions_of_round::tournament_id.eq(&tid))
            .filter(motions_of_round::round_id.eq(&rid)),
    )
    .set(motions_of_round::published_at.eq(diesel::dsl::now))
    .execute(&mut *conn)
    {
        Ok(_) => (),
        Err(e) => {
            return bad_request(
                Page::new()
                    .user(user)
                    .body(maud! {
                        ErrorAlert msg = (format!("Failed to publish motions: {}", e));
                    })
                    .render(),
            )
        }
    }

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/rounds/{}/draw/manage",
        tid, round_seq
    )))
}
