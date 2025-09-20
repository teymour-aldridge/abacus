use std::collections::HashMap;

use diesel::prelude::*;

use crate::{
    schema::{
        tournament_debate_teams, tournament_debates, tournament_draws,
        tournament_rounds, tournament_teams,
    },
    tournaments::standings::compute::metrics::{
        Metric, MetricValue, completed_preliminary_rounds,
    },
};

/// This computes the draw strength for a given team, according to the
/// provided metric (i.e. the HashMap passed to this struct). Note that the
/// draw strength
pub struct DrawStrengthComputer<const FLOAT_METRIC: bool>(
    pub std::collections::HashMap<String, MetricValue>,
);

impl<const FLOAT_METRIC: bool> Metric<MetricValue>
    for DrawStrengthComputer<FLOAT_METRIC>
{
    fn compute(
        &self,
        tid: &str,
        conn: &mut impl diesel::connection::LoadConnection<
            Backend = diesel::sqlite::Sqlite,
        >,
    ) -> std::collections::HashMap<String, MetricValue> {
        let (team, other_team) = diesel::alias!(
            tournament_teams as team,
            tournament_teams as other_team
        );

        // First retrieve a list of (team_a, team_b) where team_a debated
        // against team_b.
        let teams_and_debated_against = team
            .filter(team.field(tournament_teams::id).eq(tid))
            .inner_join(completed_preliminary_rounds())
            .inner_join({
                // todo: surely there has to be a way to extract this into a
                // function that can be re-used across all the metrics
                let draws_subquery = diesel::alias!(tournament_draws as draws);

                tournament_draws::table.on(tournament_draws::released_at.ge(
                    draws_subquery
                        .filter(
                            draws_subquery
                                .field(tournament_draws::round_id)
                                .eq(tournament_rounds::id),
                        )
                        .select(diesel::dsl::max(
                            draws_subquery.field(tournament_draws::released_at),
                        ))
                        .single_value(),
                ))
            })
            // get all debates in which this team participated
            .inner_join(
                tournament_debates::table.on(tournament_debates::draw_id
                    .eq(tournament_draws::id)
                    .and(diesel::dsl::exists(
                        tournament_debate_teams::table.filter(
                            tournament_debate_teams::debate_id
                                .eq(tournament_debates::id)
                                .and(
                                    tournament_debate_teams::team_id
                                        .eq(team.field(tournament_teams::id)),
                                ),
                        ),
                    ))),
            )
            // then find all the other teams who participated in this debate
            .inner_join(
                other_team.on(other_team
                    .field(tournament_teams::id)
                    .ne(team.field(tournament_teams::id))
                    .and(diesel::dsl::exists(
                        // we check that there is a record denoting that
                        // `other_team` is also in this debate
                        tournament_debate_teams::table.filter(
                            tournament_debate_teams::debate_id
                                .eq(tournament_debates::id)
                                .and(tournament_debate_teams::team_id.eq(
                                    other_team.field(tournament_teams::id),
                                )),
                        ),
                    ))),
            )
            .select((
                team.field(tournament_teams::id),
                other_team.field(tournament_teams::id),
            ))
            .order_by(team.field(tournament_teams::id).asc())
            .load::<(String, String)>(conn)
            .unwrap();

        let mut ds_wins = HashMap::new();

        // todo: we could also place this in the SQL
        for (team, debated_against) in &teams_and_debated_against {
            let debated_against_metric =
                match (self.0.get(debated_against).unwrap(), FLOAT_METRIC) {
                    (MetricValue::Integer(p), false) => {
                        MetricValue::Integer(*p)
                    }
                    (MetricValue::Float(f), true) => MetricValue::Float(*f),
                    _ => unreachable!(),
                };
            ds_wins
                .entry(team.clone())
                .and_modify(|metric| match (metric, FLOAT_METRIC) {
                    (MetricValue::Integer(ds), false) => {
                        *ds += match debated_against_metric {
                            MetricValue::Integer(i) => i,
                            _ => unreachable!(),
                        }
                    }
                    (MetricValue::Float(ds), true) => {
                        *ds += match debated_against_metric {
                            MetricValue::Float(decimal) => decimal,
                            _ => unreachable!(),
                        }
                    }
                    _ => unreachable!(),
                })
                .or_insert(debated_against_metric);
        }

        ds_wins
    }
}
