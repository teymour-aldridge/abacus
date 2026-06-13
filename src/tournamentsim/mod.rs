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
