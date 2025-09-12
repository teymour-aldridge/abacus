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

pub struct DsWinsComputer(pub std::collections::HashMap<String, MetricValue>);

impl Metric<MetricValue> for DsWinsComputer {
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
            .load::<(String, String)>(conn)
            .unwrap();

        let mut ds_wins = HashMap::new();

        // todo: we could also place this in the SQL
        for (team, debated_against) in &teams_and_debated_against {
            let points_of_team_hit = match self.0.get(debated_against).unwrap()
            {
                MetricValue::Points(p) => p,
                _ => unreachable!(),
            };
            ds_wins
                .entry(team.clone())
                .and_modify(|metric| match metric {
                    MetricValue::DsWins(ds) => *ds += *points_of_team_hit,
                    _ => unreachable!(),
                })
                .or_insert(MetricValue::DsWins(*points_of_team_hit));
        }

        ds_wins
    }
}
