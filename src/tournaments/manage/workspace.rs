use crate::ui::View;
use crate::{
    auth::User,
    permission::TournamentAccess,
    template::Page,
    tournaments::Tournament,
    util_resp::{StandardResponse, success},
};
use hypertext::prelude::*;
use serde::Deserialize;
#[derive(Default, Deserialize, Clone, Copy)]
pub struct WorkspaceQuery {
    #[serde(default)]
    pub modal: bool,
    #[serde(default)]
    pub edit: bool,
}
impl WorkspaceQuery {
    pub fn editing(self) -> bool {
        self.modal || self.edit
    }
    pub fn form(self) -> crate::ui::response::ModalQuery {
        crate::ui::response::ModalQuery { modal: self.modal }
    }
}
pub fn unavailable(
    tournament: &Tournament,
    user: Option<User<true>>,
    title: &str,
) -> StandardResponse {
    success(Page::new().user_opt(user).tournament(tournament.clone()).body(maud! {
        h1 { (title) }
        p { "This operational information is restricted to tournament organisers." }
        a href=(format!("/tournaments/{}", tournament.id)) { "Tournament overview" }
    }).render())
}
pub fn read_page(
    tournament: &Tournament,
    user: User<true>,
    title: &str,
    url: &str,
    access: &TournamentAccess,
    body: impl Renderable,
) -> StandardResponse {
    let edit_url = format!("{url}?edit=true");
    success(Page::new().user(user).tournament(tournament.clone()).body(maud! {
        h1 { (title) }
        (crate::ui::presentation::restricted::<TournamentAccess>("Tournament organisers").bind(access))
        (body)
        (crate::ui::presentation::action(|a: &TournamentAccess| a.is_superuser(), &edit_url, "Edit", true).bind(access))
        a class="btn btn-link" href=(format!("/tournaments/{}", tournament.id)) { "Tournament overview" }
    }).render())
}
pub fn after_redirect(
    query: crate::ui::response::ModalQuery,
    redirect: axum::response::Redirect,
) -> StandardResponse {
    use axum::response::IntoResponse;
    if query.modal {
        let response = redirect.into_response();
        let location = response
            .headers()
            .get(axum::http::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("/")
            .to_owned();
        crate::util_resp::hx_redirect(location)
    } else {
        crate::util_resp::see_other_ok(redirect)
    }
}
pub fn fields<S: serde::Serialize>(
    submitted: &S,
    categories: &[(String, String)],
) -> impl Renderable + use<S> {
    let values =
        serde_json::to_value(submitted).expect("form fields serialize");
    let values = values.as_object().expect("form is an object").clone();
    let categories = categories.to_vec();
    maud! {
        @for (name, value) in &values {
            @let label = match name.as_str() {
                "name" => "Name", "private_name"=>"Internal name", "public_name"=>"Public name", "description"=>"Description", "slug" => "URL name", "seq" => "Display order", "priority" => "Priority",
                "break_size" => "Teams advancing", "reserve_size" => "Reserve teams", "limit_" => "Public limit",
                "public" => "Publish category", "child_category_id" => "Category", "parent_category_id" => "Counts as",
                "threshold_type" => "Eligibility condition", "count" => "Qualifying speakers required", "minus" => "Non-qualifying speakers allowed",
                "category_ids" => "Qualifying speaker categories", "allowed_category_ids" => "Categories allowed by this exception", "remark" => "Reason", _ => "Value",
            };
            @if matches!(name.as_str(), "team_id"|"team"|"judge") {
                input type="hidden" name=(name) value=(value.as_str().unwrap_or(""));
            } @else if name == "available" {
                label class="form-check-label d-block my-2" {
                    input type="checkbox" class="form-check-input" name="available" value=(value.as_str().unwrap_or("true")) checked[value.as_bool().unwrap_or(!value.is_null())]; " Available"
                }
            } @else if name == "check" {
                label class="form-label d-block" { "Check in or out"
                    select class="form-select" name="check" {
                        option value="in" selected[value.as_str()==Some("in")] { "Check in" }
                        option value="out" selected[value.as_str()==Some("out")] { "Check out" }
                    }
                }
            } @else if name == "public" {
                label class="form-check-label d-block my-2" {
                    input type="checkbox" class="form-check-input" name=(name) checked[!value.is_null()] value="true"; " " (label)
                }
            } @else if name == "threshold_type" {
                label class="form-label d-block" { (label)
                    select class="form-select" name=(name) {
                        @for (option,text) in [("everyone","Every team"),("all","Every speaker qualifies"),("at_least","A minimum number of speakers qualify"),("at_least_minus","A maximum number of speakers do not qualify")] {
                            option value=(option) selected[value.as_str()==Some(option)] { (text) }
                        }
                    }
                }
            } @else if name.ends_with("category_id") || name.ends_with("category_ids") {
                @let multiple = name.ends_with("category_ids");
                label class="form-label d-block" { (label)
                    select class="form-select" name=(name) multiple[multiple] required[!multiple] {
                        @if !multiple { option value="" selected[value.as_str().is_none_or(|v| v.is_empty())] disabled { "Choose category" } }
                        @for (id, title) in &categories {
                            option value=(id) selected[value.as_str()==Some(id.as_str()) || value.as_array().is_some_and(|v| v.iter().any(|v| v.as_str()==Some(id.as_str())))] { (title) }
                        }
                    }
                }
            } @else {
                @let numeric = matches!(name.as_str(), "seq"|"priority"|"break_size"|"reserve_size"|"limit_"|"count"|"minus");
                label class="form-label d-block" { (label)
                    input class="form-control" type=(if numeric && (value.is_number() || value.as_str().is_some_and(|s| s.parse::<i64>().is_ok())) {"number"}else{"text"}) name=(name)
                        value=(if value.is_null() { String::new() } else { value.as_str().map(str::to_owned).unwrap_or_else(|| value.to_string()) });
                }
            }
        }
    }
}
pub fn submitted_form<'a>(
    action: &'a str,
    body: impl Renderable + 'a,
    error: Option<&'a str>,
) -> impl Renderable + 'a {
    maud! {
        @if let Some(error)=error { div class="alert alert-danger" role="alert" { (error) } }
        form method="post" action=(action) hx-post=(format!("{action}{}modal=true",if action.contains('?') {"&"}else{"?"})) hx-target="#ui-modal" {
            (body)
            button type="submit" class="btn btn-primary" { "Save" }
        }
    }
}

pub fn invalid_form(
    query: crate::ui::response::ModalQuery,
    tournament: &Tournament,
    user: User<true>,
    action: &str,
    return_url: &str,
    submitted: &impl serde::Serialize,
    categories: &[(String, String)],
    error: &str,
) -> StandardResponse {
    editor_page(
        query,
        "Correct this form",
        return_url,
        tournament,
        user,
        submitted_form(action, fields(submitted, categories), Some(error)),
        true,
    )
}
pub fn editor_page(
    query: crate::ui::response::ModalQuery,
    title: &str,
    return_url: &str,
    tournament: &Tournament,
    user: User<true>,
    body: impl Renderable,
    invalid: bool,
) -> StandardResponse {
    crate::ui::response::form_page(
        query,
        title,
        return_url,
        tournament,
        user,
        maud! {
            p class="badge text-bg-secondary" { "Restricted: Tournament organisers" }
            (body)
        },
        invalid,
    )
}
