use std::collections::HashMap;

use diesel::prelude::*;

use crate::{
    schema::{agg_team_results_of_debate, debates, rounds, teams},
    tournaments::teams::Team,
};

pub fn points_of_team(
    (tid,): (&str,),
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> HashMap<String, i64> {
    let results: Vec<(String, Option<i64>)> = agg_team_results_of_debate::table
        .inner_join(
            debates::table
                .on(agg_team_results_of_debate::debate_id.eq(debates::id)),
        )
        .inner_join(rounds::table.on(debates::round_id.eq(rounds::id)))
        .filter(rounds::tournament_id.eq(tid))
        .filter(rounds::kind.eq("P"))
        .filter(rounds::completed.eq(true))
        .select((
            agg_team_results_of_debate::team_id,
            agg_team_results_of_debate::points,
        ))
        .load::<(String, Option<i64>)>(conn)
        .expect("Failed to load team points");

    let mut team_points = std::collections::HashMap::new();

    for (team_id, points) in results {
        if let Some(pts) = points {
            *team_points.entry(team_id).or_insert(0i64) += pts;
        }
    }

    let teams = teams::table
        .filter(teams::tournament_id.eq(&tid))
        .load::<Team>(&mut *conn)
        .unwrap();

    for team in teams {
        team_points.entry(team.id).or_insert(0i64);
    }

    team_points
        .into_iter()
        .map(|(team_id, points)| (team_id, points))
        .collect()
}
