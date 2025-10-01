use crate::{
    schema::{
        tournament_debate_speaker_results, tournament_debates,
        tournament_rounds::{self},
        tournament_teams,
    },
    tournaments::standings::compute::metrics::{
        Metric, MetricValue, completed_preliminary_rounds,
    },
};
use diesel::{dsl, prelude::*};

pub struct TotalTeamSpeakerScoreComputer;

impl Metric<MetricValue> for TotalTeamSpeakerScoreComputer {
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
                tournament_debate_speaker_results::table.on(
                    tournament_debate_speaker_results::debate_id
                        .eq(tournament_debates::id)
                        .and(
                            tournament_debate_speaker_results::team_id
                                .eq(tournament_teams::id),
                        ),
                ),
            )
            .group_by(tournament_teams::id)
            .select((
                tournament_teams::id,
                dsl::case_when(
                    dsl::sum(tournament_debate_speaker_results::score)
                        .is_not_null(),
                    dsl::sum(tournament_debate_speaker_results::score)
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
                    MetricValue::Float(
                        rust_decimal::Decimal::from_f32_retain(b)
                            .unwrap_or_else(|| {
                                panic!(
                                    "could not convert `{b}` to rust_decimal"
                                )
                            }),
                    ),
                )
            })
            .collect()
    }
}
