use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::{form::Form, get, post, response::Redirect};
use tokio::sync::broadcast::Sender;

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{tournament_institutions, tournament_teams},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        participants::{
            Institution,
            manage::{
                create_team::CreateTeamForm,
                institution_selector::InstitutionSelector,
            },
        },
        snapshots::take_snapshot,
        teams::Team,
    },
    util_resp::{
        StandardResponse, bad_request, err_not_found, see_other_ok, success,
    },
    widgets::actions::Actions,
};

/// This has rank = 2 so that it does not collide with
/// [`super::create_team::create_team_page`].
#[get("/tournaments/<tournament_id>/teams/<team_id>", rank = 2)]
pub async fn manage_team_page(
    user: User<true>,
    tournament_id: &str,
    team_id: &str,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

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
        None => return err_not_found(),
    };

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                h1 {
                    "Team " (team.name)
                }

                Actions options=(&[
                    (format!("/tournaments/{}/teams/{}/speakers/create", tournament.id, team.id).as_str(), "Add speaker")
                ]);

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
    user: User<true>,
    tournament_id: &str,
    team_id: &str,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

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
        None => return err_not_found(),
    };

    let institutions = tournament_institutions::table
        .filter(tournament_institutions::tournament_id.eq(&tournament.id))
        .load::<Institution>(&mut *conn)
        .unwrap();
    let institution_selector = InstitutionSelector::new(
        &institutions,
        match &team.institution_id {
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
                // todo: this can be deduplicated from `create_team`
                form method="post" {
                  div class="mb-3" {
                    label for="teamName" class="form-label" { "Name of new team" }
                    input type="text"
                          class="form-control"
                          id="teamName"
                          aria-describedby="teamNameHelp"
                          value=(team.name)
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
            .render(),
    )
}

#[post("/tournaments/<tournament_id>/teams/<team_id>/edit", data = "<form>")]
pub async fn do_edit_team_details(
    user: User<true>,
    tournament_id: &str,
    team_id: &str,
    form: Form<CreateTeamForm>,
    tx: &rocket::State<Sender<Msg>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

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
            return bad_request(
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

    diesel::update(
        tournament_teams::table.filter(tournament_teams::id.eq(&team.id)),
    )
    .set((
        tournament_teams::name.eq(&form.name),
        tournament_teams::institution_id.eq(inst.map(|t| t.id)),
    ))
    .execute(&mut *conn)
    .unwrap();

    take_snapshot(&tournament.id, &mut *conn);

    let _ = tx.send(Msg {
        tournament: tournament.clone(),
        inner: MsgContents::ParticipantsUpdate,
    });

    see_other_ok(Redirect::to(format!(
        "/tournaments/{}/teams/{}",
        tournament.id, team.id
    )))
}
