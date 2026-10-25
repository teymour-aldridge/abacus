use axum::{extract::Form, response::Redirect};
use chrono::Utc;
use diesel::{prelude::*, result::DatabaseErrorKind};
use hypertext::prelude::*;
use serde::Deserialize;

use crate::util_resp::bad_request;

use crate::schema::{tournament_membership, tournament_presets, tournaments};
use crate::state::Conn;
use crate::template::Page;

use crate::auth::User;
use crate::tournaments::{
    Tournament,
    config::{PullupMetric, RankableTeamMetric, SpeakerMetric},
    manage::config::{
        TournamentPreset, apply_tournament_config, parse_tournament_config,
    },
};
use crate::util_resp::{StandardResponse, SuccessResponse, see_other_ok};
use crate::validation::is_valid_slug;

fn tournament_form<'a>(
    form: &'a CreateTournamentForm,
    presets: &'a [TournamentPreset],
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
            div class="mb-3" {
                label for="tournament-preset" class="form-label" { "Format preset" }
                select id="tournament-preset" name="preset_id" class="form-select" required {
                    @for preset in presets {
                        option value=(&preset.id) selected[form.preset_id == preset.id] { (preset.name) }
                    }
                }
                div class="form-text" { "You can customize these settings in Configuration after creating the tournament." }
            }
            button type="submit" class="btn btn-primary" { "Create tournament" }
        }
    } }
}
fn invalid_tournament(
    user: User<true>,
    form: &CreateTournamentForm,
    presets: &[TournamentPreset],
    error: &str,
) -> StandardResponse {
    bad_request(
        Page::new()
            .user(user)
            .body(tournament_form(form, presets, Some(error)))
            .render(),
    )
}
pub async fn create_tournament_page(
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let presets = tournament_presets::table
        .order(tournament_presets::name.asc())
        .load::<TournamentPreset>(&mut *conn)?;
    let form = CreateTournamentForm::default();
    Ok(SuccessResponse::Success(
        Page::new()
            .user(user)
            .body(tournament_form(&form, &presets, None))
            .render(),
    ))
}

fn default_preset_id() -> String {
    "british-parliamentary".to_string()
}

#[derive(Deserialize)]
pub struct CreateTournamentForm {
    name: String,
    abbrv: String,
    slug: String,
    #[serde(default = "default_preset_id")]
    preset_id: String,
}

impl Default for CreateTournamentForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            abbrv: String::new(),
            slug: String::new(),
            preset_id: default_preset_id(),
        }
    }
}

pub async fn do_create_tournament(
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<CreateTournamentForm>,
) -> StandardResponse {
    let tid = uuid::Uuid::now_v7().to_string();
    let presets = tournament_presets::table
        .order(tournament_presets::name.asc())
        .load::<TournamentPreset>(&mut *conn)?;
    let Some(preset) = presets.iter().find(|p| p.id == form.preset_id) else {
        return invalid_tournament(
            user,
            &form,
            &presets,
            "Choose a valid format preset.",
        );
    };

    if !(4..=32).contains(&form.name.len()) {
        return invalid_tournament(
            user,
            &form,
            &presets,
            "Tournament name must be between 4 and 32 characters.",
        );
    }
    if !(2..=8).contains(&form.abbrv.len()) {
        return invalid_tournament(
            user,
            &form,
            &presets,
            "Tournament abbreviation must be between 2 and 8 characters.",
        );
    }
    if let Err(error) = is_valid_slug(&form.slug) {
        return invalid_tournament(user, &form, &presets, &error.to_string());
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
                &presets,
                "Tournament already exists.",
            );
        }
        Err(error) => return Err(error.into()),
    }

    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    let config = parse_tournament_config(&preset.config, &user, &tournament)?;
    apply_tournament_config(&tournament, config, &mut *conn)?;

    let n = diesel::insert_into(tournament_membership::table)
        .values((
            tournament_membership::user_id.eq(user.id),
            tournament_membership::tournament_id.eq(&tid),
            tournament_membership::role
                .eq(crate::permission::Role::Tab.as_str()),
        ))
        .execute(&mut *conn)?;
    assert_eq!(n, 1);

    see_other_ok(Redirect::to(&format!("/tournaments/{tid}")))
}
