use std::collections::HashMap;

use crate::schema::{
    tournament_debate_team_results, tournament_debates,
    tournament_rounds::{self},
    tournament_teams,
};
use diesel::prelude::*;
use rust_decimal::Decimal;

pub fn atss(
    (tid, tss): (&str, HashMap<String, rust_decimal::Decimal>),
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> HashMap<String, rust_decimal::Decimal> {
    let debates_team_appears_in: HashMap<String, i64> =
        tournament_debates::table
            .inner_join(
                tournament_rounds::table
                    .on(tournament_rounds::id.eq(tournament_debates::round_id)),
            )
            .filter(
                tournament_rounds::tournament_id
                    .eq(tid)
                    .and(tournament_rounds::kind.eq("P"))
                    .and(tournament_rounds::completed.eq(true)),
            )
            .inner_join(
                tournament_teams::table.on(diesel::dsl::exists(
                    tournament_debate_team_results::table.filter(
                        tournament_debate_team_results::team_id
                            .eq(tournament_teams::id)
                            .and(
                                tournament_debate_team_results::debate_id
                                    .eq(tournament_debates::id),
                            ),
                    ),
                )),
            )
            .group_by(tournament_teams::id)
            .select((
                tournament_teams::id,
                diesel::dsl::count(tournament_debates::id),
            ))
            .load::<(String, i64)>(conn)
            .unwrap()
            .into_iter()
            .collect();

    tss.into_iter()
        .map(|(k, v)| {
            let decimal = {
                let n_rounds_debated = debates_team_appears_in.get(&k).unwrap();
                if *n_rounds_debated == 0 {
                    assert!(v == Decimal::ZERO);
                    Decimal::ZERO
                } else {
                    v / Decimal::from(*n_rounds_debated)
                }
            };
            (k, decimal)
        })
        .collect()
}
