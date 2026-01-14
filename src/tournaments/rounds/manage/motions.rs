use axum::{extract::Path, response::Redirect};
use diesel::prelude::*;

use crate::{
    auth::User,
    schema::{tournament_round_motions, tournament_rounds},
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
        tournament_round_motions::table
            .filter(tournament_round_motions::round_id.eq(&round_id)),
    )
    .set(tournament_round_motions::published_at.eq(diesel::dsl::now))
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

    let round_seq: i64 = crate::schema::tournament_rounds::table
        .filter(tournament_rounds::id.eq(&rid))
        .select(tournament_rounds::seq)
        .first(&mut *conn)
        .unwrap();

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/rounds/{}/draw/manage",
        tid, round_seq
    )))
}
