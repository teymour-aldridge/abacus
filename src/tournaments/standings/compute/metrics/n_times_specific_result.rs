use diesel::{dsl, prelude::*};

use crate::{
    schema::{
        tournament_debate_team_results, tournament_debates, tournament_rounds,
        tournament_teams,
    },
    tournaments::standings::compute::metrics::{
        Metric, MetricValue, completed_preliminary_rounds,
    },
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
        tournament_teams::table
            .filter(tournament_teams::tournament_id.eq(tid))
            .inner_join(completed_preliminary_rounds())
            .inner_join(
                tournament_debates::table
                    .on(tournament_debates::round_id.eq(tournament_rounds::id)),
            )
            .inner_join(
                tournament_debate_team_results::table.on(
                    tournament_debate_team_results::team_id
                        .eq(tournament_teams::id)
                        .and(
                            tournament_debate_team_results::debate_id
                                .eq(tournament_debates::id),
                        ),
                ),
            )
            .filter(tournament_debate_team_results::points.eq(self.0 as i64))
            .group_by(tournament_teams::id)
            .select((
                tournament_teams::id,
                diesel::dsl::count(tournament_debate_team_results::id),
            ))
            .load::<(String, i64)>(conn)
            .unwrap()
            .into_iter()
            .map(|(team, value)| (team, (MetricValue::Integer(value))))
            .collect()
    }
}
