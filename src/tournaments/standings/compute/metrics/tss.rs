use crate::{
    schema::{
        tournament_debate_speaker_results, tournament_debates,
        tournament_draws,
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
                        rust_decimal::Decimal::from_f32_retain(b).expect(
                            &format!("could not convert `{b}` to rust_decimal"),
                        ),
                    ),
                )
            })
            .collect()
    }
}
