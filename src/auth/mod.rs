use axum::{
    async_trait,
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::{PrivateCookieJar, cookie::Key};
use chrono::{Days, NaiveDateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    schema::{self},
    state::{DbPool, ThreadSafeConn},
};

pub mod login;
pub mod register;

pub const LOGIN_COOKIE: &str = "jeremy_bearimy";

#[derive(Debug, Queryable, Serialize, Deserialize, Clone)]
pub struct User<const TX: bool> {
    pub id: String,
    pub email: String,
    pub username: String,
    pub password_hash: String,
    pub created_at: NaiveDateTime,
}

impl<const TX: bool> User<TX> {
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

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            AuthError::CookieMissingOrMalformed => {
                (StatusCode::UNAUTHORIZED, "Cookie missing or malformed")
            }
            AuthError::NoDatabase => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error")
            }
            AuthError::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "Unauthorized")
            }
        };
        (status, body).into_response()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct LoginSession {
    id: String,
    expiry: NaiveDateTime,
}

#[async_trait]
impl<const TX: bool, S> FromRequestParts<S> for User<TX>
where
    S: Send + Sync,
    DbPool: FromRef<S>,
    axum_extra::extract::cookie::Key: FromRef<S>,
{
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        // Extract connection
        let conn_wrapper =
            ThreadSafeConn::<TX>::from_request_parts(parts, state)
                .await
                .map_err(|_| AuthError::NoDatabase)?;

        let mut conn = conn_wrapper
            .inner
            .try_lock()
            .map_err(|_| AuthError::NoDatabase)?;

        // Extract cookies
        let jar: PrivateCookieJar<Key> =
            PrivateCookieJar::from_request_parts(parts, state)
                .await
                .map_err(|_| AuthError::CookieMissingOrMalformed)?;

        let login_cookie = match jar.get(LOGIN_COOKIE) {
            Some(cookie) => cookie,
            None => return Err(AuthError::Unauthorized),
        };

        let login: LoginSession =
            match serde_json::from_str::<LoginSession>(login_cookie.value()) {
                Ok(t) if chrono::Utc::now().naive_utc() < t.expiry => t,
                _ => {
                    // If invalid, we should remove the cookie.
                    // But from_request_parts cannot easily modify response cookies directly unless we return a specific rejection that does so.
                    // Or we can return AuthError which doesn't remove the cookie but fails.
                    // The next request will fail again.
                    // Ideally we remove it.
                    // Axum's CookieJar is mostly for reading. `PrivateCookieJar` allows modifying too but the changes need to be returned in the response.
                    // Since this is an extractor, we can't effect the response unless we fail?
                    // Actually `PrivateCookieJar` updates are handled if the *jar* is returned.
                    // But we are not returning the jar here.
                    // For now, just fail. The user might get Stuck if we don't clear it?
                    // Rocket code did: `request.cookies().remove_private(LOGIN_COOKIE)`.
                    // In Axum, dealing with cookie removal on auth failure in extractor is tricky.
                    return Err(AuthError::Unauthorized);
                }
            };

        // If conn is held by MutexGuard, it's Option<PooledConnection>
        let user = if let Some(conn) = conn.as_mut() {
            schema::users::table
                .filter(schema::users::id.eq(login.id))
                .first(conn)
                .optional()
                .map_err(|_| AuthError::NoDatabase)?
        } else {
            return Err(AuthError::NoDatabase);
        };

        match user {
            Some(user) => Ok(user),
            None => Err(AuthError::Unauthorized),
        }
    }
}

pub fn set_login_cookie(id: String, jar: PrivateCookieJar) -> PrivateCookieJar {
    jar.add(Cookie::new(
        LOGIN_COOKIE,
        serde_json::to_string(&LoginSession {
            id,
            expiry: Utc::now()
                .naive_utc()
                .checked_add_days(Days::new(7))
                .unwrap(),
        })
        .unwrap(),
    ))
}
