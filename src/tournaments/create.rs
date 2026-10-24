use axum::{extract::Form, response::Redirect};
use chrono::Utc;
use diesel::{prelude::*, result::DatabaseErrorKind};
use hypertext::prelude::*;
use serde::Deserialize;

use crate::util_resp::bad_request;

use crate::schema::{org, tournaments};
use crate::state::Conn;
use crate::template::Page;

use crate::auth::User;
use crate::tournaments::config::{
    PullupMetric, RankableTeamMetric, SpeakerMetric,
};
use crate::util_resp::{StandardResponse, SuccessResponse, see_other_ok};
use crate::validation::is_valid_slug;

fn tournament_form<'a>(
    form: &'a CreateTournamentForm,
    error: Option<&'a str>,
) -> impl Renderable + 'a {
    maud! { div class="container py-4" {
        h1 { "Create tournament" }
        @if let Some(error)=error { div class="alert alert-danger" role="alert" { (error) } }
        form method="post" action="/tournaments/create" {
            div class="mb-3" {
                label for="tournament-name" class="form-label" { "Tournament name" }
                input type="text" class="form-control" id="tournament-name" name="name" value=(&form.name) minlength="4" maxlength="32" required;
            }
            div class="mb-3" {
                label for="tournament-abbreviation" class="form-label" { "Tournament abbreviation" }
                input type="text" class="form-control" id="tournament-abbreviation" name="abbrv" value=(&form.abbrv) minlength="2" maxlength="8" required;
            }
            div class="mb-3" {
                label for="tournament-slug" class="form-label" { "Tournament slug" }
                input type="text" class="form-control" id="tournament-slug" name="slug" value=(&form.slug) pattern="[a-zA-Z0-9]+" required;
            }
            button type="submit" class="btn btn-primary" { "Create tournament" }
        }
    } }
}
fn invalid_tournament(
    user: User<true>,
    form: &CreateTournamentForm,
    error: &str,
) -> StandardResponse {
    bad_request(
        Page::new()
            .user(user)
            .body(tournament_form(form, Some(error)))
            .render(),
    )
}
pub async fn create_tournament_page(user: User<true>) -> SuccessResponse {
    let form = CreateTournamentForm::default();
    SuccessResponse::Success(
        Page::new()
            .user(user)
            .body(tournament_form(&form, None))
            .render(),
    )
}

#[derive(Deserialize, Default)]
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
    let tid = uuid::Uuid::now_v7().to_string();

    if !(4..=32).contains(&form.name.len()) {
        return invalid_tournament(
            user,
            &form,
            "Tournament name must be between 4 and 32 characters.",
        );
    }
    if !(2..=8).contains(&form.abbrv.len()) {
        return invalid_tournament(
            user,
            &form,
            "Tournament abbreviation must be between 2 and 8 characters.",
        );
    }
    if let Err(error) = is_valid_slug(&form.slug) {
        return invalid_tournament(user, &form, &error.to_string());
    }

    let res = diesel::insert_into(tournaments::table)
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
        .execute(&mut *conn);

    match res {
        Ok(n) => {
            assert_eq!(n, 1);
        }
        Err(diesel::result::Error::DatabaseError(e, _))
            if e == DatabaseErrorKind::UniqueViolation =>
        {
            return invalid_tournament(
                user,
                &form,
                "Tournament already exists.",
            );
        }
        Err(error) => return Err(error.into()),
    }

    let n = diesel::insert_into(org::table)
        .values((
            org::id.eq(uuid::Uuid::now_v7().to_string()),
            org::user_id.eq(user.id),
            org::tournament_id.eq(&tid),
            org::is_superuser.eq(true),
        ))
        .execute(&mut *conn)?;
    assert_eq!(n, 1);

    see_other_ok(Redirect::to(&format!("/tournaments/{tid}")))
}
