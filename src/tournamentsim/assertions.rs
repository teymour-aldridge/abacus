use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::schema::{
    ballots, debates, judges_of_debate, rounds, team_ranks_of_ballot,
    teams_of_debate, tournaments,
};
use crate::tournaments::config::RankableTeamMetric;
use crate::tournaments::standings::compute::TeamStandings;

pub fn assert_tournament_properties(
    pool: &diesel::r2d2::Pool<
        diesel::r2d2::ConnectionManager<diesel::SqliteConnection>,
    >,
) {
    let mut conn = pool.get().unwrap();
    assert_draw_team_uniqueness(&mut conn);
    assert_confirmed_ballots_are_complete(&mut conn);
    assert_saved_standings_match_recomputed_standings(&mut conn);
}

fn assert_draw_team_uniqueness(conn: &mut diesel::SqliteConnection) {
    let draw_teams = teams_of_debate::table
        .select((teams_of_debate::debate_id, teams_of_debate::team_id))
        .load::<(String, String)>(conn)
        .unwrap();
    let debate_rounds = debates::table
        .select((debates::id, debates::round_id))
        .load::<(String, String)>(conn)
        .unwrap()
        .into_iter()
        .collect::<HashMap<_, _>>();

    let mut teams_seen_in_debate = HashSet::new();
    let mut teams_seen_in_round = HashSet::new();
    for (debate_id, team_id) in draw_teams {
        assert!(
            teams_seen_in_debate.insert((debate_id.clone(), team_id.clone())),
            "team {team_id} appears more than once in debate {debate_id}",
        );

        let round_id = debate_rounds
            .get(&debate_id)
            .expect("teams_of_debate should reference an existing debate");
        assert!(
            teams_seen_in_round.insert((round_id.clone(), team_id.clone())),
            "team {team_id} is assigned to multiple debates in round {round_id}",
        );
    }
}

fn assert_confirmed_ballots_are_complete(conn: &mut diesel::SqliteConnection) {
    let confirmed_debates = debates::table
        .filter(debates::status.eq("confirmed"))
        .select(debates::id)
        .load::<String>(conn)
        .unwrap()
        .into_iter()
        .collect::<HashSet<_>>();
    if confirmed_debates.is_empty() {
        return;
    }

    let non_trainee_judges = judges_of_debate::table
        .filter(judges_of_debate::status.ne("T"))
        .select((judges_of_debate::debate_id, judges_of_debate::judge_id))
        .load::<(String, String)>(conn)
        .unwrap();
    let ballots = ballots::table
        .select((ballots::id, ballots::debate_id, ballots::judge_id))
        .load::<(String, String, String)>(conn)
        .unwrap();
    let debate_teams = teams_of_debate::table
        .select((teams_of_debate::debate_id, teams_of_debate::team_id))
        .load::<(String, String)>(conn)
        .unwrap();
    let ballot_team_ranks = team_ranks_of_ballot::table
        .select((
            team_ranks_of_ballot::ballot_id,
            team_ranks_of_ballot::team_id,
        ))
        .load::<(String, String)>(conn)
        .unwrap();

    let submitted_by_debate_judge = ballots
        .iter()
        .map(|(_, debate_id, judge_id)| {
            ((debate_id.clone(), judge_id.clone()), true)
        })
        .collect::<HashMap<_, _>>();
    for (debate_id, judge_id) in non_trainee_judges {
        if confirmed_debates.contains(&debate_id) {
            assert!(
                submitted_by_debate_judge
                    .contains_key(&(debate_id.clone(), judge_id.clone())),
                "confirmed debate {debate_id} is missing a ballot from judge {judge_id}",
            );
        }
    }

    let teams_by_debate = collect_sets_by_key(debate_teams.into_iter());
    let ranks_by_ballot = collect_sets_by_key(ballot_team_ranks.into_iter());
    for (ballot_id, debate_id, _judge_id) in ballots {
        if !confirmed_debates.contains(&debate_id) {
            continue;
        }

        let expected_teams =
            teams_by_debate.get(&debate_id).cloned().unwrap_or_default();
        let ranked_teams =
            ranks_by_ballot.get(&ballot_id).cloned().unwrap_or_default();
        assert_eq!(
            ranked_teams, expected_teams,
            "confirmed ballot {ballot_id} must rank exactly the teams in debate {debate_id}",
        );
    }
}

fn collect_sets_by_key(
    rows: impl Iterator<Item = (String, String)>,
) -> HashMap<String, HashSet<String>> {
    let mut grouped = HashMap::new();
    for (key, value) in rows {
        grouped
            .entry(key)
            .or_insert_with(HashSet::new)
            .insert(value);
    }
    grouped
}

#[derive(Debug, PartialEq)]
struct TeamStandingsSnapshot {
    metrics: Vec<RankableTeamMetric>,
    ranked_metrics_of_team: BTreeMap<String, Vec<(RankableTeamMetric, String)>>,
    pullup_metrics: BTreeMap<(String, String), String>,
    rank_of_team: BTreeMap<String, i64>,
    teams_in_rank_order: Vec<Vec<String>>,
}

impl TeamStandingsSnapshot {
    fn from_standings(standings: TeamStandings) -> Self {
        let ranked_metrics_of_team = standings
            .ranked_metrics_of_team
            .into_iter()
            .map(|(team_id, metrics)| {
                (
                    team_id,
                    metrics
                        .into_iter()
                        .map(|(kind, value)| {
                            (kind, value.normalize().to_string())
                        })
                        .collect(),
                )
            })
            .collect();

        let pullup_metrics = standings
            .pullup_metrics
            .into_iter()
            .map(|((team_id, kind), value)| {
                (
                    (team_id, serde_json::to_string(&kind).unwrap()),
                    value.normalize().to_string(),
                )
            })
            .collect();

        let rank_of_team = standings.rank_of_team.into_iter().collect();

        let teams_in_rank_order = standings
            .teams_in_rank_order
            .into_iter()
            .map(|rank| {
                let mut team_ids =
                    rank.into_iter().map(|team| team.id).collect::<Vec<_>>();
                team_ids.sort();
                team_ids
            })
            .collect();

        Self {
            metrics: standings.metrics,
            ranked_metrics_of_team,
            pullup_metrics,
            rank_of_team,
            teams_in_rank_order,
        }
    }
}

fn assert_saved_standings_match_recomputed_standings(
    conn: &mut diesel::SqliteConnection,
) {
    let tournament_ids = tournaments::table
        .inner_join(rounds::table)
        .filter(rounds::completed.eq(true))
        .select(tournaments::id)
        .distinct()
        .load::<String>(conn)
        .unwrap();

    for tournament_id in tournament_ids {
        let saved = TeamStandingsSnapshot::from_standings(
            TeamStandings::fetch(&tournament_id, conn),
        );
        let recomputed = TeamStandingsSnapshot::from_standings(
            TeamStandings::recompute(&tournament_id, conn),
        );

        assert_eq!(
            saved, recomputed,
            "saved team standings for tournament {tournament_id} do not match recomputed standings",
        );
    }
}
