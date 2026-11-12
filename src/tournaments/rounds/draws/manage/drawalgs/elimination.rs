//! Seed once using the chosen opening algorithm, then preserve bracket paths.
//!
//! By default, opening rooms are folded by seed: an eight-team break has
//! 1 v 8, 2 v 7, 3 v 6 and 4 v 5. Slide, adjacent and random opening
//! pairings are also available. Subsequent rounds fold the previous rooms
//! (1 with 4, 2 with 3), not the surviving teams' original seeds. This
//! also works for four-team debates, with two advancing from each room.
//! Debate numbers must therefore remain in bracket order when saving draws.

use std::collections::{HashMap, HashSet};

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use rand::seq::SliceRandom;

use super::{
    DrawInput, MakeDrawError, general::TeamsOfRoom,
    previous_elimination_round_in_break_category,
};
use crate::{
    schema::{agg_team_results_of_debate, debates, teams_of_debate},
    tournaments::{
        config::EliminationDrawAlgorithm, rounds::Round,
        standings::compute::TeamStandings, teams::Team,
    },
};

/// Make tied seeds reproducible, including ties at the break cutoff.
pub(super) fn seed_order(standings: &TeamStandings) -> Vec<&Team> {
    standings
        .teams_in_rank_order
        .iter()
        .flat_map(|tied| {
            let mut tied = tied.iter().collect::<Vec<_>>();
            tied.sort_by(|a, b| a.id.cmp(&b.id));
            tied
        })
        .collect()
}

fn validate_size(
    count: usize,
    teams_per_room: usize,
) -> Result<(), MakeDrawError> {
    if count < teams_per_room
        || count % teams_per_room != 0
        || !(count / teams_per_room).is_power_of_two()
    {
        return Err(MakeDrawError::InvalidTeamCount(format!(
            "An elimination draw requires {teams_per_room} teams per debate \
             and a power-of-two number of debates (there were {count} teams). \
             Partial breaks and byes are not supported."
        )));
    }
    Ok(())
}

pub(super) fn prepare_brackets(
    round: &Round,
    standings: &TeamStandings,
    eligible: &HashSet<String>,
    teams_per_side: i64,
    algorithm: EliminationDrawAlgorithm,
    rng: &mut rand_chacha::ChaCha20Rng,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<Vec<Vec<String>>, MakeDrawError> {
    let teams_per_room = usize::try_from(teams_per_side)
        .ok()
        .and_then(|n| n.checked_mul(2))
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            MakeDrawError::InvalidConfiguration(
                "The number of teams per side must be positive.".to_string(),
            )
        })?;

    if let Some(previous) =
        previous_elimination_round_in_break_category(round, conn)
    {
        if !previous.completed {
            return Err(MakeDrawError::InvalidConfiguration(format!(
                "Previous round {} must be completed first.",
                previous.name
            )));
        }
        let rooms = debates::table
            .filter(debates::round_id.eq(&previous.id))
            .order_by(debates::number.asc())
            .select((debates::id, debates::number))
            .load::<(String, i64)>(conn)
            .unwrap();
        if rooms.len() < 2 || !rooms.len().is_power_of_two() {
            return Err(MakeDrawError::InvalidTeamCount(
                "The previous round must have a power-of-two number of debates \
                 and at least two debates to feed the next round.".to_string(),
            ));
        }
        let mut winners = Vec::with_capacity(rooms.len());
        let mut seen = HashSet::new();
        for (index, (debate_id, number)) in rooms.iter().enumerate() {
            if *number != index as i64 + 1 {
                return Err(MakeDrawError::InvalidConfiguration(
                    "Previous debate numbers must be consecutive starting at 1 \
                     to preserve the elimination bracket.".to_string(),
                ));
            }
            let participants = teams_of_debate::table
                .filter(teams_of_debate::debate_id.eq(debate_id))
                .select(teams_of_debate::team_id)
                .load::<String>(conn)
                .unwrap()
                .into_iter()
                .collect::<HashSet<_>>();
            let advancing = agg_team_results_of_debate::table
                .filter(agg_team_results_of_debate::debate_id.eq(debate_id))
                .filter(agg_team_results_of_debate::points.eq(1))
                .select(agg_team_results_of_debate::team_id)
                .load::<String>(conn)
                .unwrap();
            if participants.len() != teams_per_room
                || advancing.len() != teams_per_room / 2
                || advancing.iter().any(|id| {
                    !participants.contains(id) || !seen.insert(id.clone())
                })
            {
                return Err(MakeDrawError::InvalidConfiguration(format!(
                    "Debate {number} in {} must have {} participants and \
                     exactly {} advancing teams with confirmed results.",
                    previous.name,
                    teams_per_room,
                    teams_per_room / 2
                )));
            }
            winners.push(advancing);
        }
        // Fold room paths, never the surviving teams' break ranks.
        let half = winners.len() / 2;
        return Ok((0..half)
            .map(|i| {
                winners[i]
                    .iter()
                    .chain(&winners[winners.len() - 1 - i])
                    .cloned()
                    .collect()
            })
            .collect());
    }

    let seeds = seed_order(standings)
        .into_iter()
        .filter(|team| eligible.contains(&team.id))
        .map(|team| team.id.clone())
        .collect::<Vec<_>>();
    seed_brackets(seeds, teams_per_room, algorithm, rng)
}

fn seed_brackets(
    mut seeds: Vec<String>,
    teams_per_room: usize,
    algorithm: EliminationDrawAlgorithm,
    rng: &mut rand_chacha::ChaCha20Rng,
) -> Result<Vec<Vec<String>>, MakeDrawError> {
    validate_size(seeds.len(), teams_per_room)?;
    if algorithm == EliminationDrawAlgorithm::Random {
        seeds.shuffle(rng);
    }
    if matches!(
        algorithm,
        EliminationDrawAlgorithm::Adjacent | EliminationDrawAlgorithm::Random
    ) {
        return Ok(seeds
            .chunks(teams_per_room)
            .map(|room| room.to_vec())
            .collect());
    }
    let room_count = seeds.len() / teams_per_room;
    let mut rooms = vec![Vec::with_capacity(teams_per_room); room_count];
    // Fold alternates the direction of each seed band, while slide leaves
    // every band ascending. In BP a 16-team folded break therefore starts
    // with [1, 8, 9, 16], [2, 7, 10, 15], ...
    for band in 0..teams_per_room {
        for (room, teams) in rooms.iter_mut().enumerate() {
            let offset = if algorithm == EliminationDrawAlgorithm::Slide
                || band % 2 == 0
            {
                room
            } else {
                room_count - 1 - room
            };
            teams.push(seeds[band * room_count + offset].clone());
        }
    }
    Ok(rooms)
}

pub(super) fn make_outround_draw(
    mut input: DrawInput,
    brackets: Vec<Vec<String>>,
) -> Result<Vec<TeamsOfRoom>, MakeDrawError> {
    let mut available = input
        .teams
        .into_iter()
        .map(|team| (team.id.clone(), team))
        .collect::<HashMap<_, _>>();
    let mut draw = Vec::with_capacity(brackets.len());
    for bracket in brackets {
        let mut teams = Vec::with_capacity(bracket.len());
        for id in bracket {
            teams.push(available.remove(&id).ok_or_else(|| {
                MakeDrawError::InvalidTeamCount(
                    "Every team in the elimination bracket must be available. \
                     Mark all qualifying teams available before generating the draw."
                        .to_string(),
                )
            })?);
        }
        // Randomize positions within each debate, but never the room order.
        teams.shuffle(&mut input.rng);
        let opposition = teams.split_off(teams.len() / 2);
        draw.push((teams, opposition));
    }
    Ok(draw)
}
