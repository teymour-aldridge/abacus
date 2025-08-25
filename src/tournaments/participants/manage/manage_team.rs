use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::{form::Form, get, response::Redirect};
use tokio::task::spawn_blocking;

use crate::{
    auth::User,
    permission::IsTabDirector,
    schema::{tournament_institutions, tournament_teams},
    state::{Conn, LockedConn},
    template::Page,
    tournaments::{
        Tournament,
        participants::{
            Institution,
            manage::create_team::{CreateTeamForm, CreateTeamResponse},
        },
        snapshots::take_snapshot,
        teams::Team,
    },
};

#[get("/tournaments/<tournament_id>/teams/<team_id>")]
pub async fn manage_team_page(
    user: User,
    tournament_id: &str,
    team_id: &str,
    tournament: Tournament,
    mut conn: LockedConn<'_>,
    _tab: IsTabDirector,
) -> Option<Rendered<String>> {
    let team = match tournament_teams::table
        .filter(
            tournament_teams::tournament_id
                .eq(tournament_id)
                .and(tournament_teams::id.eq(team_id)),
        )
        .first::<Team>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(team) => team,
        None => return None,
    };

    Some(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                h1 {
                    "Team " (team.name)
                }

                ul class="list-group list-group-horizontal" {
                    li class="list-group-item" {
                        a href=(format!("/tournaments/{}/teams/{}/edit",
                                tournament.id,
                                team.id))
                        {
                            "Edit team details"
                        }
                    }

                    li class="list-group-item" {
                        a href="" {
                            "Add speaker"
                        }
                    }
                }
            })
            .render(),
    )
}

#[get("/tournaments/<tournament_id>/teams/<team_id>/edit")]
pub async fn edit_team_details_page(
    user: User,
    tournament_id: &str,
    team_id: &str,
    tournament: Tournament,
    mut conn: LockedConn<'_>,
    _tab: IsTabDirector,
) -> Option<Rendered<String>> {
    let team = match tournament_teams::table
        .filter(
            tournament_teams::tournament_id
                .eq(tournament_id)
                .and(tournament_teams::id.eq(team_id)),
        )
        .first::<Team>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(team) => team,
        None => return None,
    };

    // todo: this could be a guard
    let institutions = tournament_institutions::table
        .filter(tournament_institutions::tournament_id.eq(&tournament.id))
        .load::<Institution>(&mut *conn)
        .unwrap();

    Some(
        Page::new()
            .user(user)
            .tournament(tournament)
            .body(maud! {
                // todo: this can be deduplicated from `create_team`
                form {
                  div class="mb-3" {
                    label for="teamName" class="form-label" { "Name of new team" }
                    input type="email"
                          class="form-control"
                          id="teamName"
                          aria-describedby="teamNameHelp"
                          value=(team.name);
                    div id="teamNameHelp" class="form-text" {
                        "The team name. Please note that (if an institution is "
                        "selected) this will be prefixed with the institution name."
                    }
                  }
                  div class="mb-3" {
                    label for="institution" { "Institution" }
                    select name="institution_id" id="institution" {
                        option value = "-----"
                            selected=(team.institution_id.is_none())
                        {
                            "No institution"
                        }
                        @for institution in &institutions {
                            option
                                value = (institution.id)
                                selected= (
                                    Some(&institution.id)
                                        == team.institution_id.as_ref()
                                )
                            {
                                (institution.name)
                            }
                        }
                    }
                  }
                  button type="submit" class="btn btn-primary" { "Create team" }
                }

            })
            .render(),
    )
}

#[get("/tournaments/<tournament_id>/teams/<team_id>/edit", data = "<form>")]
pub async fn do_edit_team_details(
    user: User,
    tournament_id: &str,
    team_id: &str,
    tournament: Tournament,
    conn: Conn,
    _tab: IsTabDirector,
    form: Form<CreateTeamForm>,
) -> CreateTeamResponse {
    let tournament_id = tournament_id.to_string();
    let team_id = team_id.to_string();
    spawn_blocking(move || {
        let mut conn = conn.get_sync();

        let team = match tournament_teams::table
            .filter(
                tournament_teams::tournament_id
                    .eq(tournament_id)
                    .and(tournament_teams::id.eq(team_id)),
            )
            .first::<Team>(&mut *conn)
            .optional()
            .unwrap()
        {
            Some(team) => team,
            None => {
                return CreateTeamResponse::BadRequest(
                    Page::new()
                        .tournament(tournament)
                        .user(user)
                        .body(maud! {
                            p {
                                "Error: no such team."
                            }
                        })
                        .render(),
                );
            }
        };

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
                        return CreateTeamResponse::BadRequest(
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

        diesel::update(
            tournament_teams::table.filter(tournament_teams::id.eq(&team.id)),
        )
        .set((
            tournament_teams::name.eq(&form.name),
            tournament_teams::institution_id.eq(inst.map(|t| t.id)),
        ))
        .execute(&mut *conn)
        .unwrap();

        take_snapshot(&tournament.id, conn);

        CreateTeamResponse::Created(Redirect::to(format!(
            "/tournaments/{}/teams/{}",
            tournament.id, team.id
        )))
    })
    .await
    .unwrap()
}
