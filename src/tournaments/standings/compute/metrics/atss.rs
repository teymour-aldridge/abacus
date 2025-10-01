use std::collections::HashMap;

use crate::{
    schema::{
        tournament_debate_teams, tournament_debates,
        tournament_rounds::{self},
        tournament_teams,
    },
    tournaments::standings::compute::metrics::{
        Metric, MetricValue, completed_preliminary_rounds,
    },
};
use diesel::{dsl, prelude::*};
use rust_decimal::prelude::FromPrimitive;

pub struct AverageTotalSpeakerScoreComputer(pub HashMap<String, MetricValue>);

impl Metric<MetricValue> for AverageTotalSpeakerScoreComputer {
    fn compute(
        &self,
        tid: &str,
        conn: &mut impl diesel::connection::LoadConnection<
            Backend = diesel::sqlite::Sqlite,
        >,
    ) -> std::collections::HashMap<String, MetricValue> {
        tournament_teams::table
            .filter(tournament_teams::tournament_id.eq(tid))
            // for all completed preliminary rounds
            .inner_join(completed_preliminary_rounds())
            .inner_join(
                tournament_debates::table
                    .on(tournament_debates::round_id.eq(tournament_rounds::id)),
            )
            .inner_join(
                tournament_debate_teams::table
                    .on(
                        tournament_debate_teams::team_id
                            .eq(tournament_teams::id)
                            .and(
                                tournament_debate_teams::debate_id
                                    .eq(tournament_debates::id),
                            ),
                    ),
            )
            .group_by(tournament_teams::id)
            .select((
                tournament_teams::id,
                diesel::dsl::count(tournament_debates::id),
            ))
            .load::<(String, i64)>(conn)
            .unwrap()
            .into_iter()
            .map(|(a, b)| {
                let float = if b == 0 {
                    assert_eq!(
                        *self.0.get(&a).unwrap().as_float().unwrap(),
                        rust_decimal::Decimal::ZERO
                    );
                    MetricValue::Float(rust_decimal::Decimal::ZERO)
                } else {
                    MetricValue::Float(
                        self.0.get(&a).unwrap().as_float().unwrap()
                            / rust_decimal::Decimal::from_i64(b).unwrap_or_else(|| panic!("failed to convert {b} to rust_decimal::Decimal")),
                    )
                };
                (a, float)
            })
            .collect()
    }
}
