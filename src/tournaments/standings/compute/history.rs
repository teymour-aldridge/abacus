use std::collections::HashMap;

use diesel::{
    connection::LoadConnection, prelude::*, sql_types::BigInt, sqlite::Sqlite,
};

use crate::{
    schema::{teams, teams_of_debate, tournaments},
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

        let query = teams::table
            .filter(teams::tournament_id.eq(tid))
            .inner_join(
                teams_of_debate::table
                    .on(teams_of_debate::team_id.eq(teams::id)),
            )
            .group_by(teams::id);

        let history = match tournament.teams_per_side {
            1 => query
                .select((
                    teams::id,
                    diesel::dsl::count(diesel::dsl::case_when(
                        teams_of_debate::side
                            .eq(0)
                            .and(teams_of_debate::seq.eq(0)),
                        1.as_sql::<BigInt>(),
                    )),
                    diesel::dsl::count(diesel::dsl::case_when(
                        teams_of_debate::side
                            .eq(1)
                            .and(teams_of_debate::seq.eq(0)),
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
                    teams::id,
                    diesel::dsl::count(diesel::dsl::case_when(
                        teams_of_debate::side
                            .eq(0)
                            .and(teams_of_debate::seq.eq(0)),
                        1.as_sql::<BigInt>(),
                    )),
                    diesel::dsl::count(diesel::dsl::case_when(
                        teams_of_debate::side
                            .eq(1)
                            .and(teams_of_debate::seq.eq(0)),
                        1.as_sql::<BigInt>(),
                    )),
                    diesel::dsl::count(diesel::dsl::case_when(
                        teams_of_debate::side
                            .eq(0)
                            .and(teams_of_debate::seq.eq(1)),
                        1.as_sql::<BigInt>(),
                    )),
                    diesel::dsl::count(diesel::dsl::case_when(
                        teams_of_debate::side
                            .eq(0)
                            .and(teams_of_debate::seq.eq(0)),
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
