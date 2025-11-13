use std::collections::HashMap;

use diesel::prelude::*;

use crate::{
    schema::{tournament_debates, tournament_rounds},
    tournaments::standings::compute::metrics::{
        Metric, MetricValue, points::TeamPointsComputer,
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
        use crate::schema::tournament_debate_teams;

        let wins = TeamPointsComputer::compute(&TeamPointsComputer, tid, conn);

        let debates: Vec<(String, String)> = tournament_debate_teams::table
            .inner_join(
                crate::schema::tournament_debates::table
                    .on(tournament_debate_teams::debate_id
                        .eq(crate::schema::tournament_debates::id)),
            )
            .inner_join(
                crate::schema::tournament_rounds::table
                    .on(tournament_rounds::id.eq(tournament_debates::round_id)),
            )
            .filter(tournament_rounds::completed.eq(true))
            .filter(crate::schema::tournament_debates::tournament_id.eq(tid))
            .select((
                tournament_debate_teams::debate_id,
                tournament_debate_teams::team_id,
            ))
            .load(conn)
            .unwrap();

        let mut debates_to_teams: HashMap<String, Vec<String>> = HashMap::new();
        for (debate_id, team_id) in debates {
            debates_to_teams.entry(debate_id).or_default().push(team_id);
        }

        let mut draw_strengths: HashMap<String, MetricValue> = HashMap::new();

        for teams_in_debate in debates_to_teams.values() {
            for team_id in teams_in_debate {
                let mut opponent_wins_sum = 0i64;
                for other_team_id in teams_in_debate {
                    if team_id != other_team_id {
                        opponent_wins_sum += wins
                            .get(other_team_id)
                            .unwrap()
                            .as_integer()
                            .unwrap()
                            .clone();
                    }
                }

                let entry = draw_strengths
                    .entry(team_id.clone())
                    .or_insert(MetricValue::Integer(opponent_wins_sum));
                *entry.as_integer_mut().unwrap() += opponent_wins_sum;
            }
        }

        draw_strengths
    }
}
