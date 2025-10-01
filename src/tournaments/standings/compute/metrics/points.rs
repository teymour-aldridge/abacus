use diesel::prelude::*;

use crate::schema::{
    tournament_debate_team_results, tournament_debate_teams,
    tournament_debates, tournament_rounds, tournament_teams,
};
use crate::tournaments::standings::compute::metrics::{
    Metric, MetricValue, completed_preliminary_rounds,
};

pub struct TeamPointsComputer;

impl Metric<MetricValue> for TeamPointsComputer {
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

        let did_team_defeat_other_team =
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
                .single_value());

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
        .inner_join(
            tournament_debates::table.on(tournament_debates::round_id
                .eq(tournament_rounds::id)
                .and(team_is_in_this_debate)
                .and(other_team_is_in_this_debate)),
        )
        .filter(did_team_defeat_other_team)
        .group_by(team.field(tournament_teams::id))
        .select((
            team.field(tournament_teams::id),
            diesel::dsl::count(other_team.field(tournament_teams::id)),
        ))
        .load::<(String, i64)>(conn)
        .unwrap()
        .into_iter()
        .map(|(a, b)| (a, MetricValue::Integer(b)))
        .collect()
    }
}
