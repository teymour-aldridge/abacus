//! Fuzzcheck tournament simulation property checks.

#![allow(dead_code)]

use fuzzcheck::DefaultMutator;
use serde::{Deserialize, Serialize};

use crate::tournamentsim::inputs::Action;

mod assertions;
mod harness;
mod inputs;

#[allow(dead_code)]
#[derive(DefaultMutator, Clone, Debug, Hash, Serialize, Deserialize)]
pub struct WorkloadInput {
    actions: Vec<Action>,
}

#[cfg(fuzzing)]
#[test]
pub fn fuzz() {
    let result = fuzzcheck::fuzz_test(|input: &WorkloadInput| {
        harness::run_workload(input);
    })
    .default_mutator()
    .serde_serializer()
    .default_sensor_and_pool()
    .arguments_from_cargo_fuzzcheck()
    .launch();

    assert!(!result.found_test_failure);
}

#[test]
fn team_standings_are_refreshed_when_completed_rounds_or_teams_change() {
    for actions in [
        vec![
            Action::RegisterUser {
                username: "bool".to_string(),
                email: "d@q.edu".to_string(),
                password: "Igecko".to_string(),
            },
            Action::CreateTournament {
                name: "ibex".to_string(),
                abbrv: "upZ".to_string(),
                slug: String::new(),
            },
            Action::CreateRound {
                tournament_idx: 65,
                name: "otter".to_string(),
                category_idx: None,
                seq: 366268609,
            },
            Action::CreateMotion {
                tournament_idx: 65,
                round_idx: 0,
                motion: "otter".to_string(),
                infoslide: None,
            },
            Action::SetRoundCompleted {
                tournament_idx: 110,
                round_idx: 172,
                completed: true,
            },
            Action::CreateTeam {
                tournament_idx: 242,
                name: "true".to_string(),
                institution_idx: None,
            },
        ],
        vec![
            Action::RegisterUser {
                username: "bool".to_string(),
                email: "d@q.edu".to_string(),
                password: "Igecko".to_string(),
            },
            Action::CreateTournament {
                name: "lynx".to_string(),
                abbrv: "nw".to_string(),
                slug: String::new(),
            },
            Action::CreateRound {
                tournament_idx: 65,
                name: "lynx".to_string(),
                category_idx: None,
                seq: 366268609,
            },
            Action::CreateMotion {
                tournament_idx: 65,
                round_idx: 0,
                motion: "lynx".to_string(),
                infoslide: None,
            },
            Action::CreateTeam {
                tournament_idx: 242,
                name: "true".to_string(),
                institution_idx: None,
            },
            Action::SetRoundCompleted {
                tournament_idx: 110,
                round_idx: 172,
                completed: true,
            },
        ],
    ] {
        harness::run_workload(&WorkloadInput { actions });
    }
}

#[test]
fn room_creation_accepts_fuzzer_priority_extremes() {
    harness::run_workload(&WorkloadInput {
        actions: vec![
            Action::RegisterUser {
                username: "otter".to_string(),
                email: "u@y.edu".to_string(),
                password: "badger".to_string(),
            },
            Action::CreateTournament {
                name: "down".to_string(),
                abbrv: "gecko".to_string(),
                slug: "v".to_string(),
            },
            Action::CreateRoom {
                tournament_idx: 236,
                name: "s".to_string(),
                priority: -4265215526239750768,
            },
        ],
    });
}

#[test]
fn duplicate_room_constraints_are_idempotent() {
    harness::run_workload(&WorkloadInput {
        actions: vec![
            Action::RegisterUser {
                username: "lynx".to_string(),
                email: "d@r.com".to_string(),
                password: "badger".to_string(),
            },
            Action::CreateTournament {
                name: "true".to_string(),
                abbrv: "on".to_string(),
                slug: String::new(),
            },
            Action::CreateJudge {
                tournament_idx: 73,
                name: String::new(),
                email: "o@h.com".to_string(),
                institution_idx: None,
            },
            Action::CreateRoomCategory {
                tournament_idx: 81,
                name: None,
                private_name: String::new(),
                public_name: String::new(),
                description: String::new(),
            },
            Action::AddConstraint {
                tournament_idx: 129,
                ptype: "judge".to_string(),
                pid_idx: 164,
                category_idx: 153,
            },
            Action::AddConstraint {
                tournament_idx: 129,
                ptype: "judge".to_string(),
                pid_idx: 164,
                category_idx: 153,
            },
        ],
    });
}

#[test]
fn applying_preset_then_viewing_team_tab_does_not_error() {
    harness::run_workload(&WorkloadInput {
        actions: vec![
            Action::RegisterUser {
                username: "*on".to_string(),
                email: "d@r.com".to_string(),
                password: "badger".to_string(),
            },
            Action::CreateTournament {
                name: "true".to_string(),
                abbrv: "on".to_string(),
                slug: String::new(),
            },
            Action::ApplyTournamentPreset {
                tournament_idx: 6,
                preset_idx: 113,
            },
            Action::ViewTournamentPage {
                tournament_idx: 0,
                round_idx: 171,
                page_idx: 122,
            },
        ],
    });
}

#[test]
fn briefing_room_handles_new_round_without_draw() {
    harness::run_workload(&WorkloadInput {
        actions: vec![
            Action::RegisterUser {
                username: "*on".to_string(),
                email: "d@r.com".to_string(),
                password: "badger".to_string(),
            },
            Action::CreateTournament {
                name: "true".to_string(),
                abbrv: "on".to_string(),
                slug: String::new(),
            },
            Action::CreateRound {
                tournament_idx: 81,
                name: "otter".to_string(),
                category_idx: None,
                seq: 4043524234,
            },
            Action::ViewTournamentPage {
                tournament_idx: 136,
                round_idx: 95,
                page_idx: 61,
            },
        ],
    });
}

#[test]
fn fuzz_regression_c7b6bbb74ff5c56() {
    harness::run_workload(&WorkloadInput {
        actions: vec![
            Action::RegisterUser {
                username: "out".to_string(),
                email: "l@u.org".to_string(),
                password: "badger".to_string(),
            },
            Action::CreateTournament {
                name: "ibex".to_string(),
                abbrv: ",l".to_string(),
                slug: String::new(),
            },
            Action::CreateRound {
                tournament_idx: 148,
                name: "lynx".to_string(),
                category_idx: None,
                seq: 4019972498,
            },
            Action::CreateMotion {
                tournament_idx: 148,
                round_idx: 0,
                motion: "lynx".to_string(),
                infoslide: None,
            },
            Action::CreateTeam {
                tournament_idx: 125,
                name: "Xout".to_string(),
                institution_idx: None,
            },
            Action::CreateSpeaker {
                tournament_idx: 125,
                team_idx: 0,
                name: "speaker".to_string(),
                email: "s0@u.org".to_string(),
            },
            Action::CreateTeam {
                tournament_idx: 125,
                name: "Xout".to_string(),
                institution_idx: None,
            },
            Action::CreateSpeaker {
                tournament_idx: 125,
                team_idx: 1,
                name: "speaker".to_string(),
                email: "s1@u.org".to_string(),
            },
            Action::CreateTeam {
                tournament_idx: 125,
                name: "Xout".to_string(),
                institution_idx: None,
            },
            Action::CreateSpeaker {
                tournament_idx: 125,
                team_idx: 2,
                name: "speaker".to_string(),
                email: "s2@u.org".to_string(),
            },
            Action::CreateTeam {
                tournament_idx: 164,
                name: "lynx".to_string(),
                institution_idx: None,
            },
            Action::CreateSpeaker {
                tournament_idx: 164,
                team_idx: 3,
                name: "speaker".to_string(),
                email: "s3@u.org".to_string(),
            },
            Action::UpdateAllTeamEligibility {
                tournament_idx: 182,
                round_idx: 64,
                eligible: true,
            },
            Action::GenerateDraw {
                tournament_idx: 134,
                round_idx: 101,
                force: false,
            },
            Action::SetRoundCompleted {
                tournament_idx: 236,
                round_idx: 23,
                completed: true,
            },
            Action::CreateJudge {
                tournament_idx: 153,
                name: String::new(),
                email: "v@g.org".to_string(),
                institution_idx: None,
            },
            Action::MoveJudge {
                tournament_idx: 130,
                round_idx: 136,
                judge_idx: 241,
                debate_idx: 81,
                role: String::new(),
                assign: true,
            },
            Action::ViewTournamentPage {
                tournament_idx: 244,
                round_idx: 153,
                page_idx: 185,
            },
        ],
    });
}

#[test]
fn fuzz_regression_dda7db76ef12f900() {
    harness::run_workload(&WorkloadInput {
        actions: vec![
            Action::RegisterUser {
                username: "kup".to_string(),
                email: "d@r.com".to_string(),
                password: "badger".to_string(),
            },
            Action::CreateTournament {
                name: "true".to_string(),
                abbrv: "on".to_string(),
                slug: String::new(),
            },
            Action::CreateRound {
                tournament_idx: 193,
                name: "down".to_string(),
                category_idx: None,
                seq: 3958213056,
            },
            Action::CreateMotion {
                tournament_idx: 193,
                round_idx: 0,
                motion: "down".to_string(),
                infoslide: None,
            },
            Action::SetRoundCompleted {
                tournament_idx: 133,
                round_idx: 159,
                completed: true,
            },
            Action::CreateTeam {
                tournament_idx: 160,
                name: "lynx".to_string(),
                institution_idx: None,
            },
            Action::ViewTournamentPage {
                tournament_idx: 112,
                round_idx: 157,
                page_idx: 174,
            },
        ],
    });
}

#[test]
fn fuzz_regression_bee2ecfb0de2efff() {
    harness::run_workload(&WorkloadInput {
        actions: vec![
            Action::RegisterUser {
                username: "on*".to_string(),
                email: "d@r.com".to_string(),
                password: "badger".to_string(),
            },
            Action::CreateTournament {
                name: "true".to_string(),
                abbrv: "on".to_string(),
                slug: String::new(),
            },
            Action::CreateRound {
                tournament_idx: 193,
                name: "down".to_string(),
                category_idx: None,
                seq: 3958213056,
            },
            Action::CreateMotion {
                tournament_idx: 193,
                round_idx: 0,
                motion: "down".to_string(),
                infoslide: None,
            },
            Action::SetRoundCompleted {
                tournament_idx: 133,
                round_idx: 159,
                completed: true,
            },
            Action::CreateTeam {
                tournament_idx: 160,
                name: "lynx".to_string(),
                institution_idx: None,
            },
            Action::ApplyTournamentPreset {
                tournament_idx: 122,
                preset_idx: 179,
            },
        ],
    });
}

#[test]
fn fuzz_regression_5e3a4cdb7c310a40() {
    harness::run_workload(&WorkloadInput {
        actions: vec![
            Action::RegisterUser {
                username: "out".to_string(),
                email: "l@u.org".to_string(),
                password: "badger".to_string(),
            },
            Action::CreateTournament {
                name: "ibex".to_string(),
                abbrv: ",l".to_string(),
                slug: "".to_string(),
            },
            Action::CreateJudge {
                tournament_idx: 153,
                name: "".to_string(),
                email: "d@l.org".to_string(),
                institution_idx: None,
            },
            Action::CreateRoomCategory {
                tournament_idx: 123,
                name: None,
                private_name: "".to_string(),
                public_name: "".to_string(),
                description: "".to_string(),
            },
            Action::AddConstraint {
                tournament_idx: 245,
                ptype: "judge".to_string(),
                pid_idx: 156,
                category_idx: 138,
            },
            Action::CreateRoomCategory {
                tournament_idx: 123,
                name: None,
                private_name: "".to_string(),
                public_name: "".to_string(),
                description: "".to_string(),
            },
            Action::AddConstraint {
                tournament_idx: 245,
                ptype: "judge".to_string(),
                pid_idx: 156,
                category_idx: 138,
            },
            Action::MoveConstraint {
                tournament_idx: 188,
                ptype: "".to_string(),
                pid_idx: 82,
                constraint_idx: 194,
                up: false,
            },
        ],
    });
}

#[test]
fn release_full_draw_requires_motion() {
    harness::run_workload(&WorkloadInput {
        actions: vec![
            Action::RegisterUser {
                username: "lynx".to_string(),
                email: "g@r.org".to_string(),
                password: "badger".to_string(),
            },
            Action::CreateTournament {
                name: "ibex".to_string(),
                abbrv: ",l".to_string(),
                slug: String::new(),
            },
            Action::CreateRound {
                tournament_idx: 0,
                name: "lynx".to_string(),
                category_idx: None,
                seq: 4019972498,
            },
            Action::SetDrawPublished {
                tournament_idx: 0,
                round_idx: 0,
                status: "released_full".to_string(),
                published: None,
            },
        ],
    });
}

#[test]
fn fuzz_regression_7ac30aecdb5ffdaf() {
    harness::run_workload(&WorkloadInput {
        actions: vec![
            Action::RegisterUser {
                username: "out".to_string(),
                email: "l@u.org".to_string(),
                password: "badger".to_string(),
            },
            Action::CreateTournament {
                name: "ibex".to_string(),
                abbrv: ",l".to_string(),
                slug: String::new(),
            },
            Action::CreateRoom {
                tournament_idx: 97,
                name: String::new(),
                priority: 157038913731170864,
            },
            Action::CreateRound {
                tournament_idx: 148,
                name: "lynx".to_string(),
                category_idx: None,
                seq: 4019972498,
            },
            Action::CreateMotion {
                tournament_idx: 148,
                round_idx: 0,
                motion: "lynx".to_string(),
                infoslide: None,
            },
            Action::CreateTeam {
                tournament_idx: 125,
                name: "Xout".to_string(),
                institution_idx: None,
            },
            Action::CreateTeam {
                tournament_idx: 125,
                name: "Xout".to_string(),
                institution_idx: None,
            },
            Action::CreateTeam {
                tournament_idx: 125,
                name: "Xout".to_string(),
                institution_idx: None,
            },
            Action::CreateTeam {
                tournament_idx: 164,
                name: "lynx".to_string(),
                institution_idx: None,
            },
            Action::UpdateAllTeamEligibility {
                tournament_idx: 182,
                round_idx: 64,
                eligible: true,
            },
            Action::GenerateDraw {
                tournament_idx: 142,
                round_idx: 67,
                force: true,
            },
            Action::MoveRoom {
                tournament_idx: 46,
                round_idx: 189,
                room_idx: 150,
                debate_idx: 106,
                assign: true,
            },
            Action::CreateJudge {
                tournament_idx: 153,
                name: String::new(),
                email: "d@l.org".to_string(),
                institution_idx: None,
            },
            Action::CreateRoomCategory {
                tournament_idx: 123,
                name: None,
                private_name: String::new(),
                public_name: String::new(),
                description: String::new(),
            },
            Action::AddConstraint {
                tournament_idx: 245,
                ptype: "judge".to_string(),
                pid_idx: 156,
                category_idx: 138,
            },
            Action::DeleteRoomCategory {
                tournament_idx: 0,
                category_idx: 85,
            },
            Action::ViewTournamentPage {
                tournament_idx: 83,
                round_idx: 8,
                page_idx: 225,
            },
            Action::MoveJudge {
                tournament_idx: 130,
                round_idx: 136,
                judge_idx: 241,
                debate_idx: 81,
                role: String::new(),
                assign: true,
            },
            Action::ViewTournamentPage {
                tournament_idx: 233,
                round_idx: 160,
                page_idx: 70,
            },
            Action::CreateRound {
                tournament_idx: 148,
                name: "lynx".to_string(),
                category_idx: None,
                seq: 4019972498,
            },
            Action::CreateMotion {
                tournament_idx: 148,
                round_idx: 0,
                motion: "lynx".to_string(),
                infoslide: None,
            },
            Action::ViewTournamentPage {
                tournament_idx: 119,
                round_idx: 40,
                page_idx: 97,
            },
            Action::SetDrawPublished {
                tournament_idx: 178,
                round_idx: 159,
                status: "d".to_string(),
                published: Some(true),
            },
            Action::SetRoundCompleted {
                tournament_idx: 236,
                round_idx: 23,
                completed: true,
            },
        ],
    });
}

#[test]
fn fuzz_regression_a633fc1777f8b51f() {
    harness::run_workload(&WorkloadInput {
        actions: vec![
            Action::RegisterUser {
                username: "otter".to_string(),
                email: "g@r.org".to_string(),
                password: "badger".to_string(),
            },
            Action::CreateTournament {
                name: "ibex".to_string(),
                abbrv: ",l".to_string(),
                slug: String::new(),
            },
            Action::CreateRound {
                tournament_idx: 148,
                name: "lynx".to_string(),
                category_idx: None,
                seq: 4019972498,
            },
            Action::CreateTeam {
                tournament_idx: 125,
                name: "Xout".to_string(),
                institution_idx: None,
            },
            Action::CreateTeam {
                tournament_idx: 125,
                name: "Xout".to_string(),
                institution_idx: None,
            },
            Action::CreateTeam {
                tournament_idx: 125,
                name: "Xout".to_string(),
                institution_idx: None,
            },
            Action::CreateTeam {
                tournament_idx: 164,
                name: "lynx".to_string(),
                institution_idx: None,
            },
            Action::UpdateAllTeamEligibility {
                tournament_idx: 182,
                round_idx: 64,
                eligible: true,
            },
            Action::UpdateTournamentConfiguration {
                tournament_idx: 32,
                name: None,
                abbrv: None,
                slug: None,
                show_draws: true,
                show_round_results: false,
                team_tab_public: true,
                speaker_tab_public: false,
                standings_public: true,
                require_prelim_substantive_speaks: true,
                require_prelim_speaker_order: true,
                require_elim_substantive_speaks: false,
                require_elim_speaker_order: false,
                reply_speakers: true,
                reply_must_speak: true,
            },
            Action::CreateMotion {
                tournament_idx: 235,
                round_idx: 46,
                motion: "O".to_string(),
                infoslide: None,
            },
            Action::GenerateDraw {
                tournament_idx: 134,
                round_idx: 101,
                force: false,
            },
            Action::CreateJudge {
                tournament_idx: 153,
                name: String::new(),
                email: "v@g.org".to_string(),
                institution_idx: None,
            },
            Action::MoveJudge {
                tournament_idx: 130,
                round_idx: 136,
                judge_idx: 241,
                debate_idx: 81,
                role: String::new(),
                assign: true,
            },
            Action::ViewTournamentPage {
                tournament_idx: 244,
                round_idx: 153,
                page_idx: 185,
            },
        ],
    });
}
