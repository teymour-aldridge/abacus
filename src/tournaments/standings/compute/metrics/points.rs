use diesel::prelude::*;

use crate::schema::{
    tournament_debate_team_results, tournament_debates, tournament_rounds,
};
use crate::tournaments::standings::compute::metrics::{Metric, MetricValue};

pub struct TeamPointsComputer;

impl Metric<MetricValue> for TeamPointsComputer {
    fn compute(
        &self,
        tid: &str,
        conn: &mut impl diesel::connection::LoadConnection<
            Backend = diesel::sqlite::Sqlite,
        >,
    ) -> std::collections::HashMap<String, MetricValue> {
        let results: Vec<(String, i64)> = tournament_debate_team_results::table
            .inner_join(
                tournament_debates::table
                    .on(tournament_debate_team_results::debate_id
                        .eq(tournament_debates::id)),
            )
            .inner_join(
                tournament_rounds::table
                    .on(tournament_debates::round_id.eq(tournament_rounds::id)),
            )
            .filter(tournament_rounds::tournament_id.eq(tid))
            .filter(tournament_rounds::kind.eq("P"))
            .filter(tournament_rounds::completed.eq(true))
            .select((
                tournament_debate_team_results::team_id,
                tournament_debate_team_results::points,
            ))
            .load::<(String, i64)>(conn)
            .expect("Failed to load team points");

        println!("{:?}", results);

        let mut team_points = std::collections::HashMap::new();

        for (team_id, points) in results {
            *team_points.entry(team_id).or_insert(0i64) += points;
        }

        team_points
            .into_iter()
            .map(|(team_id, points)| (team_id, MetricValue::Integer(points)))
            .collect()
    }
}
