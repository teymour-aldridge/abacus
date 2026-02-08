use axum::{extract::Form, response::Redirect};
use chrono::Utc;
use diesel::prelude::*;
use hypertext::prelude::*;
use serde::Deserialize;
use uuid::Uuid;

use crate::util_resp::bad_request;

use crate::schema::{tournament_members, tournaments};
use crate::state::Conn;
use crate::template::Page;

use crate::auth::User;
use crate::tournaments::config::{
    PullupMetric, RankableTeamMetric, SpeakerMetric,
};
use crate::util_resp::{StandardResponse, SuccessResponse, see_other_ok};
use crate::validation::is_valid_slug;

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
                              required
                              name="name";
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
                              required
                              name="abbrv";
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
                                pattern = "[a-zA-Z0-9]+"
                                name="slug";
                        div id = "tournamentSlugHelp" class="form-text" {
                            "A unique identifier for the tournament, used in URLs."
                        }
                    }
                    button type="submit" class="btn btn-primary" {
                        "Submit"
                    }
                }
            })
            .render(),
    )
}

#[derive(Deserialize)]
pub struct CreateTournamentForm {
    name: String,
    abbrv: String,
    slug: String,
}

pub async fn do_create_tournament(
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateTournamentForm>,
) -> StandardResponse {
    let tid = Uuid::now_v7().to_string();

    if !(4..=32).contains(&form.name.len()) {
        return bad_request(
            maud! {p {"Tournament name must be between 4 and 32 characters."}}
                .render(),
        );
    }
    if !(2..=8).contains(&form.abbrv.len()) {
        return bad_request(maud! {p {"Tournament abbreviation must be between 2 and 8 characters."}}.render());
    }
    if let Err(e) = is_valid_slug(&form.slug) {
        return bad_request(maud! {p {(e)}}.render());
    }

    let n = diesel::insert_into(tournaments::table)
        .values((
            tournaments::id.eq(&tid),
            tournaments::name.eq(&form.name),
            tournaments::abbrv.eq(&form.abbrv),
            tournaments::slug.eq(&form.slug),
            tournaments::created_at.eq(Utc::now().naive_utc()),
            tournaments::team_tab_public.eq(false),
            tournaments::speaker_tab_public.eq(false),
            tournaments::standings_public.eq(false),
            tournaments::teams_per_side.eq(2),
            tournaments::substantive_speakers.eq(2),
            tournaments::reply_speakers.eq(false),
            tournaments::reply_must_speak.eq(true),
            tournaments::max_substantive_speech_index_for_reply.eq(2),
            tournaments::pool_ballot_setup.eq("consensus"),
            tournaments::elim_ballot_setup.eq("consensus"),
            tournaments::require_prelim_speaker_order.eq(true),
            tournaments::require_prelim_substantive_speaks.eq(true),
            tournaments::require_elim_speaker_order.eq(true),
            tournaments::require_elim_substantive_speaks.eq(false),
            tournaments::institution_penalty.eq(0),
            tournaments::history_penalty.eq(0),
            tournaments::team_standings_metrics.eq(serde_json::to_string(&[
                RankableTeamMetric::Wins,
                RankableTeamMetric::NTimesAchieved(3),
                RankableTeamMetric::NTimesAchieved(2),
                RankableTeamMetric::NTimesAchieved(1),
                RankableTeamMetric::DrawStrengthByWins,
            ])
            .unwrap()),
            tournaments::speaker_standings_metrics.eq(serde_json::to_string(
                &[SpeakerMetric::Avg, SpeakerMetric::StdDev],
            )
            .unwrap()),
            tournaments::pullup_metrics
                .eq(serde_json::to_string(&[PullupMetric::Random]).unwrap()),
            tournaments::repeat_pullup_penalty.eq(0),
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
        ))
        .execute(&mut *conn)
        .unwrap();
    assert_eq!(n, 1);

    see_other_ok(Redirect::to(&format!("/tournaments/{tid}")))
}
