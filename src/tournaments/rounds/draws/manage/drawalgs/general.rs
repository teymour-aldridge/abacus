use std::collections::HashMap;

use good_lp::{
    Constraint, Expression, ProblemVariables, Solution, SolverModel, Variable,
    constraint, highs, variable,
};
use itertools::Itertools;
use rand::Rng;
use rust_decimal::prelude::ToPrimitive;

use crate::tournaments::{
    config::{PullupMetric, RankableTeamMetric, UnrankableTeamMetric},
    rounds::draws::manage::drawalgs::{DrawInput, MakeDrawError},
    standings::compute::{TeamStandings, history::TeamHistory},
    teams::Team,
};

/// A map of program objects to the corresponding linear programming variables.
#[derive(Default)]
pub struct VariableMap {
    /// These are the variables x_{t,r,p}, each of which denotes whether team t
    /// is assigned to room r in position p.
    pub team_allocs: HashMap<(String, usize, usize), Variable>,
    /// These are the variables b_{r,s}, each of which denotes whether room r
    /// is an s-point room.
    pub room_brackets: HashMap<(usize, usize), Variable>,
    /// These are the variables y_{t,s}, each of which denotes whether team t
    /// is in an s-point bracket.
    pub team_brackets: HashMap<(String, usize), Variable>,
    /// These are the variables z_b, denoting the size of each room.
    pub bracket_size: HashMap<usize, Variable>,
}

pub type TeamsOfRoom = (Vec<Team>, Vec<Team>);

/// A relatively general draw algorithm, which uses linear programming. I
/// believe this handles WSDC and BP correctly. This may be too slow (and
/// doesn't handle Australs-style formats). If it is not sufficiently rapid, it
/// may be necessary to instead adopt format-specific algorithms. In this case,
/// we should be careful to ensure that the user is first directed to pick the
/// type of draw algorithm, and then only make available the configuration
/// options relevant to this algorithm.
///
/// The variables in the problem are
/// - x_{t,r,p}, a binary variable which is 1 if team t is assigned in room r
///   at position p
/// - b_{r,s}, a binary variable denoting whether room r is an s-point bracket
///   room
/// - y_{t,s}, a binary variable denoting whether team t is assigned to an
///   s-point bracket room
/// - z_{b}, an integer variable which denotes the size of bracket b
///
/// Additional description can be found here:
/// https://www.overleaf.com/read/sstwcyfjbrhx#1c6d64
pub fn make_draw(
    input: DrawInput,
    standings: &TeamStandings,
    TeamHistory(history): &TeamHistory,
) -> Result<Vec<TeamsOfRoom>, MakeDrawError> {
    if input.teams.is_empty() {
        return Err(MakeDrawError::InvalidTeamCount(
            "There are no teams!".to_string(),
        ));
    }

    let mut problem = ProblemVariables::new();
    let mut variable_map = VariableMap::default();

    let mut constraints: Vec<Constraint> = Vec::new();

    let teams_per_room = (input.tournament.teams_per_side * 2) as usize;
    if (input.teams.len() % teams_per_room) != 0 {
        return Err(MakeDrawError::InvalidTeamCount(format!(
            "The number of available teams should be divisible by {teams_per_room}
             (there were {} teams).",
            input.teams.len()
        )));
    }

    let (min_score, max_score) = match standings
        .ranked
        .iter()
        .flat_map(|team| {
            team.iter()
                .map(|team| standings.points_of_team(&team.id).unwrap())
        })
        .minmax()
    {
        itertools::MinMaxResult::MinMax(a, b) => (a as usize, b as usize),
        itertools::MinMaxResult::NoElements
        | itertools::MinMaxResult::OneElement(_) => unreachable!(),
    };

    let rooms = input.teams.len() / teams_per_room;

    for team in &input.teams {
        for room in 0..rooms {
            for position in 0..teams_per_room {
                let var = problem.add(
                    variable()
                        .name(format!("{}_{room}_{position}", &team.id))
                        .binary(),
                );
                variable_map
                    .team_allocs
                    .insert((team.id.clone(), room, position), var);
            }
        }
    }

    for team in &input.teams {
        for score in min_score..=max_score {
            let y_ts = problem.add(
                variable().name(format!("y_{}_{score}", team.id)).binary(),
            );
            variable_map
                .team_brackets
                .insert((team.id.clone(), score), y_ts);
        }
    }

    for room in 0..rooms {
        for score in min_score..=max_score {
            let b_rs = problem
                .add(variable().name(format!("b_{room}_{score}")).binary());
            variable_map.room_brackets.insert((room, score), b_rs);
        }
    }

    for bracket in min_score..=max_score {
        let z_b = problem.add(variable().integer().min(0));
        variable_map.bracket_size.insert(bracket, z_b);
    }

    let _each_team_assigned_exactly_once = {
        for team in &input.teams {
            let mut sum = Expression::default();

            for room in 0..rooms {
                for p in 0..teams_per_room {
                    sum += variable_map
                        .team_allocs
                        // todo: string interning
                        .get(&(team.id.clone(), room, p))
                        .unwrap();
                }
            }

            constraints.push(good_lp::constraint::eq(sum, 1));
        }
    };

    let _each_position_has_exactly_one_team_assigned = {
        for p in 0..teams_per_room {
            for room in 0..rooms {
                let mut expr = Expression::default();

                for team in &input.teams {
                    expr += variable_map
                        .team_allocs
                        .get(&(team.id.clone(), room, p))
                        .unwrap();
                }

                constraints.push(good_lp::constraint::eq(expr, 1))
            }
        }
    };

    let _each_team_is_in_exactly_one_bracket = {
        for team in &input.teams {
            let mut sum = Expression::default();

            for bracket in min_score..=max_score {
                sum += variable_map
                    .team_brackets
                    .get(&(team.id.clone(), bracket))
                    .unwrap();
            }

            constraints.push(good_lp::constraint::eq(sum, 1));
        }
    };

    let _teams_not_pulled_down = {
        for team in &input.teams {
            for bracket in min_score
                ..(standings.points_of_team(&team.id).unwrap() as usize)
            {
                constraints.push(good_lp::constraint::eq(
                    *variable_map
                        .team_brackets
                        .get(&(team.id.clone(), bracket))
                        .unwrap(),
                    0,
                ));
            }
        }
    };

    let _bracket_sizes_match = {
        for score in min_score..=max_score {
            let mut number_of_rooms_of_score_s = Expression::default();
            for room in 0..rooms {
                number_of_rooms_of_score_s +=
                    variable_map.room_brackets.get(&(room, score)).unwrap();
            }
            constraints.push(good_lp::constraint::eq(
                *variable_map.bracket_size.get(&score).unwrap(),
                number_of_rooms_of_score_s,
            ));

            let mut number_of_teams_in_bracket = Expression::default();
            for team in &input.teams {
                number_of_teams_in_bracket += *variable_map
                    .team_brackets
                    .get(&(team.id.clone(), score))
                    .unwrap();
            }
            constraints.push(good_lp::constraint::eq(
                *variable_map.bracket_size.get(&score).unwrap()
                    * (teams_per_room as f64),
                number_of_teams_in_bracket,
            ));
        }
    };

    let _right_number_of_teams_per_room = {
        for room in 0..rooms {
            let mut teams_assigned_to_this_room = Expression::default();
            for team in &input.teams {
                for position in 0..teams_per_room {
                    teams_assigned_to_this_room += variable_map
                        .team_allocs
                        .get(&(team.id.clone(), room, position))
                        .unwrap();
                }
            }
            constraints.push(good_lp::constraint::eq(
                teams_assigned_to_this_room,
                teams_per_room as f64,
            ));
        }
    };

    // We also need to ensure that when a team is assigned to score bracket s
    // it can only be allocated to a room r if r is a room that is also an
    // s-point room.
    let _team_brackets_match_room_brackets = {
        for team in &input.teams {
            for room in 0..rooms {
                for score in min_score..=max_score {
                    let slack = problem.add(
                        variable()
                            .name(format!("slack_{}_{room}_{score}", team.id)),
                    );

                    let y_ts = *variable_map
                        .team_brackets
                        .get(&(team.id.clone(), score))
                        .unwrap();

                    let b_rs = *variable_map
                        .room_brackets
                        .get(&(room, score))
                        .unwrap();

                    constraints.push(constraint!(slack <= 1 - (y_ts - b_rs)));
                    constraints.push(constraint!(slack <= 1 + (y_ts - b_rs)));
                    constraints.push(constraint!(slack >= y_ts + b_rs - 1));
                    constraints.push(constraint!(slack >= 1 - (y_ts + b_rs)));

                    for position in 0..teams_per_room {
                        let x_irp = *variable_map
                            .team_allocs
                            .get(&(team.id.clone(), room, position))
                            .unwrap();

                        constraints.push(constraint!(x_irp <= slack));
                    }
                }
            }
        }
    };

    let power_pairing_objective = {
        let pullup_metrics = serde_json::from_str::<Vec<PullupMetric>>(
            &input.tournament.pullup_metrics,
        )
        .expect("invalid pullup metric list");

        // todo: option to customise this (e.g. for pull-ups)

        let mut obj = Expression::default();
        for team in &input.teams {
            let team_score =
                standings.points_of_team(&team.id).unwrap() as usize;

            for score in std::cmp::max(team_score, min_score)..=max_score {
                let penalty = {
                    let mut penalty = 0.0;

                    for metric in &pullup_metrics {
                        // todo: need to rank based on relative importance of
                        // the metrics (work out upper/lower bounds and add
                        // multipliers accordingly)
                        match metric {
                            PullupMetric::LowestRank
                            | PullupMetric::HighestRank => {
                                let sign = if matches!(
                                    metric,
                                    PullupMetric::LowestRank
                                ) {
                                    -1.0
                                } else {
                                    1.0
                                };
                                penalty += sign
                                    * standings
                                        .ranked
                                        .iter()
                                        .enumerate()
                                        .find_map(|(idx, cmp)| {
                                            if cmp
                                                .iter()
                                                .any(|cmp| cmp.id == team.id)
                                            {
                                                Some(idx)
                                            } else {
                                                None
                                            }
                                        })
                                        .unwrap()
                                        as f64
                            }
                            PullupMetric::Random => (),
                            PullupMetric::FewerPreviousPullups => todo!(),
                            PullupMetric::LowestDsRank => {
                                let ds_rank = standings.pullup_metrics.get(&(
                                    team.id.clone(),
                                    UnrankableTeamMetric::DrawStrengthByRank,
                                )).unwrap();
                                penalty +=
                                    -(*ds_rank.as_integer().unwrap() as f64);
                            }
                            PullupMetric::LowestDsSpeaks => {
                                let sub_penalty = standings
                                    .ranked_metrics_of_team
                                    .get(&team.id)
                                    .unwrap()
                                    .iter()
                                    .find_map(|(kind, value)| {
                                        if matches!(
                                            kind,
                                            RankableTeamMetric::AverageTotalSpeakerScore
                                        ) {
                                            Some(
                                                -value
                                                    .as_float()
                                                    .unwrap()
                                                    .to_f64()
                                                    .unwrap(),
                                            )
                                        } else {
                                            None
                                        }
                                    })
                                    .unwrap();
                                penalty += sub_penalty;
                            }
                        }
                    }

                    penalty
                };

                obj += (penalty +
                    // we always want teams to be pulled up as few brackets as
                    // possible (!)
                    ((score - team_score) as f64)
                    // add small pertubation to ensure that pull ups are random
                    // (this should break ties where we could pull up multiple
                    //  teams)
                    + rand::rng().sample(
                        rand::distr::Uniform::new(0.0f64, 0.1f64).unwrap(),
                    ))
                    * *variable_map
                        .team_brackets
                        .get(&(team.id.clone(), score))
                        .unwrap();
            }
        }

        obj
    };

    let position_balance_objective = {
        let mut obj = Expression::default();
        for team in &input.teams {
            for room in 0..rooms {
                for position in 0..teams_per_room {
                    let position_cost =
                        history.get(&team.id).unwrap()[position];

                    obj += (position_cost as f64
                        // add small random value to ensure sufficient
                        // randomness in generation
                        + rand::rng().sample(
                            rand::distr::Uniform::new(0.0f64, 0.1f64).unwrap(),
                        ))
                        * *variable_map
                            .team_allocs
                            .get(&(team.id.clone(), room, position))
                            .unwrap();
                }
            }
        }
        obj
    };

    let solution = problem
        .optimise(
            good_lp::solvers::ObjectiveDirection::Minimisation,
            1000 * power_pairing_objective + position_balance_objective,
        )
        .using(highs)
        .set_mip_rel_gap(0.012)
        .unwrap()
        .solve()
        .unwrap();

    // todo: annotations (so that we can record which teams were pulled-up,
    // room brackets, etc)
    let mut drawn_rooms = Vec::with_capacity(rooms);

    for room in 0..rooms {
        let mut prop_teams = Vec::new();
        let mut opp_teams = Vec::new();

        for position in 0..teams_per_room {
            for team in &input.teams {
                let var = variable_map
                    .team_allocs
                    .get(&(team.id.clone(), room, position))
                    .unwrap();

                if solution.value(*var) >= 0.95 {
                    if position % 2 == 0 {
                        prop_teams.push(team.clone());
                    } else {
                        opp_teams.push(team.clone());
                    }
                }
            }
        }

        assert_eq!(prop_teams.len(), opp_teams.len());

        drawn_rooms.push((prop_teams, opp_teams))
    }

    Ok(drawn_rooms)
}
