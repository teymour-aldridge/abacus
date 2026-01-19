use axum::{
    extract::{Form, Path},
    response::Redirect,
};
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::User,
    schema::{tournament_institutions, tournament_judges},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        participants::{
            Institution, manage::gen_private_url::get_unique_private_url,
        },
        rounds::TournamentRounds,
    },
    util_resp::{StandardResponse, bad_request, see_other_ok, success},
    validation::is_valid_email,
};

pub async fn create_judge_page(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

    tracing::trace!("User has permission to manage participants.");

    let institutions = tournament_institutions::table
        .filter(tournament_institutions::tournament_id.eq(&tournament.id))
        .load::<Institution>(&mut *conn)
        .unwrap();

    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    success(Page::new()
        .user(user)
        .tournament(tournament.clone())
        .body(maud! {
            SidebarWrapper rounds=(&rounds) tournament=(&tournament) active_page=(None) selected_seq=(None) {
                form method="post" {
                    h1 {
                        "Create new judge"
                    }

                    div class="mb-3" {
                        label for="judgeName" class="form-label" { "Name of the judge" }
                        input
                            type="text"
                            class="form-control"
                            id="judgeName"
                            name="name"
                            required;
                    }
                    div class="mb-3" {
                        label for="judgeEmail" class="form-label" { "Email of the judge" }
                        input
                            type="text"
                            class="form-control"
                            id="judgeEmail"
                            aria-describedby="emailHelp"
                            name="email"
                            required;
                        div id="emailHelp" class="form-text" {
                            "The email of this judge."
                        }
                    }

                    div class="mb-3" {
                        label for="institution" { "Institution" }
                        select name="institution_id" id="institution" {
                            option value = "-----" {
                                "No institution"
                            }
                            @for institution in &institutions {
                                option value = (institution.id) {
                                    (institution.name)
                                }
                            }
                        }
                    }
                    button type="submit" class="btn btn-primary" { "Create team" }
                }
            }
        })
        .render())
}

#[derive(Deserialize)]
pub struct CreateJudgeForm {
    pub name: String,
    pub email: String,
    pub institution_id: String,
}

pub async fn do_create_judge(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateJudgeForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;

    tracing::trace!("Retrieved tournament with id = {}", tournament.id);

    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

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

    let id = match form.institution_id.as_str() {
        "-----" => None,
        t => Some(t),
    };

    let inst = match id {
        Some(inst) => {
            match tournament_institutions::table
                .filter(tournament_institutions::id.eq(inst))
                .first::<Institution>(&mut *conn)
                .optional()
                .unwrap()
            {
                Some(inst) => Some(inst),
                None => {
                    return bad_request(
                        Page::new()
                            .user(user)
                            .tournament(tournament)
                            .body(maud! {
                                p {
                                    "Error: that institution does not exist."
                                }
                            })
                            .render(),
                    );
                }
            }
        }
        None => None,
    };

    let private_url = get_unique_private_url(&tournament.id, &mut *conn);

    let next_number = tournament_judges::table
        .filter(tournament_judges::tournament_id.eq(&tournament_id))
        .order_by(tournament_judges::number.desc())
        .select(tournament_judges::number)
        .first::<i64>(&mut *conn)
        .optional()
        .unwrap()
        .unwrap_or(0)
        + 1;

    let n = diesel::insert_into(tournament_judges::table)
        .values((
            tournament_judges::id.eq(Uuid::now_v7().to_string()),
            tournament_judges::tournament_id.eq(&tournament.id),
            tournament_judges::name.eq(&form.name),
            tournament_judges::email.eq(&form.email),
            tournament_judges::institution_id
                .eq(inst.map(|inst| inst.id.clone())),
            tournament_judges::private_url.eq(private_url),
            tournament_judges::number.eq(next_number),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/participants",
        tournament.id
    )))
}
