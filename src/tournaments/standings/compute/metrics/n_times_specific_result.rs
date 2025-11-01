use diesel::prelude::*;

use crate::{
    schema::{
        tournament_debate_team_results, tournament_debates, tournament_rounds,
        tournament_teams,
    },
    tournaments::standings::compute::metrics::{Metric, MetricValue},
};

/// Counts the number of times that a team achieved a specific result (e.g.
/// #firsts, #seconds, ...)
pub struct NTimesSpecificResultComputer(pub u8);

impl Metric<MetricValue> for NTimesSpecificResultComputer {
    fn compute(
        &self,
        tid: &str,
        conn: &mut impl diesel::connection::LoadConnection<
            Backend = diesel::sqlite::Sqlite,
        >,
    ) -> std::collections::HashMap<String, MetricValue> {
        let results: std::collections::HashMap<String, i64> =
            tournament_teams::table
                .filter(tournament_teams::tournament_id.eq(tid))
                .inner_join(
                    tournament_debate_team_results::table
                        .inner_join(
                            tournament_debates::table.inner_join(
                                tournament_rounds::table.on(
                                    tournament_rounds::id
                                        .eq(tournament_debates::round_id)
                                        .and(tournament_rounds::kind.eq("P"))
                                        .and(
                                            tournament_rounds::completed
                                                .eq(true),
                                        ),
                                ),
                            ),
                        )
                        .on(tournament_debate_team_results::team_id
                            .eq(tournament_teams::id)
                            .and(
                                tournament_debate_team_results::debate_id
                                    .eq(tournament_debates::id),
                            )),
                )
                .filter(
                    tournament_debate_team_results::points.eq(self.0 as i64),
                )
                .group_by(tournament_teams::id)
                .select((
                    tournament_teams::id,
                    diesel::dsl::count(tournament_debate_team_results::id),
                ))
                .load::<(String, i64)>(conn)
                .unwrap()
                .into_iter()
                .collect();

        tournament_teams::table
            .filter(tournament_teams::tournament_id.eq(tid))
            .select(tournament_teams::id)
            .load::<String>(conn)
            .unwrap()
            .into_iter()
            .map(|team_id| {
                let count = results.get(&team_id).copied().unwrap_or(0);
                (team_id, MetricValue::Integer(count))
            })
            .collect()
    }
}
