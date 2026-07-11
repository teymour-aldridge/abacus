use axum::{
    extract::{Form, Path},
    response::Redirect,
};
use diesel::{prelude::*, result::DatabaseErrorKind};
use hypertext::prelude::*;
use serde::Deserialize;

use crate::{
    auth::User,
    schema::{
        speaker_categories, speaker_category_memberships, speakers,
        speakers_of_team,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament, categories::SpeakerCategory,
        manage::sidebar::SidebarWrapper,
        participants::manage::gen_private_url::get_unique_private_url,
        rounds::TournamentRounds, teams::Team,
    },
    util_resp::{StandardResponse, bad_request, see_other_ok, success},
    validation::is_valid_email,
};

pub async fn create_speaker_page(
    Path((tournament_id, team_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let team = Team::fetch(&team_id, &tournament_id, &mut *conn)?;
    let categories = speaker_categories::table
        .filter(speaker_categories::tournament_id.eq(&tournament_id))
        .order_by(speaker_categories::seq.asc())
        .load::<SpeakerCategory>(&mut *conn)
        .unwrap();

    let rounds = TournamentRounds::fetch(&tournament_id, &mut *conn).unwrap();

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper rounds=(&rounds) tournament=(&tournament) active_page=(None) selected_seq=(None) {
                    h1 {
                        "Add speaker to " (team.name)
                    }
                    form method="post" class="mt-4" {
                        div class="mb-3" {
                            label for="name" class="form-label" { "Name" }
                            input type="text" class="form-control" id="name" name="name";
                        }
                        div class="mb-3" {
                            label for="email" class="form-label" { "Email" }
                            input type="email" class="form-control" id="email" name="email";
                        }
                        @if !categories.is_empty() {
                            div class="mb-3" {
                                label class="form-label" { "Speaker categories" }
                                div class="d-flex flex-wrap gap-3" {
                                    @for category in &categories {
                                        div class="form-check" {
                                            input class="form-check-input" type="checkbox" name="category_ids" value=(category.id) id=(format!("cat-{}", category.id));
                                            label class="form-check-label" for=(format!("cat-{}", category.id)) { (category.name) }
                                        }
                                    }
                                }
                            }
                        }
                        button type="submit" class="btn btn-primary" { "Register" }
                    }
                }
            })
            .render(),
    )
}

#[derive(Deserialize)]
pub struct CreateSpeakerForm {
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub category_ids: Vec<String>,
}

#[tracing::instrument(skip(conn, form))]
pub async fn do_create_speaker(
    Path((tournament_id, team_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateSpeakerForm>,
) -> StandardResponse {
    tracing::trace!("Create speaker route");

    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let team = Team::fetch(&team_id, &tournament_id, &mut *conn)?;

    if form.name.trim().is_empty() {
        return bad_request(
            Page::new()
                .user(user)
                .tournament(tournament)
                .body(maud! {
                    "Error: Name must not be empty."
                })
                .render(),
        );
    }
    if form.name.len() > 128 {
        return bad_request(
            Page::new()
                .user(user)
                .tournament(tournament)
                .body(maud! {
                    "Error: Name is too long (max 128 characters)."
                })
                .render(),
        );
    }
    if form.email.len() > 254 {
        return bad_request(
            Page::new()
                .user(user)
                .tournament(tournament)
                .body(maud! {
                    "Error: Email is too long (max 254 characters)."
                })
                .render(),
        );
    }
    if let Err(_) = is_valid_email(&form.email) {
        return bad_request(
            Page::new()
                .user(user)
                .tournament(tournament)
                .body(maud! {
                    "Error: Invalid email address."
                })
                .render(),
        );
    }

    let private_url = get_unique_private_url(&tournament.id, &mut *conn);

    let speaker_id = uuid::Uuid::now_v7().to_string();
    let res = diesel::insert_into(speakers::table)
        .values((
            speakers::id.eq(&speaker_id),
            speakers::tournament_id.eq(&tournament.id),
            speakers::name.eq(&form.name),
            speakers::email.eq(&form.email),
            speakers::private_url.eq(private_url),
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
                        "Error: a speaker with that name already exists."
                    })
                    .render(),
            );
        }
        Err(e) => return Err(e.into()),
    }

    let n = diesel::insert_into(speakers_of_team::table)
        .values((
            speakers_of_team::id.eq(uuid::Uuid::now_v7().to_string()),
            speakers_of_team::team_id.eq(team.id),
            speakers_of_team::speaker_id.eq(&speaker_id),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    let valid_category_ids: std::collections::HashSet<String> =
        speaker_categories::table
            .filter(speaker_categories::tournament_id.eq(&tournament.id))
            .select(speaker_categories::id)
            .load::<String>(&mut *conn)
            .unwrap()
            .into_iter()
            .collect();
    for category_id in form
        .category_ids
        .into_iter()
        .filter(|id| valid_category_ids.contains(id))
    {
        diesel::insert_into(speaker_category_memberships::table)
            .values((
                speaker_category_memberships::id
                    .eq(uuid::Uuid::now_v7().to_string()),
                speaker_category_memberships::tournament_id.eq(&tournament.id),
                speaker_category_memberships::speaker_id.eq(&speaker_id),
                speaker_category_memberships::category_id.eq(category_id),
            ))
            .execute(&mut *conn)
            .unwrap();
    }

    crate::tournaments::categories::speaker::recompute_break_eligibility(
        &tournament.id,
        &mut *conn,
    );

    // todo: should probably redirect back to team page if this is where the
    // user first navigated to the edit form
    see_other_ok(Redirect::to(&format!(
        "/tournaments/{tournament_id}/participants"
    )))
}
