use std::collections::HashMap;

use diesel::prelude::*;
use rust_decimal::Decimal;

use crate::schema::{debates, rounds, teams_of_debate};

pub fn teams_of_completed_debates(
    tid: &str,
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> HashMap<String, Vec<String>> {
    let teams_of_debate: Vec<(String, String)> = debates::table
        .inner_join(rounds::table.on(rounds::id.eq(debates::round_id)))
        .filter(rounds::completed.eq(true).and(rounds::kind.eq("P")))
        .filter(rounds::tournament_id.eq(tid))
        .inner_join(
            teams_of_debate::table
                .on(teams_of_debate::debate_id.eq(debates::id)),
        )
        .select((debates::id, teams_of_debate::team_id))
        .load::<(String, String)>(conn)
        .unwrap();
    teams_of_debate.into_iter().fold(
        HashMap::new(),
        |mut map, (debate, team)| {
            map.entry(debate)
                .and_modify(|teams: &mut Vec<String>| {
                    teams.push(team.clone());
                })
                .or_insert(vec![team]);

            map
        },
    )
}

pub fn draw_strength_of_teams(
    teams_of_debate: &HashMap<String, Vec<String>>,
    team_points: &HashMap<String, Decimal>,
) -> HashMap<String, Decimal> {
    let mut ds: HashMap<String, Decimal> = team_points
        .keys()
        .map(|team| (team.clone(), Decimal::ZERO))
        .collect();

    for teams in teams_of_debate.values() {
        for team_a in teams {
            for team_b in teams {
                if team_a == team_b {
                    continue;
                }

                *ds.get_mut(team_a).unwrap() +=
                    *team_points.get(team_b).unwrap();
            }
        }
    }

    ds
}
