use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::{FromForm, form::Form, get, post, response::Redirect};
use tokio::sync::broadcast::Sender;
use uuid::Uuid;

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{tournament_institutions, tournament_teams},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        participants::{
            Institution, manage::institution_selector::InstitutionSelector,
        },
        snapshots::take_snapshot,
    },
    util_resp::{StandardResponse, bad_request, see_other_ok, success},
};

#[get("/tournaments/<tid>/teams/create", rank = 1)]
pub async fn create_teams_page(
    tid: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

    let institutions = tournament_institutions::table
        .filter(tournament_institutions::tournament_id.eq(&tournament.id))
        .load::<Institution>(&mut *conn)
        .unwrap();
    let institution_selector =
        InstitutionSelector::new(&institutions, None, Some("institution_id"));

    success(Page::new()
        .user(user)
        .tournament(tournament)
        .body(maud! {
            form method="post" {
              h1 {
                  "Create new team"
              }
              div class="mb-3" {
                label for="teamName" class="form-label" { "Name of new team" }
                input
                    type="text"
                    class="form-control"
                    id="teamName"
                    aria-describedby="teamNameHelp"
                    name="name";
                div id="teamNameHelp" class="form-text" {
                    "The team name. Please note that (if an institution is "
                    "selected) this will be prefixed with the institution name."
                }
              }
              (institution_selector)
              button type="submit" class="btn btn-primary" { "Create team" }
            }
        })
        .render())
}

#[derive(FromForm)]
pub struct CreateTeamForm {
    #[field(validate = len(4..=32))]
    pub name: String,
    pub institution_id: String,
}

#[post("/tournaments/<tid>/teams/create", data = "<form>")]
pub async fn do_create_team(
    tid: &str,
    user: User<true>,
    form: Form<CreateTeamForm>,
    tx: &rocket::State<Sender<Msg>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
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

    let exists = diesel::dsl::select(diesel::dsl::exists(
        tournament_teams::table.filter(
            tournament_teams::tournament_id
                .eq(&tid)
                .and(tournament_teams::name.eq(&form.name))
                .and(
                    tournament_teams::institution_id
                        .eq(inst.as_ref().map(|inst| inst.id.clone())),
                ),
        ),
    ))
    .get_result::<bool>(&mut *conn)
    .unwrap();

    if exists {
        return bad_request(
            Page::new()
                .user(user)
                .tournament(tournament)
                .body(maud! {
                    p {
                        "Error: a team with that name already exists."
                    }
                })
                .render(),
        );
    }

    let next_number = tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(tid))
        .order_by(tournament_teams::number.desc())
        .select(tournament_teams::number)
        .first::<i64>(&mut *conn)
        .optional()
        .unwrap()
        .unwrap_or(0)
        + 1;

    let id = Uuid::now_v7().to_string();
    let n = diesel::insert_into(tournament_teams::table)
        .values((
            (tournament_teams::id.eq(&id)),
            tournament_teams::tournament_id.eq(&tid),
            tournament_teams::name.eq(&form.name),
            tournament_teams::institution_id
                .eq(inst.as_ref().map(|inst| inst.id.clone())),
            tournament_teams::number.eq(next_number),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    take_snapshot(tid, &mut *conn);

    let _ = tx.send(Msg {
        tournament: tournament.clone(),
        inner: MsgContents::ParticipantsUpdate,
    });

    see_other_ok(Redirect::to(format!(
        "/tournaments/{}/participants",
        tournament.id
    )))
}
