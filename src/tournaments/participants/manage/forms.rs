use super::super::Institution;
use crate::{
    auth::User,
    permission::Permission,
    schema::institutions,
    tournaments::Tournament,
    ui::response::{ModalQuery, form_page},
    util_resp::{FailureResponse, StandardResponse},
};
use diesel::prelude::*;
use hypertext::prelude::*;

pub(super) fn authorize(
    tid: &str,
    uid: &str,
    conn: &mut diesel::SqliteConnection,
) -> Result<Tournament, FailureResponse> {
    let tournament = Tournament::fetch(tid, conn)?;
    tournament.check_user_has_permission(
        uid,
        Permission::ManageParticipants,
        conn,
    )?;
    Ok(tournament)
}
pub(super) fn institution(
    tid: &str,
    id: &str,
    conn: &mut diesel::SqliteConnection,
) -> Result<Option<String>, FailureResponse> {
    if id == "-----" || id.is_empty() {
        return Ok(None);
    }
    institutions::table
        .filter(institutions::tournament_id.eq(tid))
        .filter(institutions::id.eq(id))
        .select(institutions::id)
        .first::<String>(conn)
        .optional()?
        .map(Some)
        .ok_or(FailureResponse::NotFound(()))
}
pub(super) fn render(
    query: ModalQuery,
    title: &str,
    action: &str,
    tournament: &Tournament,
    user: User<true>,
    name: &str,
    email: Option<&str>,
    institution_id: Option<&str>,
    categories: Option<(&[(String, String)], &[String])>,
    error: Option<&str>,
    conn: &mut diesel::SqliteConnection,
) -> StandardResponse {
    let institutions = if institution_id.is_some() {
        institutions::table
            .filter(institutions::tournament_id.eq(&tournament.id))
            .load::<Institution>(conn)?
    } else {
        vec![]
    };
    form_page(
        query,
        title,
        &format!("/tournaments/{}/participants", tournament.id),
        tournament,
        user,
        maud! {
            @if let Some(error)=error { p class="alert alert-danger" role="alert" { (error) } }
            p class="text-muted" { "Restricted: Participant managers. Names and institutions are public; email addresses are restricted." }
            form method="post" action=(action) hx-post=(format!("{action}?modal=true")) hx-target="#ui-modal" hx-swap="innerHTML" {
                label class="form-label" for="participant-name" { "Name" }
                input class="form-control mb-3" id="participant-name" name="name" value=(name) required;
                @if let Some(email)=email {
                    label class="form-label" for="participant-email" { "Email (restricted)" }
                    input class="form-control mb-3" id="participant-email" name="email" value=(email) type="email" required;
                }
                @if let Some(selected)=institution_id {
                    label class="form-label" for="participant-institution" { "Institution" }
                    select class="form-select mb-3" id="participant-institution" name="institution_id" {
                        option value="-----" selected[selected=="-----" || selected.is_empty()] { "No institution" }
                        @for institution in &institutions { option value=(institution.id) selected[selected==institution.id] { (institution.name) } }
                    }
                }
                @if let Some((categories,selected))=categories { fieldset { legend { "Speaker categories" }
                    @for (id,name) in categories { label class="form-check" { input class="form-check-input" type="checkbox" name="category_ids" value=(id) checked[selected.contains(id)]; (name) } }
                } }
                button class="btn btn-primary mt-3" type="submit" { "Save" }
            }
        },
        error.is_some(),
    )
}
pub(super) fn person_error(name: &str, email: &str) -> Option<&'static str> {
    if name.trim().is_empty() {
        Some("Name must not be empty.")
    } else if name.len() > 128 {
        Some("Name is too long (max 128 characters).")
    } else if email.len() > 254 {
        Some("Email is too long (max 254 characters).")
    } else if crate::validation::is_valid_email(email).is_err() {
        Some("Invalid email address.")
    } else {
        None
    }
}
pub(super) fn team_error(name: &str) -> Option<&'static str> {
    if name.len() < 4 || name.len() > 32 {
        Some("Team name must be between 4 and 32 characters.")
    } else {
        None
    }
}

pub(super) fn team_exists(
    tid: &str,
    name: &str,
    institution: Option<&str>,
    exclude: Option<&str>,
    conn: &mut diesel::SqliteConnection,
) -> Result<bool, FailureResponse> {
    use crate::schema::teams;
    let mut query = teams::table
        .filter(teams::tournament_id.eq(tid))
        .filter(teams::name.eq(name))
        .into_boxed();
    query = match institution {
        Some(id) => query.filter(teams::institution_id.eq(id)),
        None => query.filter(teams::institution_id.is_null()),
    };
    if let Some(id) = exclude {
        query = query.filter(teams::id.ne(id));
    }
    Ok(query.count().get_result::<i64>(conn)? > 0)
}
