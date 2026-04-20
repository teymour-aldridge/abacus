//! Automated allocation of draw rooms based on the provided constraints.
//!
//! NOTE: although in the public-facing application we use the term "room
//! constraints", in this file we instead use the term "requirements" as
//! also means something in linear programming.

use std::collections::HashMap;

use good_lp::{
    Constraint, Expression, IntoAffineExpression, ProblemVariables, Solution,
    SolverModel, Variable, constraint,
};

use crate::tournaments::{
    rooms::{JudgeRoomConstraint, Room, SpeakerRoomConstraint},
    rounds::draws::RoundDrawRepr,
};

pub struct RoomAllocationProblemInputs {
    draw: RoundDrawRepr,
    speaker_constraints: Vec<SpeakerRoomConstraint>,
    judge_constraints: Vec<JudgeRoomConstraint>,
    available_rooms: Vec<Room>,
    /// Map (category_id, [room1, room2, ..., roomN])
    room_categories: HashMap<String, Vec<String>>,
}

pub struct RoomAllocationProblem {
    variable_container: ProblemVariables,
    variable_lookup_tbl: Vec<Vec<Variable>>,
    objective: Expression,
    input: RoomAllocationProblemInputs,
    constraints: Vec<Constraint>,
}

impl RoomAllocationProblem {
    pub fn new(input: RoomAllocationProblemInputs) -> RoomAllocationProblem {
        let mut variables = ProblemVariables::new();
        let mut variable_lookup_tbl: Vec<Vec<Variable>> = Vec::new();

        let mut constraints = Vec::new();

        let total_rooms_available = input.available_rooms.len();
        let total_debates = input.draw.debates.len();

        for i in 0..total_rooms_available {
            for j in 0..total_debates {
                variable_lookup_tbl[i].push(
                    variables.add(
                        good_lp::variable().integer().min(0).max(1).name(
                            format!("room {i} is assigned to debate {j}"),
                        ),
                    ),
                )
            }
        }

        // We now fill in some basic constraints.

        // Firstly, each room should be assigned at most once
        for i in 0..total_rooms_available {
            let mut total_debates_ith_room_assigned_to = Expression::default();

            for j in 0..total_debates {
                total_debates_ith_room_assigned_to += variable_lookup_tbl[i][j];
            }

            constraints
                .push(constraint!(total_debates_ith_room_assigned_to <= 1))
        }

        // Secondly, each debate needs a room assigned
        for j in 0..total_debates {
            let mut total_rooms_assigned_to_jth_debate = Expression::default();
            for i in 0..total_rooms_available {
                total_rooms_assigned_to_jth_debate += variable_lookup_tbl[i][j];
            }
            constraints
                .push(constraint!(total_rooms_assigned_to_jth_debate == 1))
        }

        // Having specified the feasible search space, we can move to defining
        // the objective criterion. NOTE: we define the objective so that higher
        // is better.

        // The first part of the objective is that we use whatever rooms we have
        // been told are "best".
        let mut use_better_rooms_criterion = Expression::default();

        for i in 0..total_rooms_available {
            for j in 0..total_debates {
                use_better_rooms_criterion += variable_lookup_tbl[i][j]
                    .into_expression()
                    * (input.available_rooms[i].priority as f64);
            }
        }

        // The next part is that we satisfy the requirements as best as
        // possible. This is pretty straightforward; the hard part of this is
        // a human-computer interaction issue (how can we present the knobs that
        // can be tuned in a meaningful way) and not a "solving the problem"
        // one.

        let mut satisfy_requirements_criterion = Expression::default();

        for i in 0..total_rooms_available {
            for j in 0..total_debates {
                for constraint in &input.speaker_constraints {
                    // TODO: make sure that draw.debates has a stable sort order
                    let is_speaker_in_debate = input.draw.debates[j]
                        .speakers_of_team
                        .values()
                        .any(|speakers| {
                            // TODO: would be interesting to see how much string
                            // interning reduces memory pressure
                            speakers
                                .iter()
                                .map(|speaker| speaker.id.clone())
                                .any(|t| t == constraint.speaker_id)
                        });

                    let room_satisfies_this_constraint = input.room_categories
                        [&constraint.category_id]
                        .contains(&input.available_rooms[j].id);

                    // wouldn't lazy evaluation be elegant here :)
                    if is_speaker_in_debate && room_satisfies_this_constraint {
                        satisfy_requirements_criterion +=
                            variable_lookup_tbl[i][j].into_expression()
                                * (constraint.pref as f64);
                    }
                }

                for constraint in &input.judge_constraints {
                    let is_judge_in_debate = input.draw.debates[j]
                        .judges_of_debate
                        .iter()
                        .any(|judge| judge.judge_id == constraint.judge_id);

                    let room_satisfies_this_constraint = input.room_categories
                        [&constraint.category_id]
                        .contains(&input.available_rooms[j].id);

                    if is_judge_in_debate && room_satisfies_this_constraint {
                        satisfy_requirements_criterion +=
                            variable_lookup_tbl[i][j].into_expression()
                                * (constraint.pref as f64);
                    }
                }
            }
        }

        Self {
            variable_container: variables,
            variable_lookup_tbl,
            input,
            objective: use_better_rooms_criterion
                + satisfy_requirements_criterion,
            constraints,
        }
    }

    /// Returns an assignment
    ///     debate_id -> room_id
    pub fn solve(self) -> HashMap<String, String> {
        let mut problem = self
            .variable_container
            .optimise(
                good_lp::solvers::ObjectiveDirection::Maximisation,
                self.objective,
            )
            .using(good_lp::highs);

        for constraint in self.constraints {
            problem.add_constraint(constraint);
        }

        let solution = problem.set_mip_rel_gap(0.012).unwrap().solve().unwrap();

        let mut answer = HashMap::new();
        for i in 0..self.input.available_rooms.len() {
            for j in 0..self.input.draw.debates.len() {
                let debate_this_room_is_assigned_to = self.variable_lookup_tbl
                    [i]
                    .iter()
                    .find(|assigned_room_j| {
                        solution.value(**assigned_room_j) >= 0.99
                    });

                if debate_this_room_is_assigned_to.is_some() {
                    answer.insert(
                        self.input.draw.debates[j].debate.id.clone(),
                        self.input.available_rooms[j].id.clone(),
                    );
                }
            }
        }

        answer
    }
}
