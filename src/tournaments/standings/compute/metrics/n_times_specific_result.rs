use diesel::{dsl, prelude::*};

use crate::{
    schema::{
        tournament_debate_team_results, tournament_debates, tournament_draws,
        tournament_rounds, tournament_teams,
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
        conn: &mut (
                 impl diesel::Connection<Backend = diesel::sqlite::Sqlite>
                 + diesel::connection::LoadConnection
             ),
    ) -> std::collections::HashMap<String, MetricValue> {
        tournament_teams::table
            .filter(tournament_teams::tournament_id.eq(tid))
            .inner_join(completed_preliminary_rounds())
            .inner_join({
                let draws_subquery = diesel::alias!(tournament_draws as draws);

                tournament_draws::table.on(tournament_draws::released_at.ge(
                    draws_subquery
                        .filter(
                            draws_subquery
                                .field(tournament_draws::round_id)
                                .eq(tournament_rounds::id),
                        )
                        .select(dsl::max(
                            draws_subquery.field(tournament_draws::released_at),
                        ))
                        .single_value(),
                ))
            })
            .inner_join(
                tournament_debates::table
                    .on(tournament_debates::draw_id.eq(tournament_draws::id)),
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
            .map(|(team, value)| {
                (team, MetricValue::NTimesResult(self.0, value))
            })
            .collect()
    }
}
