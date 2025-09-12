use chrono::{Days, NaiveDateTime, Utc};
use diesel::prelude::*;
use rocket::{
    Request, State,
    http::{Cookie, CookieJar, Status},
    outcome::try_outcome,
    request::{self, FromRequest},
};
use serde::{Deserialize, Serialize};
use tokio::task::spawn_blocking;

use crate::{
    schema::{self},
    state::{Conn, DbPool},
};

pub mod login;
pub mod register;

pub const LOGIN_COOKIE: &str = "jeremy_bearimy";

#[derive(Debug, Queryable, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: String,
    pub email: String,
    pub username: String,
    pub password_hash: String,
    pub created_at: NaiveDateTime,
}

impl User {
    pub fn validate_username(username: &str) -> bool {
        (username.chars().count() > 3)
            && username.chars().all(|c| c.is_ascii() && c.is_alphabetic())
    }

    pub fn validate_password(password: &str) -> bool {
        password.len() > 6
    }
}

#[derive(Debug)]
pub enum AuthError {
    CookieMissingOrMalformed,
    NoDatabase,
    Unauthorized,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct LoginSession {
    id: String,
    expiry: NaiveDateTime,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for User {
    type Error = AuthError;

    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, AuthError> {
        let conn = try_outcome!(
            request
                .guard::<Conn>()
                .await
                .map_error(|(t, _)| (t, AuthError::NoDatabase))
        );

        let mut conn = conn.get().await;

        let login_cookie = match request.cookies().get_private(LOGIN_COOKIE) {
            Some(cookie) => cookie,
            None => {
                return rocket::request::Outcome::Forward(Status::Unauthorized);
            }
        };

        let login: LoginSession =
            match serde_json::from_str::<LoginSession>(login_cookie.value()) {
                Ok(t) if chrono::Utc::now().naive_utc() < t.expiry => t,
                Err(_) | Ok(_) => {
                    // TODO: log the error so that these can be easily resolved

                    // we need to remove cookie if incorrectly formatted, as they
                    // will otherwise persist and prevent the user from logging in
                    request.cookies().remove_private(LOGIN_COOKIE);
                    return rocket::request::Outcome::Forward(
                        Status::Unauthorized,
                    );
                }
            };

        let user: Option<User> = match schema::users::table
            .filter(schema::users::id.eq(login.id))
            .first(&mut *conn)
            .optional()
        {
            Ok(Some(user)) => Some(user),
            Ok(None) => None,
            Err(_) => {
                return rocket::request::Outcome::Error((
                    Status::InternalServerError,
                    AuthError::NoDatabase,
                ));
            }
        };

        match user {
            Some(user) => return rocket::request::Outcome::Success(user),
            None => {
                return rocket::request::Outcome::Error((
                    Status::Unauthorized,
                    AuthError::Unauthorized,
                ));
            }
        }
    }
}

pub struct UserNoTx(pub User);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for UserNoTx {
    type Error = AuthError;

    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, AuthError> {
        let pool = try_outcome!(
            request
                .guard::<&State<DbPool>>()
                .await
                .map_error(|(t, _)| (t, AuthError::NoDatabase))
        );
        let pool: DbPool = pool.inner().clone();

        let login_cookie = match request.cookies().get_private(LOGIN_COOKIE) {
            Some(cookie) => cookie,
            None => {
                return rocket::request::Outcome::Forward(Status::Unauthorized);
            }
        };

        let login: LoginSession =
            match serde_json::from_str::<LoginSession>(login_cookie.value()) {
                Ok(t) if chrono::Utc::now().naive_utc() < t.expiry => t,
                Err(_) | Ok(_) => {
                    // TODO: log the error so that these can be easily resolved

                    // we need to remove cookie if incorrectly formatted, as they
                    // will otherwise persist and prevent the user from logging in
                    request.cookies().remove_private(LOGIN_COOKIE);
                    return rocket::request::Outcome::Forward(
                        Status::Unauthorized,
                    );
                }
            };

        spawn_blocking(move || {
            let mut conn = pool.get().unwrap();

            let user: Option<User> = match schema::users::table
                .filter(schema::users::id.eq(login.id))
                .first(&mut conn)
                .optional()
            {
                Ok(Some(user)) => Some(user),
                Ok(None) => None,
                Err(_) => {
                    return rocket::request::Outcome::Error((
                        Status::InternalServerError,
                        AuthError::NoDatabase,
                    ));
                }
            };

            match user {
                Some(user) => rocket::request::Outcome::Success(UserNoTx(user)),
                None => rocket::request::Outcome::Error((
                    Status::Unauthorized,
                    AuthError::Unauthorized,
                )),
            }
        })
        .await
        .unwrap()
    }
}

pub fn set_login_cookie(id: String, jar: &CookieJar) {
    jar.add_private({
        Cookie::new(
            LOGIN_COOKIE,
            serde_json::to_string(&LoginSession {
                id,
                expiry: Utc::now()
                    .naive_utc()
                    .checked_add_days(Days::new(7))
                    .unwrap(),
            })
            .unwrap(),
        )
    });
}
