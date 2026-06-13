// TODO: this function needs a lot of work. Current issues include
// - no way to see what preset has currently been applied
// - only oblique "bad configuration" error when an invalid configuration is
//   applied
// - we need a function update (config, tournament) that
//      - checks whether the given configuration can be applied to this
//        tournament
//      - if not, reports what needs to be changed to make this possible

use axum::{
    extract::{Form, Path},
    response::Redirect,
};
use diesel::{SqliteConnection, prelude::*, result::DatabaseErrorKind};
use hypertext::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    auth::User,
    schema::{
        ballots, debates, rounds, speaker_scores_of_ballot, tournament_presets,
        tournaments,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        participants::TournamentParticipants,
        rounds::{
            TournamentRounds,
            ballots::{BallotBuilder, BallotMetadata, BallotRepr},
            draws::{DebateRepr, RoundDrawRepr},
        },
        standings::compute::refresh_saved_team_standings,
    },
    util_resp::{
        FailureResponse, StandardResponse, bad_request, see_other_ok, success,
    },
};

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// This struct is used to marshall the tournament configuration to and from
/// the TOML format the user supplies.
pub struct TournamentConfig {
    pub team_tab_public: bool,
    pub speaker_tab_public: bool,
    pub standings_public: bool,
    pub show_round_results: bool,
    pub show_draws: bool,
    pub teams_per_side: i64,
    pub substantive_speakers: i64,
    pub reply_speakers: bool,
    pub reply_must_speak: bool,
    pub max_substantive_speech_index_for_reply: Option<i64>,
    pub require_prelim_substantive_speaks: bool,
    pub require_prelim_speaker_order: bool,
    pub require_elim_substantive_speaks: bool,
    pub require_elim_speaker_order: bool,
    pub substantive_speech_min_speak: Option<f32>,
    pub substantive_speech_max_speak: Option<f32>,
    pub substantive_speech_step: f32,
    pub reply_speech_min_speak: Option<f32>,
    pub reply_speech_max_speak: Option<f32>,
    pub pool_ballot_setup: String,
    pub elim_ballot_setup: String,
    #[serde(default = "default_true")]
    pub margin_includes_dissenters: bool,
    pub require_elim_ballot_substantive_speaks: bool,
    pub institution_penalty: i64,
    pub history_penalty: i64,
    pub pullup_metrics: String,
    pub repeat_pullup_penalty: i64,
    pub team_standings_metrics: String,
    pub speaker_standings_metrics: String,
    pub exclude_from_speaker_standings_after: Option<i64>,
}

pub fn config_of_tournament(tournament: &Tournament) -> TournamentConfig {
    TournamentConfig {
        team_tab_public: tournament.team_tab_public,
        speaker_tab_public: tournament.speaker_tab_public,
        standings_public: tournament.standings_public,
        show_round_results: tournament.show_round_results,
        show_draws: tournament.show_draws,
        teams_per_side: tournament.teams_per_side,
        substantive_speakers: tournament.substantive_speakers,
        reply_speakers: tournament.reply_speakers,
        reply_must_speak: tournament.reply_must_speak,
        max_substantive_speech_index_for_reply: tournament
            .max_substantive_speech_index_for_reply,
        pool_ballot_setup: tournament.pool_ballot_setup.clone(),
        elim_ballot_setup: tournament.elim_ballot_setup.clone(),
        margin_includes_dissenters: tournament.margin_includes_dissenters,
        require_elim_ballot_substantive_speaks: tournament
            .require_elim_substantive_speaks,
        institution_penalty: tournament.institution_penalty,
        history_penalty: tournament.history_penalty,
        pullup_metrics: tournament.pullup_metrics.clone(),
        repeat_pullup_penalty: tournament.repeat_pullup_penalty,
        team_standings_metrics: tournament.team_standings_metrics.clone(),
        speaker_standings_metrics: tournament.speaker_standings_metrics.clone(),
        exclude_from_speaker_standings_after: tournament
            .exclude_from_speaker_standings_after,
        substantive_speech_min_speak: tournament.substantive_speech_min_speak,
        substantive_speech_max_speak: tournament.substantive_speech_max_speak,
        substantive_speech_step: tournament
            .substantive_speech_step
            .unwrap_or(0.5),
        reply_speech_min_speak: tournament.reply_speech_min_speak,
        reply_speech_max_speak: tournament.reply_speech_max_speak,
        require_prelim_substantive_speaks: tournament
            .require_prelim_substantive_speaks,
        require_prelim_speaker_order: tournament.require_prelim_speaker_order,
        require_elim_substantive_speaks: tournament
            .require_elim_substantive_speaks,
        require_elim_speaker_order: tournament.require_elim_speaker_order,
    }
}

#[derive(Queryable)]
pub struct TournamentPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub config: String,
}

fn parse_tournament_config(
    raw_config: &str,
    user: &User<true>,
    tournament: &Tournament,
) -> Result<TournamentConfig, FailureResponse> {
    let new_config = match toml::from_str::<TournamentConfig>(raw_config) {
        Ok(config) => config,
        Err(err) => {
            return Err(bad_request(Page::new().user(user.clone()).tournament(tournament.clone()).body(maud! {
                "Error (the provided configuration file you provided is incorrect): " (err.to_string())
            }).render()).unwrap_err())
        }
    };

    if !["consensus", "individual"]
        .contains(&new_config.elim_ballot_setup.as_str())
    {
        return Err(bad_request(Page::new().user(user.clone()).tournament(tournament.clone()).body(maud! {
            "Error: `elim_ballot_setup` should be one of 'individual' or 'consensus'. "
            "You supplied " (new_config.elim_ballot_setup)
        }).render()).unwrap_err());
    }

    if !["consensus", "individual"]
        .contains(&new_config.pool_ballot_setup.as_str())
    {
        return Err(bad_request(Page::new().user(user.clone()).tournament(tournament.clone()).body(maud! {
            "Error: `pool_ballot_setup` should be one of 'individual' or 'consensus'. "
            "You supplied " (new_config.pool_ballot_setup)
        }).render()).unwrap_err());
    }

    Ok(new_config)
}

fn tournament_with_config(
    tournament: &Tournament,
    config: &TournamentConfig,
) -> Tournament {
    let mut candidate = tournament.clone();
    candidate.team_tab_public = config.team_tab_public;
    candidate.speaker_tab_public = config.speaker_tab_public;
    candidate.standings_public = config.standings_public;
    candidate.show_round_results = config.show_round_results;
    candidate.show_draws = config.show_draws;
    candidate.teams_per_side = config.teams_per_side;
    candidate.substantive_speakers = config.substantive_speakers;
    candidate.reply_speakers = config.reply_speakers;
    candidate.reply_must_speak = config.reply_must_speak;
    candidate.max_substantive_speech_index_for_reply =
        config.max_substantive_speech_index_for_reply;
    candidate.pool_ballot_setup = config.pool_ballot_setup.clone();
    candidate.elim_ballot_setup = config.elim_ballot_setup.clone();
    candidate.margin_includes_dissenters = config.margin_includes_dissenters;
    candidate.institution_penalty = config.institution_penalty;
    candidate.history_penalty = config.history_penalty;
    candidate.pullup_metrics = config.pullup_metrics.clone();
    candidate.repeat_pullup_penalty = config.repeat_pullup_penalty;
    candidate.team_standings_metrics = config.team_standings_metrics.clone();
    candidate.speaker_standings_metrics =
        config.speaker_standings_metrics.clone();
    candidate.exclude_from_speaker_standings_after =
        config.exclude_from_speaker_standings_after;
    candidate.substantive_speech_min_speak =
        config.substantive_speech_min_speak;
    candidate.substantive_speech_max_speak =
        config.substantive_speech_max_speak;
    candidate.substantive_speech_step = Some(config.substantive_speech_step);
    candidate.reply_speech_min_speak = config.reply_speech_min_speak;
    candidate.reply_speech_max_speak = config.reply_speech_max_speak;
    candidate.require_prelim_substantive_speaks =
        config.require_prelim_substantive_speaks;
    candidate.require_prelim_speaker_order =
        config.require_prelim_speaker_order;
    candidate.require_elim_substantive_speaks =
        config.require_elim_substantive_speaks;
    candidate.require_elim_speaker_order = config.require_elim_speaker_order;
    candidate
}

fn validate_config_in_isolation(config: &TournamentConfig) -> Vec<String> {
    let mut problems = Vec::new();

    if config.reply_speakers && config.teams_per_side != 1 {
        problems.push(
            "`reply_speakers` is only supported when `teams_per_side = 1`."
                .to_string(),
        );
    }

    if config.require_prelim_substantive_speaks
        && !config.require_prelim_speaker_order
    {
        problems.push(
            "`require_prelim_substantive_speaks` requires \
             `require_prelim_speaker_order`."
                .to_string(),
        );
    }

    if config.require_elim_substantive_speaks
        && !config.require_elim_speaker_order
    {
        problems.push(
            "`require_elim_substantive_speaks` requires \
             `require_elim_speaker_order`."
                .to_string(),
        );
    }

    if let (Some(min), Some(max)) = (
        config.substantive_speech_min_speak,
        config.substantive_speech_max_speak,
    ) {
        if min > max {
            problems.push(
                "`substantive_speech_min_speak` cannot be greater than \
                 `substantive_speech_max_speak`."
                    .to_string(),
            );
        }
    }

    if config.substantive_speech_step <= 0.0 {
        problems.push(
            "`substantive_speech_step` must be greater than zero.".to_string(),
        );
    }

    if let (Some(min), Some(max)) =
        (config.reply_speech_min_speak, config.reply_speech_max_speak)
    {
        if min > max {
            problems.push(
                "`reply_speech_min_speak` cannot be greater than \
                 `reply_speech_max_speak`."
                    .to_string(),
            );
        }
    }

    problems
}

fn has_round_data(tournament_id: &str, conn: &mut SqliteConnection) -> bool {
    debates::table
        .filter(debates::tournament_id.eq(tournament_id))
        .count()
        .get_result::<i64>(conn)
        .unwrap()
        > 0
}

fn has_ballots_for_round_kind(
    tournament_id: &str,
    kind: &str,
    conn: &mut SqliteConnection,
) -> bool {
    ballots::table
        .inner_join(debates::table.on(ballots::debate_id.eq(debates::id)))
        .inner_join(rounds::table.on(debates::round_id.eq(rounds::id)))
        .filter(ballots::tournament_id.eq(tournament_id))
        .filter(rounds::kind.eq(kind))
        .count()
        .get_result::<i64>(conn)
        .unwrap()
        > 0
}

fn has_speaker_scores(
    tournament_id: &str,
    conn: &mut SqliteConnection,
) -> bool {
    speaker_scores_of_ballot::table
        .filter(speaker_scores_of_ballot::tournament_id.eq(tournament_id))
        .count()
        .get_result::<i64>(conn)
        .unwrap()
        > 0
}

fn validate_existing_data_shape(
    tournament: &Tournament,
    candidate: &Tournament,
    conn: &mut SqliteConnection,
) -> Vec<String> {
    let mut problems = Vec::new();

    if tournament.teams_per_side != candidate.teams_per_side
        && has_round_data(&tournament.id, conn)
    {
        problems.push(
            "`teams_per_side` cannot be changed while draws exist.".to_string(),
        );
    }

    if (tournament.substantive_speakers != candidate.substantive_speakers
        || tournament.reply_speakers != candidate.reply_speakers)
        && has_speaker_scores(&tournament.id, conn)
    {
        problems.push(
            "`substantive_speakers` and `reply_speakers` cannot be changed \
             while speaker score ballots exist."
                .to_string(),
        );
    }

    if (tournament.require_prelim_substantive_speaks
        != candidate.require_prelim_substantive_speaks
        || tournament.require_prelim_speaker_order
            != candidate.require_prelim_speaker_order
        || tournament.pool_ballot_setup != candidate.pool_ballot_setup)
        && has_ballots_for_round_kind(&tournament.id, "P", conn)
    {
        problems.push(
            "Preliminary ballot format settings cannot be changed while \
             preliminary ballots exist."
                .to_string(),
        );
    }

    if (tournament.require_elim_substantive_speaks
        != candidate.require_elim_substantive_speaks
        || tournament.require_elim_speaker_order
            != candidate.require_elim_speaker_order
        || tournament.elim_ballot_setup != candidate.elim_ballot_setup)
        && has_ballots_for_round_kind(&tournament.id, "E", conn)
    {
        problems.push(
            "Elimination ballot format settings cannot be changed while \
             elimination ballots exist."
                .to_string(),
        );
    }

    problems
}

fn validate_debate_has_candidate_shape(
    candidate: &Tournament,
    debate: &DebateRepr,
) -> Result<(), String> {
    for seq in 0..candidate.teams_per_side {
        for side in 0..2 {
            if !debate
                .teams_of_debate
                .iter()
                .any(|team| team.side == side && team.seq == seq)
            {
                return Err(format!(
                    "draw does not contain a team at side {side}, position {seq}"
                ));
            }
        }
    }
    Ok(())
}

fn revalidate_ballot_with_candidate_config(
    ballot: &BallotRepr,
    candidate: &Tournament,
    round: &crate::tournaments::rounds::Round,
    debate: &DebateRepr,
    participants: &TournamentParticipants,
    conn: &mut SqliteConnection,
) -> Result<(), String> {
    let metadata = BallotMetadata {
        id: ballot.metadata.id.clone(),
        tournament_id: ballot.metadata.tournament_id.clone(),
        debate_id: ballot.metadata.debate_id.clone(),
        judge_id: ballot.metadata.judge_id.clone(),
        submitted_at: ballot.metadata.submitted_at,
        motion_id: ballot.metadata.motion_id.clone(),
        version: 0,
        change: ballot.metadata.change.clone(),
        editor_id: ballot.metadata.editor_id.clone(),
    };

    let mut builder = BallotBuilder::new(
        candidate,
        debate,
        round,
        participants,
        metadata,
        0,
        0,
        false,
        conn,
    )?;

    for seq in 0..candidate.teams_per_side {
        for side in 0..2 {
            let debate_team = debate.team_of_side_and_seq(side, seq);
            let speakers = if candidate.round_requires_speaker_order(round) {
                ballot
                    .scores_of_team(&debate_team.team_id)
                    .into_iter()
                    .map(|score| (score.speaker_id, score.score))
                    .collect()
            } else {
                Vec::new()
            };
            let points = ballot
                .team_ranks
                .iter()
                .find(|rank| rank.team_id == debate_team.team_id)
                .map(|rank| rank.points as usize);

            builder.add_team(side as usize, seq as usize, speakers, points)?;
        }
    }

    builder.build()?;
    Ok(())
}

fn validate_existing_ballots_with_candidate_config(
    candidate: &Tournament,
    conn: &mut SqliteConnection,
) -> Vec<String> {
    let mut problems = Vec::new();
    let participants = TournamentParticipants::load(&candidate.id, conn);
    let tournament_rounds = match TournamentRounds::fetch(&candidate.id, conn) {
        Ok(rounds) => rounds,
        Err(err) => {
            return vec![format!(
                "Could not load tournament rounds for compatibility checks: {err}."
            )];
        }
    };

    for round in tournament_rounds
        .prelim
        .into_iter()
        .chain(tournament_rounds.elim.into_iter())
    {
        let draw = RoundDrawRepr::of_round(round.clone(), conn);
        for debate in draw.debates {
            if let Err(err) =
                validate_debate_has_candidate_shape(candidate, &debate)
            {
                problems.push(format!(
                    "Round {}, debate {}: {err}.",
                    round.name, debate.debate.number
                ));
                continue;
            }

            let ballots = debate.latest_ballots(conn);
            for ballot in &ballots {
                if let Err(err) = revalidate_ballot_with_candidate_config(
                    ballot,
                    candidate,
                    &round,
                    &debate,
                    &participants,
                    conn,
                ) {
                    let judge_name = debate
                        .judges
                        .get(&ballot.metadata.judge_id)
                        .map(|judge| judge.name.as_str())
                        .unwrap_or("unknown judge");
                    problems.push(format!(
                        "Round {}, debate {}, ballot from {}: {err}.",
                        round.name, debate.debate.number, judge_name
                    ));
                }
            }

            let ballot_set_problems =
                BallotRepr::problems_of_set(&ballots, candidate, &debate);
            for problem in ballot_set_problems {
                problems.push(format!(
                    "Round {}, debate {}: {problem}",
                    round.name, debate.debate.number
                ));
            }
        }
    }

    problems
}

fn validate_tournament_config_update(
    tournament: &Tournament,
    new_config: &TournamentConfig,
    conn: &mut SqliteConnection,
) -> Result<Tournament, Vec<String>> {
    let mut problems = validate_config_in_isolation(new_config);
    let candidate = tournament_with_config(tournament, new_config);

    if problems.is_empty() {
        problems
            .extend(validate_existing_data_shape(tournament, &candidate, conn));
    }

    if problems.is_empty() {
        problems.extend(validate_existing_ballots_with_candidate_config(
            &candidate, conn,
        ));
    }

    if problems.is_empty() {
        Ok(candidate)
    } else {
        Err(problems)
    }
}

fn config_validation_error_response(
    user: &User<true>,
    tournament: &Tournament,
    problems: Vec<String>,
) -> FailureResponse {
    FailureResponse::BadRequest(
        Page::new()
            .user(user.clone())
            .tournament(tournament.clone())
            .body(maud! {
                h1 { "Configuration cannot be applied" }
                p {
                    "The new configuration is incompatible with the current tournament data."
                }
                ul {
                    @for problem in &problems {
                        li { (problem) }
                    }
                }
            })
            .render(),
    )
}

fn config_update_error_response(
    user: &User<true>,
    tournament: &Tournament,
    err: diesel::result::Error,
) -> FailureResponse {
    match err {
        diesel::result::Error::DatabaseError(
            DatabaseErrorKind::CheckViolation,
            _,
        ) => FailureResponse::BadRequest(
            Page::new()
                .user(user.clone())
                .tournament(tournament.clone())
                .body(maud! {
                    "Error: bad configuration provided."
                })
                .render(),
        ),
        _ => FailureResponse::ServerError(()),
    }
}

fn apply_tournament_config(
    tournament: &Tournament,
    new_config: TournamentConfig,
    conn: &mut SqliteConnection,
) -> Result<(), diesel::result::Error> {
    let n = diesel::update(
        tournaments::table.filter(tournaments::id.eq(&tournament.id)),
    )
    .set((
        tournaments::team_tab_public.eq(new_config.team_tab_public),
        tournaments::speaker_tab_public.eq(new_config.speaker_tab_public),
        tournaments::standings_public.eq(new_config.standings_public),
        tournaments::show_round_results.eq(new_config.show_round_results),
        tournaments::show_draws.eq(new_config.show_draws),
        tournaments::teams_per_side.eq(new_config.teams_per_side),
        tournaments::substantive_speakers.eq(new_config.substantive_speakers),
        tournaments::reply_speakers.eq(new_config.reply_speakers),
        tournaments::reply_must_speak.eq(new_config.reply_must_speak),
        tournaments::max_substantive_speech_index_for_reply
            .eq(new_config.max_substantive_speech_index_for_reply),
        tournaments::pool_ballot_setup.eq(new_config.pool_ballot_setup),
        tournaments::elim_ballot_setup.eq(new_config.elim_ballot_setup),
        tournaments::margin_includes_dissenters
            .eq(new_config.margin_includes_dissenters),
        tournaments::require_elim_substantive_speaks
            .eq(new_config.require_elim_ballot_substantive_speaks),
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
        tournaments::substantive_speech_min_speak
            .eq(new_config.substantive_speech_min_speak),
        tournaments::substantive_speech_max_speak
            .eq(new_config.substantive_speech_max_speak),
        tournaments::substantive_speech_step
            .eq(Some(new_config.substantive_speech_step)),
        tournaments::reply_speech_min_speak
            .eq(new_config.reply_speech_min_speak),
        tournaments::reply_speech_max_speak
            .eq(new_config.reply_speech_max_speak),
        tournaments::require_prelim_substantive_speaks
            .eq(new_config.require_prelim_substantive_speaks),
        tournaments::require_prelim_speaker_order
            .eq(new_config.require_prelim_speaker_order),
        tournaments::require_elim_substantive_speaks
            .eq(new_config.require_elim_substantive_speaks),
        tournaments::require_elim_speaker_order
            .eq(new_config.require_elim_speaker_order),
    ))
    .execute(conn)?;
    assert_eq!(n, 1);
    Ok(())
}

pub async fn view_tournament_configuration(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let presets = tournament_presets::table
        .order(tournament_presets::name.asc())
        .load::<TournamentPreset>(&mut *conn)
        .unwrap();

    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let current_rounds = crate::tournaments::rounds::Round::current_rounds(
        &tournament.id,
        &mut *conn,
    );

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) active_page=(None) selected_seq=(None) {
                    h1 {
                        "Edit configuration for " (tournament.name)
                    }

                    p class="text-secondary" {
                        "Start from a global preset, or provide a custom TOML configuration."
                    }

                    h2 class="h4 mt-4" { "Global presets" }
                    table class="table align-middle" {
                        thead {
                            tr {
                                th scope="col" { "Preset" }
                                th scope="col" { "Description" }
                                th scope="col" { "" }
                            }
                        }
                        tbody {
                            @for preset in &presets {
                                tr {
                                    td class="fw-semibold" { (preset.name) }
                                    td { (preset.description) }
                                    td class="text-end" {
                                        form method="post" action=(format!("/tournaments/{}/configuration/preset", tournament.id)) {
                                            input type="hidden" name="preset_id" value=(preset.id);
                                            button type="submit" class="btn btn-outline-primary btn-sm" {
                                                "Use preset"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    h2 class="h4 mt-5" { "Custom TOML" }
                    p class="text-secondary" {
                        "Edit the current tournament configuration directly in TOML."
                    }
                    a href=(format!("/tournaments/{}/configuration/custom", tournament.id)) class="btn btn-outline-secondary" {
                        "Edit TOML"
                    }
                }
            })
            .render(),
    )
}

pub async fn view_custom_tournament_configuration(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let config = toml::to_string(&config_of_tournament(&tournament)).unwrap();

    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let current_rounds = crate::tournaments::rounds::Round::current_rounds(
        &tournament.id,
        &mut *conn,
    );

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) active_page=(None) selected_seq=(None) {
                    h1 {
                        "Edit configuration for " (tournament.name)
                    }

                    form method="post" action=(format!("/tournaments/{}/configuration", tournament.id)) {
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

#[derive(Deserialize)]
pub struct UpdateConfigForm {
    config: String,
}

#[derive(Deserialize)]
pub struct ApplyPresetConfigForm {
    preset_id: String,
}

pub async fn update_tournament_configuration(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<UpdateConfigForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let new_config = parse_tournament_config(&form.config, &user, &tournament)?;

    validate_tournament_config_update(&tournament, &new_config, &mut *conn)
        .map_err(|problems| {
            config_validation_error_response(&user, &tournament, problems)
        })?;

    apply_tournament_config(&tournament, new_config, &mut *conn)
        .map_err(|err| config_update_error_response(&user, &tournament, err))?;
    refresh_saved_team_standings(&tournament.id, &mut *conn)?;

    see_other_ok(Redirect::to(&format!("/tournaments/{}", &tournament.id)))
}

pub async fn apply_tournament_preset(
    Path(tournament_id): Path<String>,
    user: User<true>,
    mut conn: Conn<true>,
    Form(form): Form<ApplyPresetConfigForm>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let preset = tournament_presets::table
        .filter(tournament_presets::id.eq(form.preset_id))
        .first::<TournamentPreset>(&mut *conn)?;
    let new_config =
        parse_tournament_config(&preset.config, &user, &tournament)?;
    validate_tournament_config_update(&tournament, &new_config, &mut *conn)
        .map_err(|problems| {
            config_validation_error_response(&user, &tournament, problems)
        })?;
    apply_tournament_config(&tournament, new_config, &mut *conn)
        .map_err(|err| config_update_error_response(&user, &tournament, err))?;
    refresh_saved_team_standings(&tournament.id, &mut *conn)?;

    see_other_ok(Redirect::to(&format!("/tournaments/{}", &tournament.id)))
}
