use diesel::prelude::*;
use hypertext::prelude::*;
use rocket::{FromForm, form::Form, get, post, response::Redirect};
use serde::{Deserialize, Serialize};

use crate::{
    auth::User,
    schema::tournaments,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament, manage::sidebar::SidebarWrapper, rounds::TournamentRounds,
    },
    util_resp::{StandardResponse, bad_request, see_other_ok, success},
};

#[derive(Serialize, Deserialize)]
pub struct TournamentConfig {
    pub team_tab_public: bool,
    pub speaker_tab_public: bool,
    pub teams_per_side: i64,
    pub substantive_speakers: i64,
    pub reply_speakers: bool,
    pub reply_must_speak: bool,
    pub max_substantive_speech_index_for_reply: Option<i64>,
    pub pool_ballot_setup: String,
    pub elim_ballot_setup: String,
    pub elim_ballots_require_speaks: bool,
    pub institution_penalty: i64,
    pub history_penalty: i64,
    pub pullup_metrics: String,
    pub repeat_pullup_penalty: i64,
    pub team_standings_metrics: String,
    pub speaker_standings_metrics: String,
    pub exclude_from_speaker_standings_after: Option<i64>,
}

fn config_of_tournament(tournament: &Tournament) -> TournamentConfig {
    TournamentConfig {
        team_tab_public: tournament.team_tab_public,
        speaker_tab_public: tournament.speaker_tab_public,
        teams_per_side: tournament.teams_per_side,
        substantive_speakers: tournament.substantive_speakers,
        reply_speakers: tournament.reply_speakers,
        reply_must_speak: tournament.reply_must_speak,
        max_substantive_speech_index_for_reply: tournament
            .max_substantive_speech_index_for_reply,
        pool_ballot_setup: tournament.pool_ballot_setup.clone(),
        elim_ballot_setup: tournament.elim_ballot_setup.clone(),
        elim_ballots_require_speaks: tournament.elim_ballots_require_speaks,
        institution_penalty: tournament.institution_penalty,
        history_penalty: tournament.history_penalty,
        pullup_metrics: tournament.pullup_metrics.clone(),
        repeat_pullup_penalty: tournament.repeat_pullup_penalty,
        team_standings_metrics: tournament.team_standings_metrics.clone(),
        speaker_standings_metrics: tournament.speaker_standings_metrics.clone(),
        exclude_from_speaker_standings_after: tournament
            .exclude_from_speaker_standings_after,
    }
}

#[get("/tournaments/<tournament_id>/configuration")]
pub async fn view_tournament_configuration(
    tournament_id: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let config = toml::to_string(&config_of_tournament(&tournament)).unwrap();

    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) {
                    h1 {
                        "Edit configuration for " (tournament.name)
                    }

                    form method="post" {
                        div class="mb-3" {
                            textarea name="config" style="resize: both;" rows="25" cols="80" {
                                (config)
                            }
                        }
                        button type="submit" class="btn btn-primary" {
                            "Submit"
                        }
                    }
                }
            })
            .render(),
    )
}

#[derive(FromForm)]
pub struct UpdateConfigForm {
    config: String,
}

#[post("/tournaments/<tournament_id>/configuration", data = "<form>")]
pub async fn update_tournament_configuration(
    tournament_id: &str,
    user: User<true>,
    mut conn: Conn<true>,
    form: Form<UpdateConfigForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    // todo: check if configuration change is incompatible with existing data
    // (for example, if there are ballots with a different format, these must
    // be deleted first!)

    let new_config = match toml::from_str::<TournamentConfig>(&form.config) {
        Ok(config) => config,
        Err(err) => {
            return bad_request(Page::new().user(user).tournament(tournament).body(maud! {
                "Error (the provided configuration file you provided is incorrect): " (err.to_string())
            }).render())
        }
    };

    if !["consensus", "individual"]
        .contains(&new_config.elim_ballot_setup.as_str())
    {
        return bad_request(Page::new().user(user).tournament(tournament).body(maud! {
            "Error: `elim_ballot_setup` should be one of 'individual' or 'consensus'."
            "You supplied " (new_config.elim_ballot_setup)
        }).render());
    }

    if !["consensus", "individual"]
        .contains(&new_config.pool_ballot_setup.as_str())
    {
        return bad_request(Page::new().user(user).tournament(tournament).body(maud! {
            "Error: `pool_ballot_setup` should be one of 'individual' or 'consensus'."
            "You supplied " (new_config.pool_ballot_setup)
        }).render());
    }

    let n = diesel::update(
        tournaments::table.filter(tournaments::id.eq(&tournament.id)),
    )
    .set((
        tournaments::team_tab_public.eq(new_config.team_tab_public),
        tournaments::speaker_tab_public.eq(new_config.speaker_tab_public),
        tournaments::teams_per_side.eq(new_config.teams_per_side),
        tournaments::substantive_speakers.eq(new_config.substantive_speakers),
        tournaments::reply_speakers.eq(new_config.reply_speakers),
        tournaments::reply_must_speak.eq(new_config.reply_must_speak),
        tournaments::max_substantive_speech_index_for_reply
            .eq(new_config.max_substantive_speech_index_for_reply),
        tournaments::pool_ballot_setup.eq(new_config.pool_ballot_setup),
        tournaments::elim_ballot_setup.eq(new_config.elim_ballot_setup),
        tournaments::elim_ballots_require_speaks
            .eq(new_config.elim_ballots_require_speaks),
        tournaments::institution_penalty.eq(new_config.institution_penalty),
        tournaments::history_penalty.eq(new_config.history_penalty),
        tournaments::pullup_metrics.eq(new_config.pullup_metrics),
        tournaments::repeat_pullup_penalty.eq(new_config.repeat_pullup_penalty),
        tournaments::team_standings_metrics
            .eq(new_config.team_standings_metrics),
        tournaments::speaker_standings_metrics
            .eq(new_config.speaker_standings_metrics),
        tournaments::exclude_from_speaker_standings_after
            .eq(new_config.exclude_from_speaker_standings_after),
    ))
    .execute(&mut *conn)
    .unwrap();
    assert_eq!(n, 1);

    see_other_ok(Redirect::to(format!("/tournaments/{}", &tournament.id)))
}
