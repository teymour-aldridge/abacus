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
