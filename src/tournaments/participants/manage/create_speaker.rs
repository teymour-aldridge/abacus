use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use hypertext::prelude::*;
use rand::{Rng, distr::Alphanumeric, rng};
use rocket::{FromForm, form::Form, get, post, response::Redirect};
use uuid::Uuid;

use crate::{
    auth::User,
    schema::{
        tournament_participants, tournament_speakers, tournament_team_speakers,
    },
    state::Conn,
    template::Page,
    tournaments::{Tournament, teams::Team},
    util_resp::{StandardResponse, see_other_ok, success},
    validation::is_valid_email,
};

#[get(
    "/tournaments/<tournament_id>/teams/<team_id>/speakers/create",
    rank = 1
)]
pub async fn create_speaker_page(
    tournament_id: &str,
    team_id: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let team = Team::fetch(team_id, tournament_id, &mut *conn)?;

    success(
        Page::new()
            .user(user)
            .tournament(tournament)
            .body(maud! {
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
            })
            .render(),
    )
}

#[derive(FromForm)]
pub struct CreateSpeakerForm {
    #[field(validate = len(..128))]
    name: String,
    #[field(validate = len(..254))]
    #[field(validate = is_valid_email())]
    email: String,
}

fn get_unique_private_url(
    tournament_id: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> String {
    loop {
        let id: String = rng()
            .sample_iter(Alphanumeric)
            .take(10)
            .map(char::from)
            .collect();

        let already_exists = diesel::dsl::select(diesel::dsl::exists(
            tournament_participants::table.filter(
                tournament_participants::tournament_id
                    .eq(tournament_id)
                    .and(tournament_participants::private_url.eq(&id)),
            ),
        ))
        .get_result::<bool>(conn)
        .unwrap();

        if !already_exists {
            return id;
        }
    }
}

#[post(
    "/tournaments/<tournament_id>/teams/<team_id>/speakers/create",
    data = "<form>"
)]
pub async fn do_create_speaker(
    tournament_id: &str,
    team_id: &str,
    user: User<true>,
    mut conn: Conn<true>,
    form: Form<CreateSpeakerForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let team = Team::fetch(team_id, tournament_id, &mut *conn)?;

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

    let speaker_id = Uuid::now_v7().to_string();
    let n = diesel::insert_into(tournament_speakers::table)
        .values((
            tournament_speakers::id.eq(&speaker_id),
            tournament_speakers::tournament_id.eq(&tournament.id),
            tournament_speakers::name.eq(&form.name),
            tournament_speakers::email.eq(&form.email),
            tournament_speakers::participant_id.eq(participant_id),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    diesel::insert_into(tournament_team_speakers::table)
        .values((
            tournament_team_speakers::team_id.eq(team.id),
            tournament_team_speakers::speaker_id.eq(speaker_id),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    // todo: should probably redirect back to team page if this is where the
    // user first navigated to the edit form
    see_other_ok(Redirect::to(format!(
        "/tournaments/{tournament_id}/participants"
    )))
}
