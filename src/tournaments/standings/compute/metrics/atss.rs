use std::collections::HashMap;

use crate::schema::{
    agg_team_results_of_debate, debates,
    rounds::{self},
    teams,
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
        debates::table
            .inner_join(rounds::table.on(rounds::id.eq(debates::round_id)))
            .filter(
                rounds::tournament_id
                    .eq(tid)
                    .and(rounds::kind.eq("P"))
                    .and(rounds::completed.eq(true)),
            )
            .inner_join(teams::table.on(diesel::dsl::exists(
                agg_team_results_of_debate::table.filter(
                    agg_team_results_of_debate::team_id.eq(teams::id).and(
                        agg_team_results_of_debate::debate_id.eq(debates::id),
                    ),
                ),
            )))
            .group_by(teams::id)
            .select((teams::id, diesel::dsl::count(debates::id)))
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
