use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::{Form, Query},
    response::Redirect,
};
use axum_extra::extract::{PrivateCookieJar, cookie::Key};
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;
use tokio::task::spawn_blocking;
use tracing::{Instrument, Level};
use url::Url;

use crate::{
    auth::{User, set_login_cookie},
    schema::users,
    state::Conn,
    template::Page,
    util_resp::{FailureResponse, StandardResponse, bad_request, see_other_ok},
    // widgets::alert::ErrorAlert,
};

pub async fn login_page(user: Option<User<true>>) -> StandardResponse {
    if user.is_some() {
        return Err(FailureResponse::BadRequest(
            Page::new()
                .user_opt(user)
                .body(maud! {
                    div class="alert alert-danger" {
                         "You are already logged in, so cannot log in!"
                    }
                })
                .render(),
        ));
    }

    bad_request(Page::new().user_opt(user).body(maud! {
        div class="container-fluid p-3" {
            form method="post" {
                div class="form-group" {
                    label for="email" { "Email or username" }
                    input type="text" class="form-control" id="email" name="id" placeholder="Enter email or username";
                }
                div class="form-group" {
                    label for="password" { "Password" }
                    input type="password" class="form-control" id="password" name="password" placeholder="Password";
                }
                button type="submit" class="btn btn-primary" { "Submit" }
            }
        }
    }).render())
}

#[derive(Deserialize)]
pub struct LoginForm {
    id: String,
    password: String,
}

#[derive(Deserialize)]
pub struct NextParams {
    next: Option<String>,
}

#[tracing::instrument(skip(conn, params, jar, form))]
pub async fn do_login(
    user: Option<User<true>>,
    Query(params): Query<NextParams>,
    mut conn: Conn<true>,
    jar: PrivateCookieJar<Key>,
    // note: MUST be skipped in tracing for security reasons!
    Form(form): Form<LoginForm>,
) -> (PrivateCookieJar<Key>, StandardResponse) {
    let next = params.next.as_deref();

    let span = tracing::span!(Level::TRACE, "retrieve_user");
    let guard = span.enter();
    let user1 =
        match users::table
            .filter(users::email.eq(&form.id).or(users::username.eq(&form.id)))
            .first::<User<true>>(&mut *conn)
            .optional()
            .unwrap()
        {
            Some(user) => user,
            None => return (jar, Err(FailureResponse::BadRequest(
                Page::new()
                    .user_opt(user)
                    .body(maud! {
                        div class="alert alert-danger" {
                             "No such user exists. Please return to the previous page and try again."
                        }
                    })
                    .render(),
             ))),
        };
    drop(guard);

    // todo: invert this (this flow is confusing)
    let wrong_password = {
        let hash = user1.password_hash.clone();
        let r = spawn_blocking(move || {
            if !cfg!(fuzzing) {
                let parsed_hash = PasswordHash::new(&hash).unwrap();

                Argon2::default()
                    .verify_password(form.password.as_bytes(), &parsed_hash)
                    .is_err()
            } else {
                tracing::error!(
                    "Not using password hashing. This should never
                    occur in a release build (hashing is disabled to speed up
                    the process of fuzzing)."
                );
                // disable hashing to speed up fuzzing process
                hash != form.password
            }
        })
        .instrument(tracing::span!(
            tracing::Level::DEBUG,
            "password_verification"
        ))
        .await
        .unwrap();

        r
    };

    if wrong_password {
        // todo: password rate limiting
        return (jar, bad_request(
            Page::new()
                .user_opt(user)
                .body(maud! {
                    div class="alert alert-danger" {
                        "Incorrect password. Please return to the previous page and try again."
                    }
                })
                .render(),
        ));
    }

    let jar = set_login_cookie(user1.id, jar);

    (
        jar,
        see_other_ok({
            let redirect_to = if let Some(url) =
                next.and_then(|url| url.parse::<Url>().ok())
            {
                url.path().to_string()
            } else {
                "/".to_string()
            };

            Redirect::to(&redirect_to)
        }),
    )
}
