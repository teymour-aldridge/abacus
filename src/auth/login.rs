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
    auth::{User, clear_login_cookie, set_login_cookie},
    schema::users,
    state::Conn,
    template::Page,
    util_resp::{StandardResponse, bad_request, see_other_ok, success},
};

fn login_form<'a>(id: &'a str, error: Option<&'a str>) -> impl Renderable + 'a {
    maud! {
        div class="container py-4" {
            h1 { "Sign in" }
            @if let Some(error)=error { div class="alert alert-danger" role="alert" { (error) } }
            form method="post" class="mt-4" {
                div class="mb-3" {
                    label for="email" class="form-label" { "Email or username" }
                    input type="text" class="form-control" id="email" name="id" value=(id) autocomplete="username" required;
                }
                div class="mb-3" {
                    label for="password" class="form-label" { "Password" }
                    input type="password" class="form-control" id="password" name="password" autocomplete="current-password" required;
                }
                button type="submit" class="btn btn-primary" { "Sign in" }
            }
        }
    }
}
fn invalid_login(user: Option<User<true>>, id: &str) -> StandardResponse {
    bad_request(
        Page::new()
            .user_opt(user)
            .body(login_form(
                id,
                Some("The username or password was not recognised."),
            ))
            .render(),
    )
}
pub async fn login_page(user: Option<User<true>>) -> StandardResponse {
    if user.is_some() {
        return see_other_ok(Redirect::to("/"));
    }
    success(
        Page::new()
            .user_opt(user)
            .body(login_form("", None))
            .render(),
    )
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

#[tracing::instrument(skip(user, conn, params, jar, form))]
pub async fn do_login(
    user: Option<User<true>>,
    Query(params): Query<NextParams>,
    mut conn: Conn<true>,
    jar: PrivateCookieJar<Key>,
    // note: MUST be skipped in tracing for security reasons!
    Form(form): Form<LoginForm>,
) -> (PrivateCookieJar<Key>, StandardResponse) {
    if user.is_some() {
        return (jar, see_other_ok(Redirect::to("/")));
    }
    let next = params.next.as_deref();
    let login_id = form.id.as_str();

    let span = tracing::span!(Level::TRACE, "retrieve_user");
    let guard = span.enter();
    let user1 = match users::table
        .filter(users::email.eq(&form.id).or(users::username.eq(&form.id)))
        .first::<User<true>>(&mut *conn)
        .optional()
    {
        Ok(Some(user)) => user,
        Ok(None) => return (jar, invalid_login(user, login_id)),
        Err(error) => return (jar, Err(error.into())),
    };
    drop(guard);

    // todo: invert this (this flow is confusing)
    let wrong_password = {
        let hash = user1.password_hash.clone();
        let r = spawn_blocking(move || {
            if !cfg!(test) {
                let Ok(parsed_hash) = PasswordHash::new(&hash) else {
                    return true;
                };

                Argon2::default()
                    .verify_password(form.password.as_bytes(), &parsed_hash)
                    .is_err()
            } else {
                tracing::error!(
                    "Not using password hashing. This should never
                    occur in a release build (hashing is disabled to speed up
                    the test workload)."
                );
                // disable hashing to speed up the test workload
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
        return (jar, invalid_login(user, login_id));
    }

    let jar = set_login_cookie(user1.id, jar);

    (
        jar,
        see_other_ok({
            let redirect_to = local_redirect(next);

            Redirect::to(&redirect_to)
        }),
    )
}

fn local_redirect(next: Option<&str>) -> String {
    fn local(path: &str) -> bool {
        path.starts_with('/')
            && !path.starts_with("//")
            && !path.contains('\\')
            && !path.chars().any(|c| c.is_ascii_control())
    }
    next.filter(|next| local(next))
        .map(str::to_owned)
        .or_else(|| {
            next.and_then(|next| next.parse::<Url>().ok())
                .map(|url| url.path().to_owned())
                .filter(|path| local(path))
        })
        .unwrap_or_else(|| "/".to_owned())
}

pub async fn do_logout(
    jar: PrivateCookieJar<Key>,
) -> (PrivateCookieJar<Key>, StandardResponse) {
    let jar = clear_login_cookie(jar);
    (jar, see_other_ok(Redirect::to("/")))
}
