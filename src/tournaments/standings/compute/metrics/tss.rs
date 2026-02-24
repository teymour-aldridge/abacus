use std::collections::HashMap;

use crate::{
    schema::{
        agg_speaker_results_of_debate, debates,
        rounds::{self},
        teams,
    },
    tournaments::standings::compute::metrics::completed_preliminary_rounds,
};
use diesel::{dsl, prelude::*};

pub fn total_speaker_score_of_team(
    (tid,): (&str,),
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> HashMap<String, rust_decimal::Decimal> {
    teams::table
        .filter(teams::tournament_id.eq(tid))
        // for all completed preliminary rounds
        .inner_join(completed_preliminary_rounds())
        .inner_join(debates::table.on(debates::round_id.eq(rounds::id)))
        .inner_join(
            agg_speaker_results_of_debate::table.on(
                agg_speaker_results_of_debate::debate_id
                    .eq(debates::id)
                    .and(agg_speaker_results_of_debate::team_id.eq(teams::id)),
            ),
        )
        .group_by(teams::id)
        .select((
            teams::id,
            dsl::case_when(
                dsl::sum(agg_speaker_results_of_debate::score).is_not_null(),
                dsl::sum(agg_speaker_results_of_debate::score)
                    .assume_not_null(),
            )
            .otherwise(0.0),
        ))
        .load::<(String, f32)>(conn)
        .unwrap()
        .into_iter()
        .map(|(a, b)| {
            (
                a,
                rust_decimal::Decimal::from_f32_retain(b).unwrap_or_else(
                    || panic!("could not convert `{b}` to rust_decimal"),
                ),
            )
        })
        .collect()
}
