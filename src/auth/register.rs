use argon2::Argon2;
use argon2::PasswordHasher;
use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;
use chrono::Utc;
use diesel::{insert_into, prelude::*};
use hypertext::prelude::*;
use rocket::{FromForm, Responder, form::Form, get, post, response::Redirect};
use serde::Serialize;
use uuid::Uuid;

use crate::validation::*;
use crate::{auth::User, schema::users, state::LockedConn, template::Page};

#[derive(Responder)]
pub enum RegisterResponse {
    TryAgain(Rendered<String>),
    AlreadyLoggedIn(Redirect),
    Success(Redirect),
}

#[get("/register")]
pub async fn register_page(user: Option<User>) -> RegisterResponse {
    if user.is_some() {
        // todo: flash message
        return RegisterResponse::AlreadyLoggedIn(Redirect::to("/"));
    }

    RegisterResponse::TryAgain(
        Page::new()
            .body(maud! {
                h1 {"Register"}
                form method="post" class="mt-4" {
                    div class="mb-3" {
                        label for="username" class="form-label" { "Username" }
                        input type="text" class="form-control" id="username" name="username";
                    }
                    div class="mb-3" {
                        label for="email" class="form-label" { "Email" }
                        input type="email" class="form-control" id="email" name="email";
                    }
                    div class="mb-3" {
                        label for="password" class="form-label" { "Password" }
                        input type="password" class="form-control" id="password" name="password";
                    }
                    div class="mb-3" {
                        label for="password2" class="form-label" { "Confirm Password" }
                        input type="password" class="form-control" id="password2" name="password2";
                    }
                    button type="submit" class="btn btn-primary" { "Register" }
                }
            })
            .render(),
    )
}

#[derive(FromForm, Serialize)]
pub struct RegisterForm<'v> {
    #[field(validate = is_ascii_no_spaces())]
    pub username: &'v str,
    #[field(validate = is_valid_email())]
    pub(crate) email: &'v str,
    #[field(validate = len(6..))]
    #[field(validate = eq(self.password2))]
    pub(crate) password: &'v str,
    #[field(validate = len(6..))]
    pub(crate) password2: &'v str,
}

#[post("/register", data = "<form>")]
// todo: spawn_blocking for function?
pub async fn do_register(
    user: Option<User>,
    mut conn: LockedConn<'_>,
    form: Form<RegisterForm<'_>>,
) -> RegisterResponse {
    if user.is_some() {
        // todo: flash message
        return RegisterResponse::AlreadyLoggedIn(Redirect::to("/"));
    }

    let existing = users::table
        .filter(
            users::username
                .eq(&form.username)
                .or(users::email.eq(&form.email)),
        )
        .first::<User>(&mut *conn)
        .optional()
        .unwrap();

    match existing {
        Some(user) => {
            let is_email_problem = user.email == form.email;

            return RegisterResponse::TryAgain(
                Page::new()
                    .body(maud! {
                        div class="alert alert-danger" role="alert" {
                            @if is_email_problem {
                                "That email is already taken"
                            } @else {
                                "That username is already taken"
                            }

                            ". Please return to the previous page and try again."
                        }
                    })
                    .render(),
            );
        }
        None => {
            let salt = SaltString::generate(&mut OsRng);

            let argon2 = Argon2::default();

            let password_hash = argon2
                .hash_password(form.password.as_bytes(), &salt)
                .unwrap()
                .to_string();

            insert_into(users::table)
                .values((
                    users::id.eq(Uuid::now_v7().to_string()),
                    users::email.eq(form.email),
                    users::username.eq(form.username),
                    users::password_hash.eq(password_hash),
                    users::created_at.eq(Utc::now().naive_utc()),
                ))
                .execute(&mut *conn)
                .unwrap();

            RegisterResponse::Success(Redirect::to("/user"))
        }
    }
}
