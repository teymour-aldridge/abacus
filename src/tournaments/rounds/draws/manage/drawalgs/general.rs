use std::collections::HashMap;

use good_lp::{
    Constraint, Expression, ProblemVariables, Solution, SolverModel, Variable,
    constraint, highs, variable,
};
use itertools::Itertools;
use rand::Rng;

use crate::tournaments::{
    config::PullupMetric,
    rounds::draws::manage::drawalgs::{DrawInput, MakeDrawError},
    standings::compute::{TeamStandings, history::TeamHistory},
    teams::Team,
};

/// A map of program objects to the corresponding linear programming variables.
#[derive(Default)]
pub struct VariableMap {
    pub team_allocs: HashMap<(String, usize, usize), Variable>,
    pub room_brackets: HashMap<(usize, usize), Variable>,
    pub team_brackets: HashMap<(String, usize), Variable>,
    pub bracket_size: HashMap<usize, Variable>,
}

pub type TeamsOfRoom = (Vec<Team>, Vec<Team>);

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
        .map(|team| {
            team.iter()
                .map(|team| standings.points_of_team(&team.id).unwrap())
        })
        .flatten()
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

    {
        for team in &input.teams {
            let sum = {
                let mut expr = Expression::default();

                for room in 0..rooms {
                    for p in 0..teams_per_room {
                        expr += variable_map
                            .team_allocs
                            // todo: string interning
                            .get(&(team.id.clone(), room, p))
                            .unwrap();
                    }
                }

                expr
            };

            constraints.push(good_lp::constraint::eq(sum, 1));
        }
    };

    {
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

    {
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

    {
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

    {
        for score in min_score..=max_score {
            let mut sum = Expression::default();
            for room in 0..rooms {
                sum += variable_map.room_brackets.get(&(room, score)).unwrap();
            }
            constraints.push(good_lp::constraint::eq(
                *variable_map.bracket_size.get(&score).unwrap(),
                sum,
            ));

            let mut sum = Expression::default();
            for team in &input.teams {
                sum += *variable_map
                    .team_brackets
                    .get(&(team.id.clone(), score))
                    .unwrap();
            }
            constraints.push(good_lp::constraint::eq(
                *variable_map.bracket_size.get(&score).unwrap()
                    * (teams_per_room as f64),
                sum,
            ));
        }
    };

    {
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

    {
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
                            PullupMetric::LowestRank => {
                                penalty += standings
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
                            PullupMetric::HighestRank => {
                                penalty += -(standings
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
                                    as f64)
                            }
                            PullupMetric::Random => (),
                            PullupMetric::FewerPreviousPullups => todo!(),
                            PullupMetric::LowestDsRank => todo!(),
                            PullupMetric::LowestDsSpeaks => todo!(),
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
