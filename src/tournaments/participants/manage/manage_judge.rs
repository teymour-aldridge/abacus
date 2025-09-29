use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::{form::Form, get, post, response::Redirect};
use uuid::Uuid;

use crate::{
    auth::User,
    schema::{tournament_institutions, tournament_judges},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        participants::{
            Institution, Judge,
            manage::{
                create_judge::CreateJudgeForm,
                institution_selector::InstitutionSelector,
            },
        },
    },
    util_resp::{StandardResponse, err_not_found, see_other_ok, success},
};

#[get("/tournaments/<tournament_id>/judges/<judge_id>/edit", rank = 2)]
pub async fn edit_judge_details_page(
    user: User<true>,
    tournament_id: &str,
    judge_id: &str,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

    let judge = match tournament_judges::table
        .filter(
            tournament_judges::tournament_id
                .eq(&tournament.id)
                .and(tournament_judges::id.eq(&judge_id)),
        )
        .first::<Judge>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(judge) => judge,
        None => return err_not_found(),
    };

    let institutions = tournament_institutions::table
        .filter(tournament_institutions::tournament_id.eq(&tournament_id))
        .load::<Institution>(&mut *conn)
        .unwrap();

    let institution_picker = InstitutionSelector::new(
        &institutions,
        match &judge.institution_id {
            Some(id) => Some(id.as_str()),
            None => None,
        },
        Some("institution_id"),
    );

    success(
        Page::new()
            .user(user)
            .tournament(tournament)
            .body(maud! {
                form method="post" {
                  div class="mb-3" {
                    label for="judgeName" class="form-label" { "Name of judge" }
                    input type="text"
                          class="form-control"
                          id="judgeName"
                          aria-describedby="judgeNameHelp"
                          value=(judge.name)
                          name="name";
                    div id="judgeNameHelp" class="form-text" {
                        "The name of the judge (this will be displayed publicly"
                        "on the tab site)."
                    }
                  }
                  div class="mb-3" {
                    label for="judgeEmail" class="form-label" { "Email of judge" }
                    input type="email"
                          class="form-control"
                          id="judgeEmail"
                          aria-describedby="judgeEmailHelp"
                          value=(judge.email)
                          name="email";
                    div id="judgeNameHelp" class="form-text" {
                        "The email of the judge (not displayed publicly)."
                    }
                  }
                  (institution_picker)
                  button type="submit" class="btn btn-primary" { "Create team" }
                }
            })
            .render(),
    )
}

#[post(
    "/tournaments/<tournament_id>/judges/<judge_id>/edit",
    rank = 2,
    data = "<form>"
)]
pub async fn do_edit_judge_details(
    tournament_id: &str,
    judge_id: &str,
    user: User<true>,
    mut conn: Conn<true>,
    form: Form<CreateJudgeForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

    let judge = match tournament_judges::table
        .find(judge_id)
        .filter(tournament_judges::tournament_id.eq(&tournament.id))
        .first::<Judge>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(judge) => judge,
        None => return err_not_found(),
    };

    let institution_id = match form.institution_id.as_str() {
        "-----" => None,
        id => {
            let inst_exists = diesel::dsl::select(diesel::dsl::exists(
                tournament_judges::table.filter(
                    tournament_judges::id
                        .eq(id)
                        .and(tournament_judges::institution_id.eq(id)),
                ),
            ))
            .get_result::<bool>(&mut *conn)
            .unwrap();

            if inst_exists {
                Some(id)
            } else {
                return err_not_found();
            }
        }
    };

    diesel::update(
        tournament_judges::table.filter(tournament_judges::id.eq(&judge.id)),
    )
    .set((
        tournament_judges::id.eq(Uuid::now_v7().to_string()),
        tournament_judges::tournament_id.eq(&tournament.id),
        tournament_judges::name.eq(&form.name),
        tournament_judges::email.eq(&form.email),
        tournament_judges::institution_id.eq(institution_id),
    ))
    .execute(&mut *conn)
    .unwrap();

    see_other_ok(Redirect::to(format!(
        "/tournaments/{}/participants",
        tournament.id
    )))
}
