use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::{FromForm, form::Form, get, post, response::Redirect};
use uuid::Uuid;

use crate::{
    auth::User,
    schema::{
        tournament_institutions, tournament_judges, tournament_participants,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        participants::{
            Institution, manage::create_speaker::get_unique_private_url,
        },
    },
    util_resp::{StandardResponse, bad_request, see_other_ok, success},
    validation::is_valid_email,
};

#[get("/tournaments/<tournament_id>/judges/create")]
pub async fn create_judge_page(
    tournament_id: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

    let institutions = tournament_institutions::table
        .filter(tournament_institutions::tournament_id.eq(&tournament.id))
        .load::<Institution>(&mut *conn)
        .unwrap();

    success(Page::new()
        .user(user)
        .tournament(tournament)
        .body(maud! {
            form method="post" {
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
        })
        .render())
}

#[derive(FromForm)]
pub struct CreateJudgeForm {
    #[field(validate = len(..128))]
    pub name: String,
    #[field(validate = len(..254))]
    #[field(validate = is_valid_email())]
    pub email: String,
    pub institution_id: String,
}

#[post("/tournaments/<tournament_id>/judges/create", data = "<form>")]
pub async fn do_create_judge(
    tournament_id: &str,
    user: User<true>,
    form: Form<CreateJudgeForm>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

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

    let participant_id = Uuid::now_v7().to_string();
    let n = diesel::insert_into(tournament_participants::table)
        .values((
            tournament_participants::id.eq(&participant_id),
            tournament_participants::tournament_id.eq(tournament_id),
            tournament_participants::private_url
                .eq(get_unique_private_url(tournament_id, &mut *conn)),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    let next_number = tournament_judges::table
        .filter(tournament_judges::tournament_id.eq(tournament_id))
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
            tournament_judges::participant_id.eq(participant_id),
            tournament_judges::number.eq(next_number),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    see_other_ok(Redirect::to(format!(
        "/tournaments/{}/participants",
        tournament.id
    )))
}
