use std::collections::{HashMap, HashSet};

use diesel::prelude::*;

use crate::schema::{ballots, debates, rounds, team_ranks_of_ballot, teams};

pub fn ballot_points_of_team(
    (tid,): (&str,),
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> HashMap<String, i64> {
    let rows = team_ranks_of_ballot::table
        .inner_join(ballots::table)
        .inner_join(debates::table.on(ballots::debate_id.eq(debates::id)))
        .inner_join(rounds::table.on(debates::round_id.eq(rounds::id)))
        .filter(rounds::tournament_id.eq(tid))
        .filter(rounds::kind.eq("P"))
        .filter(rounds::completed.eq(true))
        .select((
            ballots::debate_id,
            ballots::judge_id,
            ballots::version,
            ballots::id,
            team_ranks_of_ballot::team_id,
            team_ranks_of_ballot::points,
        ))
        .load::<(String, String, i64, String, String, i64)>(conn)
        .expect("Failed to load ballot team points");

    let mut latest_version_by_debate_judge: HashMap<(String, String), i64> =
        HashMap::new();
    for (debate_id, judge_id, version, _, _, _) in &rows {
        latest_version_by_debate_judge
            .entry((debate_id.clone(), judge_id.clone()))
            .and_modify(|latest| *latest = (*latest).max(*version))
            .or_insert(*version);
    }

    let latest_ballot_ids = rows
        .iter()
        .filter_map(|(debate_id, judge_id, version, ballot_id, _, _)| {
            let latest = latest_version_by_debate_judge
                .get(&(debate_id.clone(), judge_id.clone()))?;
            (*version == *latest).then(|| ballot_id.clone())
        })
        .collect::<HashSet<_>>();

    let mut ballot_points = rows
        .into_iter()
        .filter(|(_, _, _, ballot_id, _, _)| {
            latest_ballot_ids.contains(ballot_id)
        })
        .fold(
            HashMap::new(),
            |mut points_by_team, (_, _, _, _, team_id, points)| {
                *points_by_team.entry(team_id).or_insert(0) += points;
                points_by_team
            },
        );

    for team_id in teams::table
        .filter(teams::tournament_id.eq(tid))
        .select(teams::id)
        .load::<String>(conn)
        .unwrap()
    {
        ballot_points.entry(team_id).or_insert(0);
    }

    ballot_points
}
