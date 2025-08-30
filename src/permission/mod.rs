use diesel::prelude::*;
use diesel::select;
use tokio::task::spawn_blocking;

use rocket::{
    Request,
    http::Status,
    outcome::{Outcome, try_outcome},
    request::{self, FromRequest},
};

use crate::auth::UserNoTx;
use crate::tournaments::TournamentNoTx;
use crate::{
    auth::User,
    schema::tournament_members,
    state::{Conn, DbPool},
    tournaments::Tournament,
};

pub struct IsTabDirector;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for IsTabDirector {
    type Error = ();

    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, ()> {
        let tournament = match request.guard::<Tournament>().await {
            Outcome::Success(t) => t,
            Outcome::Error(e) => return Outcome::Error(e),
            Outcome::Forward(f) => return Outcome::Forward(f),
        };

        let user = match request.guard::<User>().await {
            Outcome::Success(t) => t,
            Outcome::Error((e, _)) => return Outcome::Error((e, ())),
            Outcome::Forward(f) => return Outcome::Forward(f),
        };

        let conn = try_outcome!(
            request.guard::<Conn>().await.map_error(|(t, _)| (t, ()))
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
            .get_result::<bool>(&mut *conn.get_sync())
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

/// The same as [`IsTabDirector`], except does not run in a transaction.
// TODO: we can probably use polymorphism to delete the NoTx types.
pub struct IsTabDirectorNoTx;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for IsTabDirectorNoTx {
    type Error = ();

    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, ()> {
        let tournament = match request.guard::<TournamentNoTx>().await {
            Outcome::Success(t) => t,
            Outcome::Error(e) => return Outcome::Error(e),
            Outcome::Forward(f) => return Outcome::Forward(f),
        };

        let user = match request.guard::<UserNoTx>().await {
            Outcome::Success(t) => t,
            Outcome::Error((e, _)) => return Outcome::Error((e, ())),
            Outcome::Forward(f) => return Outcome::Forward(f),
        };

        let pool: DbPool = try_outcome!(
            request
                .guard::<&rocket::State<DbPool>>()
                .await
                .map_error(|(t, _)| (t, ()))
        )
        .inner()
        .clone();

        let has_permission = spawn_blocking(move || {
            let mut conn = pool.get().unwrap();

            select(diesel::dsl::exists(
                tournament_members::table.filter(
                    tournament_members::user_id
                        .eq(user.0.id)
                        .and(
                            tournament_members::tournament_id
                                .eq(tournament.0.id),
                        )
                        .and(tournament_members::is_superuser.eq(true)),
                ),
            ))
            .get_result::<bool>(&mut conn)
            .unwrap()
        })
        .await
        .unwrap();

        if has_permission {
            Outcome::Success(IsTabDirectorNoTx)
        } else {
            Outcome::Error((Status::Forbidden, ()))
        }
    }
}
