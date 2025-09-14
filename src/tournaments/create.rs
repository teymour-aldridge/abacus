use chrono::Utc;
use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::form::Form;
use rocket::{FromForm, get, post, response::Redirect};
use uuid::Uuid;

use crate::schema::{tournament_members, tournaments};
use crate::state::Conn;
use crate::template::Page;

use crate::auth::User;
use crate::tournaments::config::{SpeakerMetric, TeamMetric};
use crate::util_resp::{StandardResponse, SuccessResponse, see_other_ok};
use crate::validation::is_valid_slug;

#[get("/tournaments/create")]
pub async fn create_tournament_page(user: User<true>) -> SuccessResponse {
    SuccessResponse::Success(
        Page::new()
            .user(user)
            .body(maud! {
                form method="post" {
                    div class="mb-3" {
                        label for = "tournamentName" class="form-label" {
                            "Tournament name"
                        }
                        input type = "text"
                              class = "form-control"
                              id = "tournamentName"
                              aria-describedby="tournamentNameHelp"
                              minlength = "4"
                              maxlength = "32"
                              required;
                        div id = "tournamentNameHelp" class="form-text" {
                            "The full name of the tournament."
                        }
                    }
                    div class="mb-3" {
                        label for = "tournamentName" class="form-label" {
                            "Tournament abbreviation"
                        }
                        input type = "text"
                              minlength = "2"
                              maxlength = "8"
                              class = "form-control"
                              id = "tournamentName"
                              aria-describedby="tournamentNameHelp"
                              required;
                        div id = "tournamentNameHelp" class="form-text" {
                            "A short name for the tournament."
                        }
                    }
                    div class="mb-3" {
                        label for = "tournamentSlug" class="form-label" {
                            "Tournament slug"
                        }
                        input type = "text"
                                class = "form-control"
                                id = "tournamentSlug"
                                aria-describedby="tournamentSlugHelp"
                                required
                                pattern = r#"pattern="[a-zA-Z0-9]+""#;
                        div id = "tournamentSlugHelp" class="form-text" {
                            "A unique identifier for the tournament, used in URLs."
                        }
                    }
                }
            })
            .render(),
    )
}

#[derive(FromForm)]
pub struct CreateTournamentForm<'v> {
    #[field(validate = len(4..=32))]
    name: &'v str,
    #[field(validate = len(2..=8))]
    abbrv: &'v str,
    #[field(validate = is_valid_slug())]
    slug: &'v str,
}

#[post("/tournaments/create", data = "<form>")]
/// Performs the actual tournament creation.
///
/// The user will be added as a super user.
pub async fn do_create_tournament(
    form: Form<CreateTournamentForm<'_>>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tid = Uuid::now_v7().to_string();

    let n = diesel::insert_into(tournaments::table)
        .values((
            tournaments::id.eq(&tid),
            tournaments::name.eq(form.name),
            tournaments::abbrv.eq(form.abbrv),
            tournaments::slug.eq(form.slug),
            tournaments::created_at.eq(Utc::now().naive_utc()),
            tournaments::teams_per_side.eq(2),
            tournaments::substantive_speakers.eq(2),
            tournaments::reply_speakers.eq(false),
            tournaments::reply_must_speak.eq(true),
            tournaments::max_substantive_speech_index_for_reply.eq(2),
            tournaments::pool_ballot_setup.eq("consensus"),
            tournaments::elim_ballot_setup.eq("consensus"),
            tournaments::elim_ballots_require_speaks.eq(false),
            tournaments::institution_penalty.eq(None::<i64>),
            tournaments::history_penalty.eq(None::<i64>),
            tournaments::team_standings_metrics.eq(serde_json::to_string(&[
                TeamMetric::Wins,
                TeamMetric::NTimesAchieved(3),
                TeamMetric::NTimesAchieved(2),
                TeamMetric::NTimesAchieved(1),
                TeamMetric::DrawStrengthByWins,
            ])
            .unwrap()),
            tournaments::speaker_standings_metrics.eq(serde_json::to_string(
                &[SpeakerMetric::Avg, SpeakerMetric::StdDev],
            )
            .unwrap()),
            tournaments::exclude_from_speaker_standings_after.eq(-1),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    let n = diesel::insert_into(tournament_members::table)
        .values((
            tournament_members::id.eq(Uuid::now_v7().to_string()),
            tournament_members::user_id.eq(user.id),
            tournament_members::tournament_id.eq(&tid),
            tournament_members::is_superuser.eq(true),
            tournament_members::is_ca.eq(false),
            tournament_members::is_equity.eq(false),
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    see_other_ok(Redirect::to(format!("/tournaments/{tid}")))
}
