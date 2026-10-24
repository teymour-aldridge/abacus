use std::collections::HashMap;

use diesel::prelude::*;

use crate::schema::{agg_team_results_of_debate, debates, rounds, teams};

pub fn times_team_achieved_p_points(
    (p, tid): (u8, &str),
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> HashMap<String, i64> {
    let results: std::collections::HashMap<String, i64> = teams::table
        .filter(teams::tournament_id.eq(tid))
        .inner_join(
            agg_team_results_of_debate::table
                .on(agg_team_results_of_debate::team_id.eq(teams::id)),
        )
        .inner_join(
            debates::table
                .on(agg_team_results_of_debate::debate_id.eq(debates::id)),
        )
        .inner_join(rounds::table.on(rounds::id.eq(debates::round_id)))
        .filter(rounds::kind.eq("P"))
        .filter(rounds::completed.eq(true))
        .filter(agg_team_results_of_debate::points.eq(p as i64))
        .group_by(teams::id)
        .select((
            teams::id,
            diesel::dsl::count(agg_team_results_of_debate::id),
        ))
        .load::<(String, i64)>(conn)
        .unwrap()
        .into_iter()
        .collect();

    teams::table
        .filter(teams::tournament_id.eq(tid))
        .select(teams::id)
        .load::<String>(conn)
        .unwrap()
        .into_iter()
        .map(|team_id| {
            let count = results.get(&team_id).copied().unwrap_or(0);
            (team_id, count)
        })
        .collect()
}
