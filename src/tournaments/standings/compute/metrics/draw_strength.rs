use std::collections::HashMap;

use diesel::prelude::*;
use rust_decimal::prelude::ToPrimitive;

use crate::schema::{
    tournament_debate_teams, tournament_debates, tournament_rounds,
};

pub fn draw_strength_of_teams(
    (tid, team_points): (&str, HashMap<String, rust_decimal::Decimal>),
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> HashMap<String, i64> {
    let teams_of_debate: Vec<(String, String)> =
        tournament_debates::table
            .inner_join(
                tournament_rounds::table
                    .on(tournament_rounds::id.eq(tournament_debates::round_id)),
            )
            .filter(
                tournament_rounds::completed
                    .eq(true)
                    .and(tournament_rounds::kind.eq("P")),
            )
            .filter(tournament_rounds::tournament_id.eq(tid))
            .inner_join(tournament_debate_teams::table.on(
                tournament_debate_teams::debate_id.eq(tournament_debates::id),
            ))
            .select((tournament_debates::id, tournament_debate_teams::team_id))
            .load::<(String, String)>(conn)
            .unwrap();
    let teams_of_debate = teams_of_debate.into_iter().fold(
        HashMap::new(),
        |mut map, (debate, team)| {
            map.entry(debate)
                .and_modify(|teams: &mut Vec<String>| {
                    teams.push(team.clone());
                })
                .or_insert(vec![team]);

            map
        },
    );

    let mut ds: HashMap<String, i64> =
        HashMap::with_capacity(team_points.len());

    for (_, teams) in teams_of_debate {
        for team_a in &teams {
            for team_b in &teams {
                if team_a == team_b {
                    continue;
                }

                ds.entry(team_a.clone())
                    .and_modify(|entry| {
                        *entry +=
                            team_points.get(team_b).unwrap().to_i64().unwrap()
                    })
                    .or_insert(
                        team_points.get(team_b).unwrap().to_i64().unwrap(),
                    );
            }
        }
    }

    ds
}
