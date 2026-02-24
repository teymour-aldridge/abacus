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
    schema::{speakers, speakers_of_team},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament, manage::sidebar::SidebarWrapper,
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

    let speaker_id = Uuid::now_v7().to_string();
    let n = diesel::insert_into(speakers::table)
        .values((
            speakers::id.eq(&speaker_id),
            speakers::tournament_id.eq(&tournament.id),
            speakers::name.eq(&form.name),
            speakers::email.eq(&form.email),
            speakers::private_url.eq(private_url),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    diesel::insert_into(speakers_of_team::table)
        .values((
            speakers_of_team::id.eq(Uuid::now_v7().to_string()),
            speakers_of_team::team_id.eq(team.id),
            speakers_of_team::speaker_id.eq(speaker_id),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    // todo: should probably redirect back to team page if this is where the
    // user first navigated to the edit form
    see_other_ok(Redirect::to(&format!(
        "/tournaments/{tournament_id}/participants"
    )))
}
