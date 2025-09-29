//! Creates a random draw.

use rand::seq::IteratorRandom;

use crate::tournaments::rounds::draws::manage::drawalgs::{
    DrawInput, MakeDrawError, general::TeamsOfRoom,
};

/// Generates a random draw.
pub fn gen_random(
    DrawInput {
        tournament,
        round: _,
        metrics: _,
        mut teams,
        mut rng,
        standings: _,
        history: _,
    }: DrawInput,
) -> Result<Vec<TeamsOfRoom>, MakeDrawError> {
    let denominator = (tournament.teams_per_side * 2) as usize;

    if teams.is_empty() {
        return Err(MakeDrawError::InvalidTeamCount(
            "There are no available teams!".to_string(),
        ));
    }

    if teams.len() % (denominator) != 0 {
        return Err(MakeDrawError::InvalidTeamCount(format!(
            "Expected the number of teams to be divisible by {denominator}.
             However, there are ${0} available teams to be drawn.",
            teams.len()
        )));
    }

    let mut output = Vec::with_capacity(teams.len() / denominator);

    while !teams.is_empty() {
        let mut pick_random_teams_for_side = || {
            let mut ret = Vec::new();
            for _ in 0..tournament.teams_per_side {
                let idx = (0..teams.len()).choose(&mut rng).unwrap();
                ret.push(teams.swap_remove(idx));
            }
            ret
        };

        output
            .push((pick_random_teams_for_side(), pick_random_teams_for_side()))
    }

    Ok(output)
}
