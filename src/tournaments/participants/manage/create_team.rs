use axum::{
    extract::{Extension, Form, Path},
    response::Redirect,
};
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;
use tokio::sync::broadcast::Sender;
use uuid::Uuid;

use crate::{
    auth::User,
    msg::{Msg, MsgContents},
    schema::{institutions, teams},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        participants::{
            Institution,
            manage::{
                institution_selector::InstitutionSelector, team_form::TeamForm,
            },
        },
        snapshots::take_snapshot,
    },
    util_resp::{StandardResponse, bad_request, see_other_ok, success},
};

pub async fn create_teams_page(
    Path(tid): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

    let institutions = institutions::table
        .filter(institutions::tournament_id.eq(&tournament.id))
        .load::<Institution>(&mut *conn)
        .unwrap();
    let institution_selector =
        InstitutionSelector::new(&institutions, None, Some("institution_id"));
    let form = TeamForm::new(&institution_selector);

    success(Page::new()
        .user(user)
        .tournament(tournament)
        .body(maud! {
            div class="card" {
                div class="card-body" {
                    h1 class="card-title" {
                        "Create new team"
                    }
                    form method="post" {
                      (form)
                      button type="submit" class="btn btn-primary" { "Create team" }
                    }
                }
            }
        })
        .render())
}

#[derive(Deserialize)]
pub struct CreateTeamForm {
    pub name: String,
    pub institution_id: String,
}

#[tracing::instrument(skip(conn, tx, form))]
pub async fn do_create_team(
    Path(tid): Path<String>,
    user: User<true>,
    Extension(tx): Extension<Sender<Msg>>,
    mut conn: Conn<true>,
    Form(form): Form<CreateTeamForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_has_permission(
        &user.id,
        crate::permission::Permission::ManageParticipants,
        &mut *conn,
    )?;

    if form.name.len() < 4 || form.name.len() > 32 {
        return bad_request(
            Page::new()
                .user(user)
                .tournament(tournament)
                .body(maud! {
                    "Error: Team name must be between 4 and 32 characters."
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
            match institutions::table
                .filter(institutions::id.eq(inst))
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
        teams::table.filter(
            teams::tournament_id
                .eq(&tid)
                .and(teams::name.eq(&form.name))
                .and(
                    teams::institution_id
                        .eq(inst.as_ref().map(|inst| inst.id.clone())),
                ),
        ),
    ))
    .get_result::<bool>(&mut *conn)
    .unwrap();

    if exists {
        tracing::trace!("Error: team already exists");
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

    tracing::trace!("Team does not exist, proceeding to create");

    let next_number = teams::table
        .filter(teams::tournament_id.eq(&tid))
        .order_by(teams::number.desc())
        .select(teams::number)
        .first::<i64>(&mut *conn)
        .optional()
        .unwrap()
        .unwrap_or(0)
        + 1;

    let id = Uuid::now_v7().to_string();
    let n = diesel::insert_into(teams::table)
        .values((
            (teams::id.eq(&id)),
            teams::tournament_id.eq(&tid),
            teams::name.eq(&form.name),
            teams::institution_id.eq(inst.as_ref().map(|inst| inst.id.clone())),
            teams::number.eq(next_number),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    take_snapshot(&tid, &mut *conn);

    let _ = tx.send(Msg {
        tournament: tournament.clone(),
        inner: MsgContents::ParticipantsUpdate,
    });

    see_other_ok(Redirect::to(&format!(
        "/tournaments/{}/participants",
        tournament.id
    )))
}
