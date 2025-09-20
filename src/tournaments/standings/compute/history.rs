use std::collections::HashMap;

use diesel::{
    connection::LoadConnection, prelude::*, sql_types::BigInt, sqlite::Sqlite,
};

use crate::{
    schema::{tournament_debate_teams, tournament_teams, tournaments},
    tournaments::Tournament,
};

/// Contains the position history of each team in the given tournament. The map
/// is from team IDs to a list of team positions.
pub struct TeamHistory(pub HashMap<String, Vec<usize>>);

impl TeamHistory {
    pub fn fetch(
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Self {
        let tournament = tournaments::table
            .filter(tournaments::id.eq(tid))
            .first::<Tournament>(conn)
            .unwrap();

        let query =
            tournament_teams::table
                .filter(tournament_teams::tournament_id.eq(tid))
                .inner_join(tournament_debate_teams::table.on(
                    tournament_debate_teams::team_id.eq(tournament_teams::id),
                ))
                .group_by(tournament_teams::id);

        let history = match tournament.teams_per_side {
            1 => query
                .select((
                    tournament_teams::id,
                    diesel::dsl::count(diesel::dsl::case_when(
                        tournament_debate_teams::side
                            .eq(0)
                            .and(tournament_debate_teams::seq.eq(0)),
                        1.as_sql::<BigInt>(),
                    )),
                    diesel::dsl::count(diesel::dsl::case_when(
                        tournament_debate_teams::side
                            .eq(1)
                            .and(tournament_debate_teams::seq.eq(0)),
                        1.as_sql::<BigInt>(),
                    )),
                ))
                .load::<(String, i64, i64)>(conn)
                .unwrap()
                .into_iter()
                .map(|(string, aff, neg)| {
                    (string, vec![aff as usize, neg as usize])
                })
                .collect(),
            2 => query
                .select((
                    tournament_teams::id,
                    diesel::dsl::count(diesel::dsl::case_when(
                        tournament_debate_teams::side
                            .eq(0)
                            .and(tournament_debate_teams::seq.eq(0)),
                        1.as_sql::<BigInt>(),
                    )),
                    diesel::dsl::count(diesel::dsl::case_when(
                        tournament_debate_teams::side
                            .eq(1)
                            .and(tournament_debate_teams::seq.eq(0)),
                        1.as_sql::<BigInt>(),
                    )),
                    diesel::dsl::count(diesel::dsl::case_when(
                        tournament_debate_teams::side
                            .eq(0)
                            .and(tournament_debate_teams::seq.eq(1)),
                        1.as_sql::<BigInt>(),
                    )),
                    diesel::dsl::count(diesel::dsl::case_when(
                        tournament_debate_teams::side
                            .eq(0)
                            .and(tournament_debate_teams::seq.eq(0)),
                        1.as_sql::<BigInt>(),
                    )),
                ))
                .load::<(String, i64, i64, i64, i64)>(conn)
                .unwrap()
                .into_iter()
                .map(|(id, og, oo, cg, co)| {
                    (
                        id,
                        vec![
                            og as usize,
                            oo as usize,
                            cg as usize,
                            co as usize,
                        ],
                    )
                })
                .collect(),
            _ => {
                // todo: should probably rewrite query to handle a variable
                // numebr of teams
                unreachable!()
            }
        };

        TeamHistory(history)
    }
}
