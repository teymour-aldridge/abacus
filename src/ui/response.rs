use crate::{
    auth::User,
    template::Page,
    tournaments::Tournament,
    util_resp::{
        StandardResponse, bad_request, hx_redirect, see_other_ok, success,
    },
};
use hypertext::prelude::*;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize, Clone, Copy)]
pub struct ModalQuery {
    #[serde(default)]
    pub modal: bool,
}

pub fn form_page<const TX: bool>(
    query: ModalQuery,
    title: &str,
    return_url: &str,
    tournament: &Tournament,
    user: User<TX>,
    body: impl Renderable,
    invalid: bool,
) -> StandardResponse {
    if query.modal {
        success(super::presentation::modal(title, return_url, body).render())
    } else {
        let page = Page::new()
            .tournament(tournament.clone())
            .user(user)
            .body(maud! { div class="container py-4" {
                h1 { (title) } (body)
                a href=(return_url) { "Return" }
            } })
            .render();
        if invalid {
            bad_request(page)
        } else {
            success(page)
        }
    }
}
pub fn after_edit(query: ModalQuery, url: String) -> StandardResponse {
    if query.modal {
        hx_redirect(url)
    } else {
        see_other_ok(axum::response::Redirect::to(&url))
    }
}

pub async fn private_ui_response(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let asset = request.uri().path() == "/style.css"
        || request.uri().path().starts_with("/fonts/");
    let mut response = next.run(request).await;
    if !asset {
        use axum::http::{HeaderValue, header};
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        let varies = response
            .headers()
            .get_all(header::VARY)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect::<Vec<_>>()
            .join(", ");
        if !varies.split(',').any(|value| {
            value.trim().eq_ignore_ascii_case("HX-Request")
                || value.trim() == "*"
        }) {
            response
                .headers_mut()
                .append(header::VARY, HeaderValue::from_static("HX-Request"));
        }
    }
    response
}
