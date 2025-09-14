use diesel::prelude::*;
use diesel::select;
use tokio::task::spawn_blocking;

use rocket::{
    Request,
    http::Status,
    outcome::{Outcome, try_outcome},
    request::{self, FromRequest},
};

use crate::state::Conn;
use crate::{auth::User, schema::tournament_members, tournaments::Tournament};

pub struct IsTabDirector<const TX: bool>;

#[rocket::async_trait]
impl<'r, const TX: bool> FromRequest<'r> for IsTabDirector<TX> {
    type Error = ();

    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, ()> {
        let tournament = match request.guard::<Tournament>().await {
            Outcome::Success(t) => t,
            Outcome::Error(e) => return Outcome::Error(e),
            Outcome::Forward(f) => return Outcome::Forward(f),
        };

        let user = match request.guard::<User<TX>>().await {
            Outcome::Success(t) => t,
            Outcome::Error((e, _)) => return Outcome::Error((e, ())),
            Outcome::Forward(f) => return Outcome::Forward(f),
        };

        let mut conn = try_outcome!(
            request
                .guard::<Conn<TX>>()
                .await
                .map_error(|(t, _)| (t, ()))
        );

        let has_permission = spawn_blocking(move || {
            select(diesel::dsl::exists(
                tournament_members::table.filter(
                    tournament_members::user_id
                        .eq(user.id)
                        .and(
                            tournament_members::tournament_id.eq(tournament.id),
                        )
                        .and(tournament_members::is_superuser.eq(true)),
                ),
            ))
            .get_result::<bool>(&mut *conn)
            .unwrap()
        })
        .await
        .unwrap();

        if has_permission {
            Outcome::Success(IsTabDirector)
        } else {
            Outcome::Error((Status::Forbidden, ()))
        }
    }
}
