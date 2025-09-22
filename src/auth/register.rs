use argon2::Argon2;
use argon2::PasswordHasher;
use argon2::password_hash::SaltString;
use argon2::password_hash::rand_core::OsRng;
use chrono::Utc;
use diesel::{insert_into, prelude::*};
use hypertext::prelude::*;
use rocket::{FromForm, form::Form, get, post, response::Redirect};
use serde::Serialize;
use uuid::Uuid;

use crate::state::Conn;
use crate::util_resp::StandardResponse;
use crate::util_resp::bad_request;
use crate::util_resp::see_other_ok;
use crate::validation::*;
use crate::widgets::alert::ErrorAlert;
use crate::{auth::User, schema::users, template::Page};

#[get("/register")]
pub async fn register_page(user: Option<User<true>>) -> StandardResponse {
    if user.is_some() {
        // todo: flash message
        return bad_request(maud! {p {"You are already logged in!"}}.render());
    }

    bad_request(
        Page::new()
            .user_opt(user)
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
pub async fn do_register(
    user: Option<User<true>>,
    mut conn: Conn<true>,
    form: Form<RegisterForm<'_>>,
) -> StandardResponse {
    if user.is_some() {
        // todo: flash message
        return bad_request(maud! {p {"You are already logged in!"}}.render());
    }

    let existing = users::table
        .filter(
            users::username
                .eq(&form.username)
                .or(users::email.eq(&form.email)),
        )
        .first::<User<true>>(&mut *conn)
        .optional()
        .unwrap();

    match existing {
        Some(user) => {
            let is_email_problem = user.email == form.email;

            bad_request(
                Page::<_, true>::new()
                    .body(maud! {
                        ErrorAlert msg=(match is_email_problem {
                            true => "That email is already taken",
                            false => "That username is already taken"
                        }.to_string() +
                        ". Please return to the previous page and try again.");
                    })
                    .render(),
            )
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

            see_other_ok(Redirect::to("/user"))
        }
    }
}
