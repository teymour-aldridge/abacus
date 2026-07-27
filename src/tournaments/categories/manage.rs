use std::collections::{HashMap, HashSet};

use axum::{Form, extract::Path, response::Redirect};
use diesel::{prelude::*, result::DatabaseErrorKind};
use hypertext::prelude::*;
use serde::Deserialize;

use crate::{
    auth::User,
    schema::{
        break_categories, speaker_categories, speaker_category_implications,
        speakers, speakers_of_team, teams,
    },
    state::Conn,
    template::{ActiveNav, Page},
    tournaments::{
        Tournament,
        categories::{
            BreakCategory, EligibilityRule, SpeakerCategory,
            SpeakerCategoryImplication, SpeakerThreshold,
            speaker::{
                CategoryIndex, implication_would_create_cycle,
                recompute_break_eligibility,
            },
        },
        manage::sidebar::SidebarWrapper,
        participants::Speaker,
        rounds::TournamentRounds,
        teams::Team,
    },
    util_resp::{
        StandardResponse, bad_request, err_not_found, see_other_ok, success,
    },
};

#[derive(Deserialize)]
pub struct CreateSpeakerCategoryForm {
    name: String,
    slug: Option<String>,
    seq: i64,
    public: Option<String>,
    limit_: i64,
}

#[derive(Deserialize)]
pub struct CreateBreakCategoryForm {
    name: String,
    slug: Option<String>,
    seq: i64,
    priority: i64,
    break_size: i64,
    reserve_size: i64,
    public: Option<String>,
    limit_: i64,
}

#[derive(Deserialize)]
pub struct AddImplicationForm {
    child_category_id: String,
    parent_category_id: String,
}

#[derive(Deserialize)]
pub struct UpdateBreakRuleForm {
    threshold_type: String,
    count: Option<usize>,
    minus: Option<usize>,
    #[serde(default)]
    category_ids: Vec<String>,
}

pub async fn manage_categories_page(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
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
    let category_by_id: HashMap<String, SpeakerCategory> = speaker_categories
        .iter()
        .cloned()
        .map(|category| (category.id.clone(), category))
        .collect();

    success(
        Page::new()
            .active_nav(ActiveNav::Participants)
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) selected_seq=(None) active_page=(None) {
                    div class="d-flex flex-column gap-5" {
                        section {
                            div class="d-flex justify-content-between align-items-center mb-3" {
                                h1 class="h4 fw-bold mb-0" { "Speaker Categories" }
                            }

                            div class="card mb-4" {
                                div class="card-body bg-light" {
                                    form action=(format!("/tournaments/{}/categories/speaker/create", tournament.id)) method="post" class="row g-3 align-items-end" {
                                        div class="col-md-3" {
                                            label class="form-label" for="name" { "Name" }
                                            input class="form-control" type="text" name="name" placeholder="ESL" required;
                                        }
                                        div class="col-md-3" {
                                            label class="form-label" for="slug" { "Slug" }
                                            input class="form-control" type="text" name="slug" placeholder="esl";
                                        }
                                        div class="col-md-2" {
                                            label class="form-label" for="seq" { "Order" }
                                            input class="form-control" type="number" name="seq" value=(speaker_categories.len() + 1) required;
                                        }
                                        div class="col-md-2" {
                                            label class="form-label" for="limit_" { "Public limit" }
                                            input class="form-control" type="number" name="limit_" value="0" min="0" required;
                                        }
                                        div class="col-md-1 form-check mb-2" {
                                            input class="form-check-input" type="checkbox" name="public" id="speaker_public" checked;
                                            label class="form-check-label" for="speaker_public" { "Public" }
                                        }
                                        div class="col-md-1" {
                                            button class="btn btn-primary w-100" type="submit" { "Add" }
                                        }
                                    }
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
                                                    form action=(format!("/tournaments/{}/categories/speaker/{}/delete", tournament.id, category.id)) method="post"
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
                                    form action=(format!("/tournaments/{}/categories/implications/add", tournament.id)) method="post" class="row g-3 align-items-end" {
                                        div class="col-md-5" {
                                            label class="form-label" { "Category" }
                                            select class="form-select" name="child_category_id" required {
                                                option value="" selected disabled { "Choose category" }
                                                @for category in &speaker_categories {
                                                    option value=(category.id) { (category.name) }
                                                }
                                            }
                                        }
                                        div class="col-md-5" {
                                            label class="form-label" { "Counts as" }
                                            select class="form-select" name="parent_category_id" required {
                                                option value="" selected disabled { "Choose parent category" }
                                                @for category in &speaker_categories {
                                                    option value=(category.id) { (category.name) }
                                                }
                                            }
                                        }
                                        div class="col-md-2" {
                                            button class="btn btn-primary w-100" type="submit" { "Add" }
                                        }
                                    }
                                }
                            }
                            div class="d-flex flex-wrap gap-2" {
                                @for implication in &implications {
                                    @let child = category_by_id.get(&implication.child_category_id);
                                    @let parent = category_by_id.get(&implication.parent_category_id);
                                    @if let (Some(child), Some(parent)) = (child, parent) {
                                        div class="badge bg-white text-dark border d-inline-flex align-items-center gap-2 p-2" {
                                            (child.name) " counts as " (parent.name)
                                            form action=(format!("/tournaments/{}/categories/implications/{}/delete", tournament.id, implication.id)) method="post" class="m-0" {
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
                                    form action=(format!("/tournaments/{}/categories/break/create", tournament.id)) method="post" class="row g-3 align-items-end" {
                                        div class="col-md-3" {
                                            label class="form-label" { "Name" }
                                            input class="form-control" type="text" name="name" placeholder="ESL" required;
                                        }
                                        div class="col-md-2" {
                                            label class="form-label" { "Slug" }
                                            input class="form-control" type="text" name="slug" placeholder="esl";
                                        }
                                        div class="col-md-1" {
                                            label class="form-label" { "Order" }
                                            input class="form-control" type="number" name="seq" value=(break_categories.len() + 1) required;
                                        }
                                        div class="col-md-1" {
                                            label class="form-label" { "Priority" }
                                            input class="form-control" type="number" name="priority" value="0" min="0" required;
                                        }
                                        div class="col-md-1" {
                                            label class="form-label" { "Break" }
                                            input class="form-control" type="number" name="break_size" value="2" min="2" required;
                                        }
                                        div class="col-md-1" {
                                            label class="form-label" { "Reserve" }
                                            input class="form-control" type="number" name="reserve_size" value="0" min="0" required;
                                        }
                                        div class="col-md-1" {
                                            label class="form-label" { "Limit" }
                                            input class="form-control" type="number" name="limit_" value="0" min="0" required;
                                        }
                                        div class="col-md-1 form-check mb-2" {
                                            input class="form-check-input" type="checkbox" name="public" id="break_public" checked;
                                            label class="form-check-label" for="break_public" { "Public" }
                                        }
                                        div class="col-md-1" {
                                            button class="btn btn-primary w-100" type="submit" { "Add" }
                                        }
                                    }
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
                                            form action=(format!("/tournaments/{}/categories/break/{}/delete", tournament.id, break_category.id)) method="post"
                                                onsubmit="return confirm('Delete this break category?');" {
                                                button class="btn btn-sm btn-outline-danger" type="submit" { "Delete" }
                                            }
                                        }

                                        form action=(format!("/tournaments/{}/categories/break/{}/rule", tournament.id, break_category.id)) method="post" class="row g-3 align-items-end mb-3" {
                                            div class="col-md-3" {
                                                label class="form-label" { "Eligible when" }
                                                select class="form-select" name="threshold_type" {
                                                    option value="everyone" selected[is_everyone_rule(&rule)] { "Everyone" }
                                                    option value="all" selected[matches!(rule, EligibilityRule::IfThresholdMet { threshold: SpeakerThreshold::All, .. })] { "All speakers are in" }
                                                    option value="at_least" selected[matches!(rule, EligibilityRule::IfThresholdMet { threshold: SpeakerThreshold::Geq { .. }, .. })] { "At least N speakers are in" }
                                                    option value="at_least_minus" selected[matches!(rule, EligibilityRule::IfThresholdMet { threshold: SpeakerThreshold::GeqNMinusK { .. }, .. })] { "All but N speakers are in" }
                                                }
                                            }
                                            div class="col-md-2" {
                                                label class="form-label" { "N" }
                                                input class="form-control" type="number" name="count" min="1" value=(rule_count(&rule).unwrap_or(1));
                                            }
                                            div class="col-md-2" {
                                                label class="form-label" { "But N" }
                                                input class="form-control" type="number" name="minus" min="0" value=(rule_minus(&rule).unwrap_or(1));
                                            }
                                            div class="col-md-4" {
                                                label class="form-label" { "Speaker categories" }
                                                select class="form-select" name="category_ids" multiple size="3" {
                                                    @for category in &speaker_categories {
                                                        option value=(category.id) selected[selected_categories.contains(&category.id)] {
                                                            (category.name)
                                                        }
                                                    }
                                                }
                                            }
                                            div class="col-md-1" {
                                                button class="btn btn-primary w-100" type="submit" { "Save" }
                                            }
                                        }

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
                        }
                    }
                }
            })
            .render(),
    )
}

pub async fn create_speaker_category(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateSpeakerCategoryForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let slug = clean_slug(form.slug.as_deref().unwrap_or(&form.name));
    let res = diesel::insert_into(speaker_categories::table)
        .values((
            speaker_categories::id.eq(uuid::Uuid::now_v7().to_string()),
            speaker_categories::tournament_id.eq(&tid),
            speaker_categories::name.eq(form.name.trim()),
            speaker_categories::slug.eq(slug),
            speaker_categories::seq.eq(form.seq),
            speaker_categories::public.eq(form.public.is_some()),
            speaker_categories::limit_.eq(form.limit_),
        ))
        .execute(&mut *conn);
    match res {
        Ok(n) => assert_eq!(n, 1),
        Err(diesel::result::Error::DatabaseError(
            DatabaseErrorKind::UniqueViolation,
            _,
        )) => {
            return bad_request(
                Page::new()
                    .user(user)
                    .tournament(tournament)
                    .body(maud! {
                        "Error: a speaker category with that slug or sequence already exists."
                    })
                    .render(),
            );
        }
        Err(e) => return Err(e.into()),
    }
    recompute_break_eligibility(&tid, &mut *conn);
    see_other_ok(Redirect::to(&format!("/tournaments/{tid}/categories")))
}

pub async fn delete_speaker_category(
    Path((tid, category_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
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
    see_other_ok(Redirect::to(&format!("/tournaments/{tid}/categories")))
}

pub async fn create_break_category(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateBreakCategoryForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let slug = clean_slug(form.slug.as_deref().unwrap_or(&form.name));
    let res = diesel::insert_into(break_categories::table)
        .values((
            break_categories::id.eq(uuid::Uuid::now_v7().to_string()),
            break_categories::tournament_id.eq(&tid),
            break_categories::name.eq(form.name.trim()),
            break_categories::priority.eq(form.priority),
            break_categories::slug.eq(slug),
            break_categories::seq.eq(form.seq),
            break_categories::break_size.eq(form.break_size),
            break_categories::reserve_size.eq(form.reserve_size),
            break_categories::public.eq(form.public.is_some()),
            break_categories::limit_.eq(form.limit_),
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
            return bad_request(
                Page::new()
                    .user(user)
                    .tournament(tournament)
                    .body(maud! {
                        "Error: a break category with that slug or sequence already exists."
                    })
                    .render(),
            );
        }
        Err(e) => return Err(e.into()),
    }
    recompute_break_eligibility(&tid, &mut *conn);
    see_other_ok(Redirect::to(&format!("/tournaments/{tid}/categories")))
}

pub async fn delete_break_category(
    Path((tid, category_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
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
    see_other_ok(Redirect::to(&format!("/tournaments/{tid}/categories")))
}

pub async fn add_implication(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<AddImplicationForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let existing = speaker_category_implications::table
        .filter(speaker_category_implications::tournament_id.eq(&tid))
        .load::<SpeakerCategoryImplication>(&mut *conn)
        .unwrap();
    if form.child_category_id == form.parent_category_id
        || implication_would_create_cycle(
            &form.child_category_id,
            &form.parent_category_id,
            &existing,
        )
    {
        return bad_request(
            Page::new()
                .user(user)
                .tournament(tournament)
                .body(maud! { "Error: that counts-as rule would create a cycle." })
                .render(),
        );
    }
    diesel::insert_into(speaker_category_implications::table)
        .values((
            speaker_category_implications::id
                .eq(uuid::Uuid::now_v7().to_string()),
            speaker_category_implications::tournament_id.eq(&tid),
            speaker_category_implications::child_category_id
                .eq(&form.child_category_id),
            speaker_category_implications::parent_category_id
                .eq(&form.parent_category_id),
        ))
        .execute(&mut *conn)?;
    recompute_break_eligibility(&tid, &mut *conn);
    see_other_ok(Redirect::to(&format!("/tournaments/{tid}/categories")))
}

pub async fn delete_implication(
    Path((tid, implication_id)): Path<(String, String)>,
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
    see_other_ok(Redirect::to(&format!("/tournaments/{tid}/categories")))
}

pub async fn update_break_rule(
    Path((tid, break_category_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<UpdateBreakRuleForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let rule = match form.threshold_type.as_str() {
        "everyone" => EligibilityRule::AllowAll,
        "all" => EligibilityRule::IfThresholdMet {
            included_categories: form.category_ids,
            threshold: SpeakerThreshold::All,
        },
        "at_least" => EligibilityRule::IfThresholdMet {
            included_categories: form.category_ids,
            threshold: SpeakerThreshold::Geq {
                count: form.count.unwrap_or(1),
            },
        },
        "at_least_minus" => EligibilityRule::IfThresholdMet {
            included_categories: form.category_ids,
            threshold: SpeakerThreshold::GeqNMinusK {
                k: form.minus.unwrap_or(1),
            },
        },
        _ => return err_not_found(),
    };
    let valid_category_ids: HashSet<String> = speaker_categories::table
        .filter(speaker_categories::tournament_id.eq(&tid))
        .select(speaker_categories::id)
        .load::<String>(&mut *conn)
        .unwrap()
        .into_iter()
        .collect();
    if let Err(msg) = rule.validate(&valid_category_ids) {
        return bad_request(
            Page::new()
                .user(user)
                .tournament(tournament)
                .body(maud! { (msg) })
                .render(),
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
    see_other_ok(Redirect::to(&format!("/tournaments/{tid}/categories")))
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

fn is_everyone_rule(rule: &EligibilityRule) -> bool {
    matches!(rule, EligibilityRule::AllowAll)
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

fn eligibility_previews(
    tournament_id: &str,
    break_categories: &[BreakCategory],
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> HashMap<String, EligibilityPreview> {
    let category_index = CategoryIndex::load(tournament_id, conn);
    let teams = teams::table
        .filter(teams::tournament_id.eq(tournament_id))
        .order_by(teams::number.asc())
        .load::<Team>(conn)
        .unwrap();
    let speaker_rows = speakers_of_team::table
        .inner_join(teams::table)
        .inner_join(
            speakers::table.on(speakers_of_team::speaker_id.eq(speakers::id)),
        )
        .filter(teams::tournament_id.eq(tournament_id))
        .select((
            speakers_of_team::team_id,
            speakers_of_team::speaker_id,
            speakers::all_columns,
        ))
        .load::<(String, String, Speaker)>(conn)
        .unwrap();
    let mut speaker_ids_by_team: HashMap<String, Vec<String>> = HashMap::new();
    for (team_id, speaker_id, _) in speaker_rows {
        speaker_ids_by_team
            .entry(team_id)
            .or_default()
            .push(speaker_id);
    }

    break_categories
        .iter()
        .map(|break_category| {
            let rule = EligibilityRule::from_json_or_default(
                &break_category.eligibility_rule_json,
            );
            let mut rows = Vec::new();
            let mut eligible_count = 0;
            for team in &teams {
                let speaker_ids = speaker_ids_by_team
                    .get(&team.id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let result = rule.evaluate(speaker_ids, &category_index);
                if result.eligible {
                    eligible_count += 1;
                }
                rows.push(PreviewRow {
                    team_name: team.name.clone(),
                    eligible: result.eligible,
                    explanation: result.explanation,
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
