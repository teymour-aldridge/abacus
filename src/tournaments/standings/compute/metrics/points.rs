use diesel::prelude::*;

use crate::schema::{
    tournament_debate_team_results, tournament_debate_teams,
    tournament_debates, tournament_draws, tournament_rounds, tournament_teams,
};
use crate::tournaments::standings::compute::metrics::{
    Metric, MetricValue, completed_preliminary_rounds,
};

pub struct TeamPointsComputer;

impl Metric<MetricValue> for TeamPointsComputer {
    fn compute(
        &self,
        tid: &str,
        conn: &mut impl diesel::connection::LoadConnection<Backend = diesel::sqlite::Sqlite>,
    ) -> std::collections::HashMap<String, MetricValue> {
        let (team, other_team) = diesel::alias!(
            tournament_teams as team,
            tournament_teams as other_team
        );

        let team_is_in_this_debate = diesel::dsl::exists(
            tournament_debate_teams::table
                .filter(
                    tournament_debate_teams::team_id
                        .eq(team.field(tournament_teams::id)),
                )
                .filter(
                    tournament_debate_teams::debate_id
                        .eq(tournament_debates::id),
                ),
        );

        let other_team_is_in_this_debate = diesel::dsl::exists(
            tournament_debate_teams::table
                .filter(
                    tournament_debate_teams::team_id
                        .eq(other_team.field(tournament_teams::id)),
                )
                .filter(
                    tournament_debate_teams::debate_id
                        .eq(tournament_debates::id),
                ),
        );

        team.filter(
            team.field(tournament_teams::tournament_id)
                .eq(tid.to_string()),
        )
        .inner_join(
            other_team.on(other_team
                .field(tournament_teams::tournament_id)
                .eq(tid.to_string())),
        )
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
                    .select(diesel::dsl::max(
                        draws_subquery.field(tournament_draws::released_at),
                    ))
                    .single_value(),
            ))
        })
        .inner_join(
            tournament_debates::table.on(tournament_debates::draw_id
                .eq(tournament_draws::id)
                .and(team_is_in_this_debate)
                .and(other_team_is_in_this_debate)),
        )
        .filter(
            (tournament_debate_team_results::table
                .filter(
                    tournament_debate_team_results::team_id
                        .eq(team.field(tournament_teams::id)),
                )
                .filter(
                    tournament_debate_team_results::debate_id
                        .eq(tournament_debates::id),
                )
                .select(tournament_debate_team_results::points)
                .single_value())
            .gt(tournament_debate_team_results::table
                .filter(
                    tournament_debate_team_results::team_id
                        .eq(other_team.field(tournament_teams::id)),
                )
                .filter(
                    tournament_debate_team_results::debate_id
                        .eq(tournament_debates::id),
                )
                .select(tournament_debate_team_results::points)
                .single_value()),
        )
        .group_by(team.field(tournament_teams::id))
        .select((
            team.field(tournament_teams::id),
            diesel::dsl::count(other_team.field(tournament_teams::id)),
        ))
        .load::<(String, i64)>(conn)
        .unwrap()
        .into_iter()
        .map(|(a, b)| (a, MetricValue::Points(b)))
        .collect()
    }
}
