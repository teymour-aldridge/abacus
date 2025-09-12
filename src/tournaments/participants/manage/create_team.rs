use diesel::{
    dsl::{exists, select},
    prelude::*,
};
use hypertext::prelude::*;
use rocket::{FromForm, form::Form, get, response::Redirect};
use tokio::task::spawn_blocking;
use uuid::Uuid;

use crate::{
    auth::User,
    permission::IsTabDirector,
    schema::{tournament_institutions, tournament_teams},
    state::{Conn, LockedConn},
    template::Page,
    tournaments::{
        Tournament, participants::Institution, snapshots::take_snapshot,
    },
    util_resp::GenerallyUsefulResponse,
};

#[get("/tournaments/<_tid>/teams/create")]
pub async fn create_teams_page(
    _tid: &str,
    tournament: Tournament,
    mut conn: LockedConn<'_>,
    user: User,
    _tab: IsTabDirector,
) -> Rendered<String> {
    let institutions = tournament_institutions::table
        .filter(tournament_institutions::tournament_id.eq(&tournament.id))
        .load::<Institution>(&mut *conn)
        .unwrap();

    Page::new()
        .user(user)
        .tournament(tournament)
        .body(maud! {
            form {
              div class="mb-3" {
                label for="teamName" class="form-label" { "Name of new team" }
                input type="email" class="form-control" id="teamName" aria-describedby="teamNameHelp";
                div id="teamNameHelp" class="form-text" {
                    "The team name. Please note that (if an institution is "
                    "selected) this will be prefixed with the institution name."
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
        .render()
}

#[derive(FromForm)]
pub struct CreateTeamForm {
    #[field(validate = len(4..=32))]
    pub name: String,
    pub institution_id: String,
}

#[get("/tournaments/<tid>/teams/create", data = "<form>")]
pub async fn do_create_team(
    tid: &str,
    tournament: Tournament,
    conn: Conn,
    user: User,
    _tab: IsTabDirector,
    form: Form<CreateTeamForm>,
) -> GenerallyUsefulResponse {
    let tid = tid.to_string();
    spawn_blocking(move || {
        let mut conn = conn.get_sync();

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
                        return GenerallyUsefulResponse::BadRequest(
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

        let exists = select(exists(
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
            return GenerallyUsefulResponse::BadRequest(
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

        let n = diesel::insert_into(tournament_teams::table)
            .values((
                (tournament_teams::id.eq(Uuid::now_v7().to_string())),
                tournament_teams::tournament_id.eq(&tid),
                tournament_teams::name.eq(&form.name),
                tournament_teams::institution_id
                    .eq(inst.as_ref().map(|inst| inst.id.clone())),
            ))
            .execute(&mut *conn)
            .unwrap();
        assert_eq!(n, 1);

        take_snapshot(&tid, &mut* conn);

        GenerallyUsefulResponse::Success(Redirect::to(format!(
            "/tournaments/{}/participants",
            tournament.id
        )))
    }).await.unwrap()
}
