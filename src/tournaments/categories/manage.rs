use std::collections::{HashMap, HashSet};

use axum::{extract::Path, response::Redirect};
use axum_extra::extract::Form;
use diesel::{prelude::*, result::DatabaseErrorKind};
use hypertext::prelude::*;
use serde::Deserialize;

use crate::{
    auth::User,
    schema::{
        break_categories, speaker_categories, speaker_category_implications,
        team_break_eligibility, teams,
    },
    state::Conn,
    tournaments::{
        Tournament,
        categories::{
            BreakCategory, EligibilityRule, SpeakerCategory,
            SpeakerCategoryImplication, SpeakerThreshold,
            speaker::{
                implication_would_create_cycle, recompute_break_eligibility,
            },
        },
        manage::sidebar::SidebarWrapper,
        rounds::TournamentRounds,
        standings::compute::TeamStandings,
        teams::Team,
    },
    util_resp::{StandardResponse, err_not_found},
};

#[derive(Deserialize, serde::Serialize)]
pub struct CreateSpeakerCategoryForm {
    name: String,
    slug: Option<String>,
    seq: String,
    public: Option<String>,
    limit_: String,
}

#[derive(Deserialize, serde::Serialize)]
pub struct CreateBreakCategoryForm {
    name: String,
    slug: Option<String>,
    seq: String,
    priority: String,
    break_size: String,
    reserve_size: String,
    public: Option<String>,
    limit_: String,
}

#[derive(Deserialize, serde::Serialize)]
pub struct AddImplicationForm {
    child_category_id: String,
    parent_category_id: String,
}

#[derive(Deserialize, serde::Serialize)]
pub struct UpdateBreakRuleForm {
    threshold_type: String,
    count: Option<String>,
    minus: Option<String>,
    #[serde(default)]
    category_ids: Vec<String>,
}

#[derive(Deserialize, serde::Serialize)]
pub struct SaveBreakRemarkForm {
    team_id: String,
    remark: String,
    #[serde(default)]
    allowed_category_ids: Vec<String>,
}

struct BreakTeamRow {
    team: Team,
    eligible_categories: Vec<String>,
    remark: Option<BreakRemarkView>,
}

struct BreakRemarkView {
    text: String,
    allowed_category_ids: HashSet<String>,
}

pub async fn manage_break_page(
    Path(tid): Path<String>,
    axum::extract::Query(query): axum::extract::Query<
        crate::tournaments::manage::workspace::WorkspaceQuery,
    >,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    let access = crate::permission::TournamentAccess::resolve(
        &tournament.id,
        user.as_ref().map(|u| u.id.as_str()),
        &mut *conn,
    )?;
    if !access.is_superuser() {
        if query.editing() {
            return Err(crate::util_resp::FailureResponse::Unauthorized(()));
        }
        let speaker_names = speaker_categories::table
            .filter(speaker_categories::tournament_id.eq(&tid))
            .filter(speaker_categories::public.eq(true))
            .order(speaker_categories::seq.asc())
            .select(speaker_categories::name)
            .load::<String>(&mut *conn)?;
        let break_names = break_categories::table
            .filter(break_categories::tournament_id.eq(&tid))
            .filter(break_categories::public.eq(true))
            .order(break_categories::seq.asc())
            .select(break_categories::name)
            .load::<String>(&mut *conn)?;
        return crate::util_resp::success(crate::template::Page::new().user_opt(user).tournament(tournament).body(maud! {
            h1 { "Categories" }
            @if speaker_names.is_empty() && break_names.is_empty() { p { "Category information is not yet public." } }
            @if !speaker_names.is_empty() { h2 { "Speaker categories" } ul { @for name in &speaker_names { li { (name) } } } }
            @if !break_names.is_empty() { h2 { "Break categories" } ul { @for name in &break_names { li { (name) } } } }
        }).render());
    }
    let user = user.expect("superuser is authenticated");
    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    let speaker_categories = speaker_categories::table
        .filter(speaker_categories::tournament_id.eq(&tid))
        .order_by(speaker_categories::seq.asc())
        .load::<SpeakerCategory>(&mut *conn)
        .unwrap();
    let break_categories = break_categories::table
        .filter(break_categories::tournament_id.eq(&tid))
        .order_by(break_categories::seq.asc())
        .load::<BreakCategory>(&mut *conn)
        .unwrap();
    let implications = speaker_category_implications::table
        .filter(speaker_category_implications::tournament_id.eq(&tid))
        .load::<SpeakerCategoryImplication>(&mut *conn)
        .unwrap();
    let previews = eligibility_previews(&tid, &break_categories, &mut *conn);
    let break_team_rows = break_team_rows(&tid, &break_categories, &mut *conn);
    let category_by_id: HashMap<String, SpeakerCategory> = speaker_categories
        .iter()
        .cloned()
        .map(|category| (category.id.clone(), category))
        .collect();

    let speaker_options: Vec<(String, String)> = speaker_categories
        .iter()
        .map(|c| (c.id.clone(), c.name.clone()))
        .collect();
    let break_options: Vec<(String, String)> = break_categories
        .iter()
        .map(|c| (c.id.clone(), c.name.clone()))
        .collect();
    let canonical = format!("/tournaments/{tid}/categories");
    if !query.editing() {
        return crate::tournaments::manage::workspace::read_page(
            &tournament,
            user,
            "Break categories",
            &canonical,
            &access,
            maud! { h2 { "Speaker categories" } ul { @for category in &speaker_categories { li { (category.name) } } } h2 { "Break categories" } @for category in &break_categories { h3 { (category.name) } @if category.public { span class="badge text-bg-success" { "Public" } } @else { span class="badge text-bg-danger" { "Unpublished" } } @if let Some(preview) = previews.get(&category.id) { p { (preview.eligible_count) " eligible teams" } } } h2 { "Team break status" } ul { @for row in &break_team_rows { li { (row.team.name) ": " (row.eligible_categories.join(", ")) } } } },
        );
    }
    crate::tournaments::manage::workspace::editor_page(
        query.form(),
        "Break categories",
        &canonical,
        &tournament.clone(),
        user,
        maud! {
                    SidebarWrapper tournament=(&tournament) rounds=(&rounds) selected_seq=(None) active_page=(None) {
                        div class="d-flex flex-column gap-5" {
                            section {
                                div class="d-flex justify-content-between align-items-center mb-3" {
                                    div {
                                        h1 class="h3 fw-bold mb-1" { "Manage Break" }
                                        p class="text-muted mb-0" { "Configure break categories, eligibility rules, and team overrides." }
                                    }
                                }

                                h2 class="h4 fw-bold mt-4 mb-3" { "Speaker Categories" }

                                div class="card mb-4" {
                                    div class="card-body bg-light" {
                                        (crate::tournaments::manage::workspace::submitted_form(&format!("/tournaments/{}/categories/speaker/create", tournament.id), crate::tournaments::manage::workspace::fields(&CreateSpeakerCategoryForm { name:String::new(),slug:Some(String::new()),seq:(speaker_categories.len()+1).to_string(),public:Some("true".into()),limit_:"0".into() }, &[]), None))
                                    }
                                }

                                div class="table-responsive border rounded" {
                                    table class="table table-hover align-middle mb-0" {
                                        thead class="bg-light" {
                                            tr {
                                                th { "Order" }
                                                th { "Name" }
                                                th { "Slug" }
                                                th { "Public" }
                                                th class="text-end" { "Actions" }
                                            }
                                        }
                                        tbody {
                                            @for category in &speaker_categories {
                                                tr {
                                                    td { (category.seq) }
                                                    td class="fw-medium" { (category.name) }
                                                    td { code { (category.slug) } }
                                                    td { (if category.public { "Yes" } else { "No" }) }
                                                    td class="text-end" {
                                                        form action=(format!("/tournaments/{}/categories/speaker/{}/delete", tournament.id, category.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/categories/speaker/{}/delete", tournament.id, category.id), if format!("/tournaments/{}/categories/speaker/{}/delete", tournament.id, category.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" method="post"
                                                            onsubmit="return confirm('Delete this speaker category? Memberships and implication links will also be removed.');" {
                                                            button class="btn btn-sm btn-outline-danger" type="submit" { "Delete" }
                                                        }
                                                    }
                                                }
                                            }
                                            @if speaker_categories.is_empty() {
                                                tr { td colspan="5" class="text-center text-muted py-4" { "No speaker categories yet." } }
                                            }
                                        }
                                    }
                                }
                            }

                            section {
                                h2 class="h4 fw-bold mb-3" { "Category Counts-As Rules" }
                                div class="card mb-4" {
                                    div class="card-body bg-light" {
                                        (crate::tournaments::manage::workspace::submitted_form(&format!("/tournaments/{}/categories/implications/add", tournament.id), crate::tournaments::manage::workspace::fields(&AddImplicationForm {child_category_id:String::new(),parent_category_id:String::new()}, &speaker_options), None))
                                    }
                                }
                                div class="d-flex flex-wrap gap-2" {
                                    @for implication in &implications {
                                        @let child = category_by_id.get(&implication.child_category_id);
                                        @let parent = category_by_id.get(&implication.parent_category_id);
                                        @if let (Some(child), Some(parent)) = (child, parent) {
                                            div class="badge bg-white text-dark border d-inline-flex align-items-center gap-2 p-2" {
                                                (child.name) " counts as " (parent.name)
                                                form action=(format!("/tournaments/{}/categories/implications/{}/delete", tournament.id, implication.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/categories/implications/{}/delete", tournament.id, implication.id), if format!("/tournaments/{}/categories/implications/{}/delete", tournament.id, implication.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" method="post" class="m-0" {
                                                    button type="submit" class="btn-close" aria-label="Remove" {}
                                                }
                                            }
                                        }
                                    }
                                    @if implications.is_empty() {
                                        span class="text-muted" { "No counts-as rules yet." }
                                    }
                                }
                            }

                            section {
                                h2 class="h4 fw-bold mb-3" { "Break Categories" }
                                div class="card mb-4" {
                                    div class="card-body bg-light" {
                                        (crate::tournaments::manage::workspace::submitted_form(&format!("/tournaments/{}/categories/break/create", tournament.id), crate::tournaments::manage::workspace::fields(&CreateBreakCategoryForm { name:String::new(),slug:Some(String::new()),seq:(break_categories.len()+1).to_string(),priority:"0".into(),break_size:"2".into(),reserve_size:"0".into(),public:Some("true".into()),limit_:"0".into() }, &[]), None))
                                    }
                                }

                                div class="list-group" {
                                    @for break_category in &break_categories {
                                        @let rule = EligibilityRule::from_json_or_default(&break_category.eligibility_rule_json);
                                        @let selected_categories = rule_category_ids(&rule);
                                        @let preview = previews.get(&break_category.id);
                                        div class="list-group-item p-4" {
                                            div class="d-flex justify-content-between align-items-start gap-3 mb-3" {
                                                div {
                                                    h3 class="h5 fw-bold mb-1" { (break_category.name) }
                                                    div class="text-muted small" {
                                                        "Priority " (break_category.priority) " · break size " (break_category.break_size)
                                                    }
                                                }
                                                form action=(format!("/tournaments/{}/categories/break/{}/delete", tournament.id, break_category.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/categories/break/{}/delete", tournament.id, break_category.id), if format!("/tournaments/{}/categories/break/{}/delete", tournament.id, break_category.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" method="post"
                                                    onsubmit="return confirm('Delete this break category?');" {
                                                    button class="btn btn-sm btn-outline-danger" type="submit" { "Delete" }
                                                }
                                            }

                                            (crate::tournaments::manage::workspace::submitted_form(&format!("/tournaments/{}/categories/break/{}/rule", tournament.id, break_category.id), crate::tournaments::manage::workspace::fields(&UpdateBreakRuleForm {
            threshold_type: match rule { EligibilityRule::AllowAll => "everyone", EligibilityRule::IfThresholdMet { threshold: SpeakerThreshold::All,..} => "all", EligibilityRule::IfThresholdMet { threshold: SpeakerThreshold::Geq {..},..} => "at_least", _=> "at_least_minus" }.into(),
            count: Some(rule_count(&rule).unwrap_or(1).to_string()), minus: Some(rule_minus(&rule).unwrap_or(1).to_string()), category_ids: selected_categories.iter().cloned().collect()
        }, &speaker_options), None))

                                            @if let Some(preview) = preview {
                                                div class="small text-muted mb-2" {
                                                    (preview.eligible_count) " eligible, " (preview.ineligible_count) " ineligible"
                                                }
                                                div class="table-responsive border rounded" {
                                                    table class="table table-sm mb-0 align-middle" {
                                                        tbody {
                                                            @for row in preview.rows.iter().take(8) {
                                                                tr {
                                                                    td class=(if row.eligible { "text-success fw-medium" } else { "text-muted" }) {
                                                                        (if row.eligible { "Eligible" } else { "Ineligible" })
                                                                    }
                                                                    td { (row.team_name) }
                                                                    td class="text-muted" { (row.explanation) }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    @if break_categories.is_empty() {
                                        div class="list-group-item text-center text-muted py-5" { "No break categories yet." }
                                    }
                                }

                                div class="card mt-4" id="team-break-status" {
                                    div class="card-header bg-light" {
                                        h3 class="h5 fw-bold mb-1" { "Team break status" }
                                    }
                                    div class="table-responsive" {
                                        table class="table table-hover align-middle mb-0" {
                                            thead class="bg-light" {
                                                tr {
                                                    th { "Team" }
                                                    th { "Currently breaking in" }
                                                    th { "Break status" }
                                                    th { "Remark and category overrides" }
                                                }
                                            }
                                            tbody {
                                                @for row in &break_team_rows {
                                                    tr {
                                                        td class="fw-medium" { (row.team.name) }
                                                        td {
                                                            @for category in &row.eligible_categories {
                                                                span class="badge text-bg-success me-1" { (category) }
                                                            }
                                                            @if row.eligible_categories.is_empty() {
                                                                span class="text-muted" { "None" }
                                                            }
                                                        }
                                                        td {
                                                            @if row.remark.is_some() {
                                                                span class="badge text-bg-danger" { "Excluded by default" }
                                                            } @else {
                                                                span class="badge text-bg-secondary" { "Automatically computed" }
                                                            }
                                                        }
                                                        td {
                                                            @if let Some(remark) = &row.remark {
                                                                (crate::tournaments::manage::workspace::submitted_form(&format!("/tournaments/{}/categories/break-remarks/save", tournament.id), crate::tournaments::manage::workspace::fields(&SaveBreakRemarkForm { team_id:row.team.id.clone(),remark:remark.text.clone(),allowed_category_ids:remark.allowed_category_ids.iter().cloned().collect() }, &break_options), None))
                                                                form action=(format!("/tournaments/{}/categories/break-remarks/{}/delete", tournament.id, row.team.id)) hx-post=(format!("{}{}modal=true", format!("/tournaments/{}/categories/break-remarks/{}/delete", tournament.id, row.team.id), if format!("/tournaments/{}/categories/break-remarks/{}/delete", tournament.id, row.team.id).contains('?') { "&" } else { "?" })) hx-target="#ui-modal" hx-swap="innerHTML" method="post" {
                                                                    button class="btn btn-sm btn-outline-secondary" type="submit" { "Return to automatic" }
                                                                }
                                                            } @else {
                                                                (crate::tournaments::manage::workspace::submitted_form(&format!("/tournaments/{}/categories/break-remarks/save", tournament.id), crate::tournaments::manage::workspace::fields(&SaveBreakRemarkForm { team_id:row.team.id.clone(),remark:String::new(),allowed_category_ids:vec![] }, &break_options), None))
                                                            }
                                                        }
                                                    }
                                                }
                                                @if break_team_rows.is_empty() {
                                                    tr { td colspan="4" class="text-center text-muted py-4" { "No teams have been added." } }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
        false,
    )
}

pub async fn save_break_remark(
    Path(tid): Path<String>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<SaveBreakRemarkForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    Team::fetch(&form.team_id, &tid, &mut *conn)?;

    let remark = form.remark.trim();
    if remark.is_empty() {
        return invalid_category_form(
            modal_query,
            &tournament,
            user,
            &format!("/tournaments/{tid}/categories/break-remarks/save"),
            &format!("/tournaments/{tid}/categories"),
            &form,
            &mut *conn,
            "A break remark cannot be empty.",
        );
    }

    let category_ids = break_categories::table
        .filter(break_categories::tournament_id.eq(&tid))
        .select(break_categories::id)
        .load::<String>(&mut *conn)
        .unwrap();
    if category_ids.is_empty() {
        return invalid_category_form(
            modal_query,
            &tournament,
            user,
            &format!("/tournaments/{tid}/categories/break-remarks/save"),
            &format!("/tournaments/{tid}/categories"),
            &form,
            &mut *conn,
            "Create a break category before excluding teams.",
        );
    }
    let valid_category_ids: HashSet<&String> = category_ids.iter().collect();
    if form
        .allowed_category_ids
        .iter()
        .any(|id| !valid_category_ids.contains(id))
    {
        return err_not_found();
    }

    diesel::delete(
        team_break_eligibility::table
            .filter(team_break_eligibility::tournament_id.eq(&tid))
            .filter(team_break_eligibility::team_id.eq(&form.team_id)),
    )
    .execute(&mut *conn)?;
    let included: HashSet<&String> = form.allowed_category_ids.iter().collect();
    for category_id in &category_ids {
        let is_included = included.contains(category_id);
        diesel::insert_into(team_break_eligibility::table)
            .values((
                team_break_eligibility::id.eq(uuid::Uuid::now_v7().to_string()),
                team_break_eligibility::tournament_id.eq(&tid),
                team_break_eligibility::team_id.eq(&form.team_id),
                team_break_eligibility::break_category_id.eq(category_id),
                team_break_eligibility::eligible.eq(is_included),
                team_break_eligibility::source.eq(if is_included {
                    "manual_include"
                } else {
                    "manual_exclude"
                }),
                team_break_eligibility::explanation.eq(remark),
            ))
            .execute(&mut *conn)?;
    }
    recompute_break_eligibility(&tid, &mut *conn);
    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!(
            "/tournaments/{tid}/categories#team-break-status"
        )),
    )
}

pub async fn delete_break_remark(
    Path((tid, team_id)): Path<(String, String)>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    Team::fetch(&team_id, &tid, &mut *conn)?;
    diesel::delete(
        team_break_eligibility::table
            .filter(team_break_eligibility::tournament_id.eq(&tid))
            .filter(team_break_eligibility::team_id.eq(&team_id)),
    )
    .execute(&mut *conn)?;
    recompute_break_eligibility(&tid, &mut *conn);
    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!(
            "/tournaments/{tid}/categories#team-break-status"
        )),
    )
}

pub async fn create_speaker_category(
    Path(tid): Path<String>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateSpeakerCategoryForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let seq = form.seq.parse::<i64>().ok();
    let limit_ = form.limit_.parse::<i64>().ok();
    if form.name.trim().is_empty()
        || seq.is_none_or(|n| n < 1)
        || limit_.is_none_or(|n| n < 0)
    {
        return invalid_category_form(
            modal_query,
            &tournament,
            user,
            &format!("/tournaments/{tid}/categories/speaker/create"),
            &format!("/tournaments/{tid}/categories"),
            &form,
            &mut *conn,
            "Enter a name and valid whole numbers. Order must be positive, teams advancing at least two, and other values non-negative.",
        );
    }
    let slug = clean_slug(
        form.slug
            .as_deref()
            .filter(|slug| !slug.trim().is_empty())
            .unwrap_or(&form.name),
    );
    let res = diesel::insert_into(speaker_categories::table)
        .values((
            speaker_categories::id.eq(uuid::Uuid::now_v7().to_string()),
            speaker_categories::tournament_id.eq(&tid),
            speaker_categories::name.eq(form.name.trim()),
            speaker_categories::slug.eq(slug),
            speaker_categories::seq.eq(seq.unwrap()),
            speaker_categories::public.eq(form.public.is_some()),
            speaker_categories::limit_.eq(limit_.unwrap()),
        ))
        .execute(&mut *conn);
    match res {
        Ok(n) => assert_eq!(n, 1),
        Err(diesel::result::Error::DatabaseError(
            DatabaseErrorKind::UniqueViolation,
            _,
        )) => {
            return invalid_category_form(
                modal_query,
                &tournament,
                user,
                &format!("/tournaments/{tid}/categories/speaker/create"),
                &format!("/tournaments/{tid}/categories"),
                &form,
                &mut *conn,
                "Error: a speaker category with that slug or sequence already exists.",
            );
        }
        Err(e) => return Err(e.into()),
    }
    recompute_break_eligibility(&tid, &mut *conn);
    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{tid}/categories")),
    )
}

pub async fn delete_speaker_category(
    Path((tid, category_id)): Path<(String, String)>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    speaker_categories::table
        .filter(speaker_categories::tournament_id.eq(&tid))
        .filter(speaker_categories::id.eq(&category_id))
        .select(speaker_categories::id)
        .first::<String>(&mut *conn)
        .optional()?
        .ok_or(crate::util_resp::FailureResponse::NotFound(()))?;

    diesel::delete(
        speaker_category_implications::table.filter(
            speaker_category_implications::child_category_id
                .eq(&category_id)
                .or(speaker_category_implications::parent_category_id
                    .eq(&category_id)),
        ),
    )
    .execute(&mut *conn)?;
    diesel::delete(
        crate::schema::speaker_category_memberships::table.filter(
            crate::schema::speaker_category_memberships::category_id
                .eq(&category_id),
        ),
    )
    .execute(&mut *conn)?;
    diesel::delete(
        speaker_categories::table
            .filter(speaker_categories::tournament_id.eq(&tid))
            .filter(speaker_categories::id.eq(&category_id)),
    )
    .execute(&mut *conn)?;
    recompute_break_eligibility(&tid, &mut *conn);
    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{tid}/categories")),
    )
}

pub async fn create_break_category(
    Path(tid): Path<String>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateBreakCategoryForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let seq = form.seq.parse::<i64>().ok();
    let priority = form.priority.parse::<i64>().ok();
    let break_size = form.break_size.parse::<i64>().ok();
    let reserve_size = form.reserve_size.parse::<i64>().ok();
    let limit_ = form.limit_.parse::<i64>().ok();
    if form.name.trim().is_empty()
        || seq.is_none_or(|n| n < 1)
        || priority.is_none_or(|n| n < 0)
        || break_size.is_none_or(|n| n < 2)
        || reserve_size.is_none_or(|n| n < 0)
        || limit_.is_none_or(|n| n < 0)
    {
        return invalid_category_form(
            modal_query,
            &tournament,
            user,
            &format!("/tournaments/{tid}/categories/break/create"),
            &format!("/tournaments/{tid}/categories"),
            &form,
            &mut *conn,
            "Enter a name and valid whole numbers. Order must be positive, teams advancing at least two, and other values non-negative.",
        );
    }
    let slug = clean_slug(
        form.slug
            .as_deref()
            .filter(|slug| !slug.trim().is_empty())
            .unwrap_or(&form.name),
    );
    let category_id = uuid::Uuid::now_v7().to_string();
    let manually_managed_teams: HashMap<String, String> =
        team_break_eligibility::table
            .filter(team_break_eligibility::tournament_id.eq(&tid))
            .filter(team_break_eligibility::source.ne("derived"))
            .select((
                team_break_eligibility::team_id,
                team_break_eligibility::explanation,
            ))
            .load::<(String, String)>(&mut *conn)
            .unwrap()
            .into_iter()
            .collect();
    let res = diesel::insert_into(break_categories::table)
        .values((
            break_categories::id.eq(&category_id),
            break_categories::tournament_id.eq(&tid),
            break_categories::name.eq(form.name.trim()),
            break_categories::priority.eq(priority.unwrap()),
            break_categories::slug.eq(slug),
            break_categories::seq.eq(seq.unwrap()),
            break_categories::break_size.eq(break_size.unwrap()),
            break_categories::reserve_size.eq(reserve_size.unwrap()),
            break_categories::public.eq(form.public.is_some()),
            break_categories::limit_.eq(limit_.unwrap()),
            break_categories::eligibility_rule_json
                .eq(serde_json::to_string(&EligibilityRule::AllowAll).unwrap()),
        ))
        .execute(&mut *conn);
    match res {
        Ok(n) => assert_eq!(n, 1),
        Err(diesel::result::Error::DatabaseError(
            DatabaseErrorKind::UniqueViolation,
            _,
        )) => {
            return invalid_category_form(
                modal_query,
                &tournament,
                user,
                &format!("/tournaments/{tid}/categories/break/create"),
                &format!("/tournaments/{tid}/categories"),
                &form,
                &mut *conn,
                "Error: a break category with that slug or sequence already exists.",
            );
        }
        Err(e) => return Err(e.into()),
    }
    for (team_id, remark) in manually_managed_teams {
        diesel::insert_into(team_break_eligibility::table)
            .values((
                team_break_eligibility::id.eq(uuid::Uuid::now_v7().to_string()),
                team_break_eligibility::tournament_id.eq(&tid),
                team_break_eligibility::team_id.eq(team_id),
                team_break_eligibility::break_category_id.eq(&category_id),
                team_break_eligibility::eligible.eq(false),
                team_break_eligibility::source.eq("manual_exclude"),
                team_break_eligibility::explanation.eq(remark),
            ))
            .execute(&mut *conn)?;
    }
    recompute_break_eligibility(&tid, &mut *conn);
    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{tid}/categories")),
    )
}

pub async fn delete_break_category(
    Path((tid, category_id)): Path<(String, String)>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    break_categories::table
        .filter(break_categories::tournament_id.eq(&tid))
        .filter(break_categories::id.eq(&category_id))
        .select(break_categories::id)
        .first::<String>(&mut *conn)
        .optional()?
        .ok_or(crate::util_resp::FailureResponse::NotFound(()))?;

    diesel::delete(
        crate::schema::team_break_eligibility::table.filter(
            crate::schema::team_break_eligibility::break_category_id
                .eq(&category_id),
        ),
    )
    .execute(&mut *conn)?;
    diesel::delete(
        break_categories::table
            .filter(break_categories::tournament_id.eq(&tid))
            .filter(break_categories::id.eq(&category_id)),
    )
    .execute(&mut *conn)?;
    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{tid}/categories")),
    )
}

pub async fn add_implication(
    Path(tid): Path<String>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<AddImplicationForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let selected_category_ids = speaker_categories::table
        .select(speaker_categories::id)
        .filter(speaker_categories::tournament_id.eq(&tid))
        .filter(
            speaker_categories::id
                .eq(&form.child_category_id)
                .or(speaker_categories::id.eq(&form.parent_category_id)),
        )
        .load::<String>(&mut *conn)?;
    if !selected_category_ids.contains(&form.child_category_id)
        || !selected_category_ids.contains(&form.parent_category_id)
    {
        return invalid_category_form(
            modal_query,
            &tournament,
            user,
            &format!("/tournaments/{tid}/categories/implications/add"),
            &format!("/tournaments/{tid}/categories"),
            &form,
            &mut *conn,
            "Error: both speaker categories must belong to this tournament.",
        );
    }
    let existing = speaker_category_implications::table
        .filter(speaker_category_implications::tournament_id.eq(&tid))
        .load::<SpeakerCategoryImplication>(&mut *conn)
        .unwrap();
    if existing.iter().any(|implication| {
        implication.child_category_id == form.child_category_id
            && implication.parent_category_id == form.parent_category_id
    }) {
        return invalid_category_form(
            modal_query,
            &tournament,
            user,
            &format!("/tournaments/{tid}/categories/implications/add"),
            &format!("/tournaments/{tid}/categories"),
            &form,
            &mut *conn,
            "Error: that counts-as rule already exists.",
        );
    }
    if form.child_category_id == form.parent_category_id
        || implication_would_create_cycle(
            &form.child_category_id,
            &form.parent_category_id,
            &existing,
        )
    {
        return invalid_category_form(
            modal_query,
            &tournament,
            user,
            &format!("/tournaments/{tid}/categories/implications/add"),
            &format!("/tournaments/{tid}/categories"),
            &form,
            &mut *conn,
            "Error: that counts-as rule would create a cycle.",
        );
    }
    let inserted = diesel::insert_into(speaker_category_implications::table)
        .values((
            speaker_category_implications::id
                .eq(uuid::Uuid::now_v7().to_string()),
            speaker_category_implications::tournament_id.eq(&tid),
            speaker_category_implications::child_category_id
                .eq(&form.child_category_id),
            speaker_category_implications::parent_category_id
                .eq(&form.parent_category_id),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(inserted, 1);
    recompute_break_eligibility(&tid, &mut *conn);
    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{tid}/categories")),
    )
}

pub async fn delete_implication(
    Path((tid, implication_id)): Path<(String, String)>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    diesel::delete(
        speaker_category_implications::table
            .filter(speaker_category_implications::tournament_id.eq(&tid))
            .filter(speaker_category_implications::id.eq(&implication_id)),
    )
    .execute(&mut *conn)?;
    recompute_break_eligibility(&tid, &mut *conn);
    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{tid}/categories")),
    )
}

pub async fn update_break_rule(
    Path((tid, break_category_id)): Path<(String, String)>,
    axum::extract::Query(modal_query): axum::extract::Query<
        crate::ui::response::ModalQuery,
    >,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<UpdateBreakRuleForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    break_categories::table
        .filter(break_categories::tournament_id.eq(&tid))
        .filter(break_categories::id.eq(&break_category_id))
        .select(break_categories::id)
        .first::<String>(&mut *conn)
        .optional()?
        .ok_or(crate::util_resp::FailureResponse::NotFound(()))?;

    let count = form
        .count
        .as_deref()
        .filter(|v| !v.is_empty())
        .unwrap_or("1")
        .parse::<usize>()
        .ok();
    let minus = form
        .minus
        .as_deref()
        .filter(|v| !v.is_empty())
        .unwrap_or("1")
        .parse::<usize>()
        .ok();
    if count.is_none() || minus.is_none() {
        return invalid_category_form(
            modal_query,
            &tournament,
            user,
            &format!(
                "/tournaments/{tid}/categories/break/{break_category_id}/rule"
            ),
            &format!("/tournaments/{tid}/categories"),
            &form,
            &mut *conn,
            "Speaker counts must be non-negative whole numbers.",
        );
    }
    let rule = match form.threshold_type.as_str() {
        "everyone" => EligibilityRule::AllowAll,
        "all" => EligibilityRule::IfThresholdMet {
            included_categories: form.category_ids.clone(),
            threshold: SpeakerThreshold::All,
        },
        "at_least" => EligibilityRule::IfThresholdMet {
            included_categories: form.category_ids.clone(),
            threshold: SpeakerThreshold::Geq {
                count: count.unwrap(),
            },
        },
        "at_least_minus" => EligibilityRule::IfThresholdMet {
            included_categories: form.category_ids.clone(),
            threshold: SpeakerThreshold::GeqNMinusK { k: minus.unwrap() },
        },
        _ => {
            return invalid_category_form(
                modal_query,
                &tournament,
                user,
                &format!(
                    "/tournaments/{tid}/categories/break/{break_category_id}/rule"
                ),
                &format!("/tournaments/{tid}/categories"),
                &form,
                &mut *conn,
                "Choose an eligibility condition.",
            );
        }
    };
    let valid_category_ids: HashSet<String> = speaker_categories::table
        .filter(speaker_categories::tournament_id.eq(&tid))
        .select(speaker_categories::id)
        .load::<String>(&mut *conn)
        .unwrap()
        .into_iter()
        .collect();
    if let Err(msg) = rule.validate(&valid_category_ids) {
        return invalid_category_form(
            modal_query,
            &tournament,
            user,
            &format!(
                "/tournaments/{tid}/categories/break/{break_category_id}/rule"
            ),
            &format!("/tournaments/{tid}/categories"),
            &form,
            &mut *conn,
            &msg,
        );
    }
    diesel::update(
        break_categories::table
            .filter(break_categories::tournament_id.eq(&tid))
            .filter(break_categories::id.eq(&break_category_id)),
    )
    .set(
        break_categories::eligibility_rule_json
            .eq(serde_json::to_string(&rule).unwrap()),
    )
    .execute(&mut *conn)?;
    recompute_break_eligibility(&tid, &mut *conn);
    crate::tournaments::manage::workspace::after_redirect(
        modal_query,
        Redirect::to(&format!("/tournaments/{tid}/categories")),
    )
}

fn clean_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in value.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn rule_category_ids(rule: &EligibilityRule) -> HashSet<String> {
    match rule {
        EligibilityRule::AllowAll => HashSet::new(),
        EligibilityRule::IfThresholdMet {
            included_categories: categories,
            ..
        } => categories.iter().cloned().collect(),
    }
}

fn rule_count(rule: &EligibilityRule) -> Option<usize> {
    match rule {
        EligibilityRule::IfThresholdMet {
            threshold: SpeakerThreshold::Geq { count },
            ..
        } => Some(*count),
        _ => None,
    }
}

fn rule_minus(rule: &EligibilityRule) -> Option<usize> {
    match rule {
        EligibilityRule::IfThresholdMet {
            threshold: SpeakerThreshold::GeqNMinusK { k: minus },
            ..
        } => Some(*minus),
        _ => None,
    }
}

struct PreviewRow {
    team_name: String,
    eligible: bool,
    explanation: String,
}

struct EligibilityPreview {
    eligible_count: usize,
    ineligible_count: usize,
    rows: Vec<PreviewRow>,
}

fn break_team_rows(
    tournament_id: &str,
    break_categories: &[BreakCategory],
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> Vec<BreakTeamRow> {
    let teams = teams::table
        .filter(teams::tournament_id.eq(tournament_id))
        .order_by(teams::number.asc())
        .load::<Team>(conn)
        .unwrap();
    let eligibility = team_break_eligibility::table
        .filter(team_break_eligibility::tournament_id.eq(tournament_id))
        .select((
            team_break_eligibility::team_id,
            team_break_eligibility::break_category_id,
            team_break_eligibility::eligible,
            team_break_eligibility::source,
            team_break_eligibility::explanation,
        ))
        .load::<(String, String, bool, String, String)>(conn)
        .unwrap();
    let standings = TeamStandings::fetch(tournament_id, conn);
    let mut eligible_by_category: HashMap<String, HashSet<String>> =
        HashMap::new();
    let mut included_by_team: HashMap<String, HashSet<String>> = HashMap::new();
    let mut remark_text_by_team: HashMap<String, String> = HashMap::new();
    for (team_id, category_id, eligible, source, explanation) in eligibility {
        if eligible {
            eligible_by_category
                .entry(category_id.clone())
                .or_default()
                .insert(team_id.clone());
        }
        if source == "manual_include" {
            included_by_team
                .entry(team_id.clone())
                .or_default()
                .insert(category_id);
        }
        if source != "derived" {
            remark_text_by_team.entry(team_id).or_insert(explanation);
        }
    }
    let mut eligible_by_team: HashMap<String, Vec<String>> = HashMap::new();
    for category in break_categories {
        let eligible = eligible_by_category
            .get(&category.id)
            .cloned()
            .unwrap_or_default();
        let mut selected = 0;
        for team in standings.teams_in_rank_order.iter().flatten() {
            if selected >= category.break_size as usize {
                break;
            }
            if eligible.contains(&team.id) {
                eligible_by_team
                    .entry(team.id.clone())
                    .or_default()
                    .push(category.name.clone());
                selected += 1;
            }
        }
    }
    let remarks_by_team: HashMap<String, BreakRemarkView> = remark_text_by_team
        .into_iter()
        .map(|(team_id, text)| {
            let allowed_category_ids =
                included_by_team.remove(&team_id).unwrap_or_default();
            (
                team_id,
                BreakRemarkView {
                    text,
                    allowed_category_ids,
                },
            )
        })
        .collect();

    teams
        .into_iter()
        .map(|team| {
            let eligible_categories =
                eligible_by_team.remove(&team.id).unwrap_or_default();
            let remark =
                remarks_by_team.get(&team.id).map(|remark| BreakRemarkView {
                    text: remark.text.clone(),
                    allowed_category_ids: remark.allowed_category_ids.clone(),
                });
            BreakTeamRow {
                team,
                eligible_categories,
                remark,
            }
        })
        .collect()
}

fn eligibility_previews(
    tournament_id: &str,
    break_categories: &[BreakCategory],
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> HashMap<String, EligibilityPreview> {
    let teams = teams::table
        .filter(teams::tournament_id.eq(tournament_id))
        .order_by(teams::number.asc())
        .load::<Team>(conn)
        .unwrap();
    let effective_rows = team_break_eligibility::table
        .filter(team_break_eligibility::tournament_id.eq(tournament_id))
        .select((
            team_break_eligibility::team_id,
            team_break_eligibility::break_category_id,
            team_break_eligibility::eligible,
            team_break_eligibility::explanation,
        ))
        .load::<(String, String, bool, String)>(conn)
        .unwrap();
    let effective: HashMap<(String, String), (bool, String)> = effective_rows
        .into_iter()
        .map(|(team_id, category_id, eligible, explanation)| {
            ((team_id, category_id), (eligible, explanation))
        })
        .collect();

    break_categories
        .iter()
        .map(|break_category| {
            let mut rows = Vec::new();
            let mut eligible_count = 0;
            for team in &teams {
                let (eligible, explanation) = effective
                    .get(&(team.id.clone(), break_category.id.clone()))
                    .cloned()
                    .unwrap_or((
                        false,
                        "Eligibility has not been computed.".to_string(),
                    ));
                if eligible {
                    eligible_count += 1;
                }
                rows.push(PreviewRow {
                    team_name: team.name.clone(),
                    eligible,
                    explanation,
                });
            }
            (
                break_category.id.clone(),
                EligibilityPreview {
                    eligible_count,
                    ineligible_count: rows.len().saturating_sub(eligible_count),
                    rows,
                },
            )
        })
        .collect()
}

fn invalid_category_form(
    query: crate::ui::response::ModalQuery,
    tournament: &Tournament,
    user: User<true>,
    action: &str,
    return_url: &str,
    submitted: &impl serde::Serialize,
    conn: &mut diesel::SqliteConnection,
    error: &str,
) -> StandardResponse {
    let categories = if action.contains("break-remarks") {
        break_categories::table
            .filter(break_categories::tournament_id.eq(&tournament.id))
            .order(break_categories::seq.asc())
            .select((break_categories::id, break_categories::name))
            .load::<(String, String)>(conn)?
    } else {
        speaker_categories::table
            .filter(speaker_categories::tournament_id.eq(&tournament.id))
            .order(speaker_categories::seq.asc())
            .select((speaker_categories::id, speaker_categories::name))
            .load::<(String, String)>(conn)?
    };
    crate::tournaments::manage::workspace::invalid_form(
        query,
        tournament,
        user,
        action,
        return_url,
        submitted,
        &categories,
        error,
    )
}
