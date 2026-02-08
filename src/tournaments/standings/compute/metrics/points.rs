use std::collections::HashMap;

use diesel::prelude::*;

use crate::{
    schema::{
        tournament_debate_team_results, tournament_debates, tournament_rounds,
        tournament_teams,
    },
    tournaments::teams::Team,
};

pub fn points_of_team(
    (tid,): (&str,),
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> HashMap<String, i64> {
    let results: Vec<(String, Option<i64>)> =
        tournament_debate_team_results::table
            .inner_join(
                tournament_debates::table
                    .on(tournament_debate_team_results::debate_id
                        .eq(tournament_debates::id)),
            )
            .inner_join(
                tournament_rounds::table
                    .on(tournament_debates::round_id.eq(tournament_rounds::id)),
            )
            .filter(tournament_rounds::tournament_id.eq(tid))
            .filter(tournament_rounds::kind.eq("P"))
            .filter(tournament_rounds::completed.eq(true))
            .select((
                tournament_debate_team_results::team_id,
                tournament_debate_team_results::points,
            ))
            .load::<(String, Option<i64>)>(conn)
            .expect("Failed to load team points");

    let mut team_points = std::collections::HashMap::new();

    for (team_id, points) in results {
        if let Some(pts) = points {
            *team_points.entry(team_id).or_insert(0i64) += pts;
        }
    }

    let teams = tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(&tid))
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
