use std::collections::{HashMap, HashSet};

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};

use crate::{
    schema::{debates, rounds, teams_of_debate},
    tournaments::rounds::Round,
};

/// Prior preliminary-round meetings, counting every pair of teams in a room.
///
/// Each unordered pair is stored with the lexicographically smaller ID first.
/// Nested maps allow lookups by borrowed team IDs without allocating strings.
#[derive(Clone, Debug, Default)]
pub struct OpponentHistory {
    counts: HashMap<String, HashMap<String, usize>>,
}

impl OpponentHistory {
    pub fn n_times_prev_debated(&self, a: &str, b: &str) -> usize {
        let (a, b) = if a < b { (a, b) } else { (b, a) };
        self.counts
            .get(a)
            .and_then(|opponents| opponents.get(b))
            .copied()
            .unwrap_or(0)
    }

    /// Includes existing draws even without results or a released draw, but
    /// excludes the current round, later rounds, and all elimination rounds.
    pub fn fetch(
        round: &Round,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Self {
        let entries = teams_of_debate::table
            .inner_join(debates::table.inner_join(rounds::table))
            .filter(teams_of_debate::tournament_id.eq(&round.tournament_id))
            .filter(debates::tournament_id.eq(&round.tournament_id))
            .filter(rounds::tournament_id.eq(&round.tournament_id))
            .filter(rounds::kind.eq("P"))
            .filter(rounds::seq.lt(round.seq))
            .select((debates::id, teams_of_debate::team_id))
            .load::<(String, String)>(conn)
            .unwrap();

        let mut teams_by_debate: HashMap<String, HashSet<String>> =
            HashMap::new();
        for (debate, team) in entries {
            teams_by_debate.entry(debate).or_default().insert(team);
        }

        let mut history = Self::default();
        for teams in teams_by_debate.into_values() {
            let mut teams = teams.into_iter().collect::<Vec<_>>();
            teams.sort_unstable();
            for (i, a) in teams.iter().enumerate() {
                for b in &teams[i + 1..] {
                    *history
                        .counts
                        .entry(a.clone())
                        .or_default()
                        .entry(b.clone())
                        .or_default() += 1;
                }
            }
        }
        history
    }
}
