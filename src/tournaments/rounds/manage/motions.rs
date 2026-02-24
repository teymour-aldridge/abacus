use axum::{extract::Path, response::Redirect};
use diesel::prelude::*;

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

pub async fn publish_motions(
    Path((tid, rid)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let round_id = rid.clone();
    match diesel::update(
        motions_of_round::table
            .filter(motions_of_round::round_id.eq(&round_id)),
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

    let round_seq: i64 = crate::schema::rounds::table
        .filter(rounds::id.eq(&rid))
        .select(rounds::seq)
        .first(&mut *conn)
        .unwrap();

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/rounds/{}/draw/manage",
        tid, round_seq
    )))
}
