use argon2::{Argon2, PasswordHash, PasswordVerifier};
use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::{
    FromForm, form::Form, get, http::CookieJar, post, response::Redirect,
};
use url::Url;

use crate::{
    auth::{User, set_login_cookie},
    schema::users,
    state::LockedConn,
    template::Page,
    util_resp::GenerallyUsefulResponse,
    widgets::alert::ErrorAlert,
};

#[get("/login")]
pub async fn login_page(user: Option<User>) -> GenerallyUsefulResponse {
    if user.is_some() {
        return GenerallyUsefulResponse::BadRequest(
            Page::new()
                .user_opt(user)
                .body(maud! {
                    ErrorAlert
                        msg = "You are already logged in, so cannot log in!";
                })
                .render(),
        );
    }

    GenerallyUsefulResponse::BadRequest(Page::new().user_opt(user).body(maud! {
        form method="post" {
            div class="form-group" {
                label for="email" { "Email address" }
                input type="email" class="form-control" id="email" name="email" placeholder="Enter email";
            }
            div class="form-group" {
                label for="password" { "Password" }
                input type="password" class="form-control" id="password" name="password" placeholder="Password";
            }
            button type="submit" class="btn btn-primary" { "Submit" }
        }
    }).render())
}

#[derive(FromForm)]
pub struct LoginForm {
    id: String,
    password: String,
}

#[post("/login?<next>", data = "<form>")]
pub async fn do_login(
    user: Option<User>,
    next: Option<&str>,
    mut conn: LockedConn<'_>,
    form: Form<LoginForm>,
    jar: &CookieJar<'_>,
) -> GenerallyUsefulResponse {
    let user1 =
        match users::table
            .filter(users::email.eq(&form.id).or(users::username.eq(&form.id)))
            .first::<User>(&mut *conn)
            .optional()
            .unwrap()
        {
            Some(user) => user,
            None => return GenerallyUsefulResponse::BadRequest(
                Page::new()
                    .user_opt(user)
                    .body(maud! {
                        ErrorAlert
                            msg =  "No such user exists. Please return to the
                                    previous page and try again.";
                    })
                    .render(),
            ),
        };

    let parsed_hash = PasswordHash::new(&user1.password_hash).unwrap();
    if Argon2::default()
        .verify_password(form.password.as_bytes(), &parsed_hash).is_err()
    {
        // todo: password rate limiting
        return GenerallyUsefulResponse::BadRequest(
            Page::new()
                .user_opt(user)
                .body(maud! {
                    ErrorAlert msg =
                        "Incorrect password. Please return to the previous page
                         and try again.";
                })
                .render(),
        );
    }

    set_login_cookie(user1.id, jar);

    GenerallyUsefulResponse::Success({
        let redirect_to = if let Some(url) =
            next.and_then(|url| url.parse::<Url>().ok())
        {
            url.path().to_string()
        } else {
            "/".to_string()
        };

        Redirect::to(redirect_to)
    })
}
