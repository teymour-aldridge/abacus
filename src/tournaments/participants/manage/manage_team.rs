use axum::{
    extract::{Extension, Form, Path},
    response::Redirect,
};
use diesel::prelude::*;
use hypertext::prelude::*;
use tokio::sync::broadcast::Sender;

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{tournament_institutions, tournament_teams},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        participants::{
            Institution,
            manage::{
                create_team::CreateTeamForm,
                institution_selector::InstitutionSelector, team_form::TeamForm,
            },
        },
        rounds::TournamentRounds,
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
pub async fn manage_team_page(
    Path((tournament_id, team_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

    let team = match tournament_teams::table
        .filter(
            tournament_teams::tournament_id
                .eq(&tournament_id)
                .and(tournament_teams::id.eq(&team_id)),
        )
        .first::<Team>(&mut *conn)
        .optional()
        .unwrap()
    {
        Some(team) => team,
        None => return err_not_found(),
    };

    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) active_page=(None) selected_seq=(None) {
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

                }
            })
            .render(),
    )
}

pub async fn edit_team_details_page(
    Path((tournament_id, team_id)): Path<(String, String)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

    let team = match tournament_teams::table
        .filter(
            tournament_teams::tournament_id
                .eq(&tournament_id)
                .and(tournament_teams::id.eq(&team_id)),
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

    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    let form = TeamForm::new(&institution_selector).with_team_name(&team.name);

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) active_page=(None) selected_seq=(None) {
                    div class="card" {
                        div class="card-body" {
                            h1 class="card-title" { "Edit Team" }
                            form method="post" {
                              (form)
                              button type="submit" class="btn btn-primary" { "Save" }
                            }
                        }
                    }
                }
            })
            .render(),
    )
}

pub async fn do_edit_team_details(
    Path((tournament_id, team_id)): Path<(String, String)>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    Form(form): Form<CreateTeamForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

    let team = match tournament_teams::table
        .filter(
            tournament_teams::tournament_id
                .eq(&tournament_id)
                .and(tournament_teams::id.eq(&team_id)),
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

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/teams/{}",
        tournament.id, team.id
    )))
}
