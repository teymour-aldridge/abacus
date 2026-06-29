use diesel::sql_types::BigInt;
use diesel::{
    ExpressionMethods, JoinOnDsl, QueryDsl, QueryableByName, RunQueryDsl,
    sql_query,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

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
    assert_no_cross_tournament_references(&mut conn);
    assert_draw_team_uniqueness(&mut conn);
    assert_draw_judge_allocation_invariants(&mut conn);
    assert_confirmed_ballots_are_complete(&mut conn);
    assert_saved_standings_match_recomputed_standings(&mut conn);
}

#[derive(QueryableByName)]
struct CrossTournamentMismatch {
    #[diesel(sql_type = diesel::sql_types::Text)]
    relationship: String,
    #[diesel(sql_type = BigInt)]
    mismatch_count: i64,
}

#[derive(QueryableByName)]
struct SchemaTable {
    #[diesel(sql_type = diesel::sql_types::Text)]
    name: String,
}

#[derive(QueryableByName)]
struct SchemaColumn {
    #[diesel(sql_type = diesel::sql_types::Text)]
    name: String,
}

#[derive(QueryableByName)]
struct SchemaForeignKeyRow {
    #[diesel(sql_type = BigInt)]
    id: i64,
    #[diesel(sql_type = BigInt)]
    seq: i64,
    #[diesel(sql_type = diesel::sql_types::Text)]
    parent_table: String,
    #[diesel(sql_type = diesel::sql_types::Text)]
    child_column: String,
    #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
    parent_column: Option<String>,
}

#[derive(Clone)]
struct CrossTournamentCheck {
    relationship: String,
    sql: String,
}

struct CrossTournamentAssertionPlan {
    sql: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct ForeignKey {
    parent_table: String,
    column_pairs: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
struct TableSchema {
    columns: HashSet<String>,
    foreign_keys: Vec<ForeignKey>,
}

type SchemaGraph = BTreeMap<String, TableSchema>;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct TournamentPath {
    description: String,
    joins: Vec<ForeignKey>,
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn quote_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn read_schema_graph(conn: &mut diesel::SqliteConnection) -> SchemaGraph {
    let tables = sql_query(
        "SELECT name \
         FROM sqlite_schema \
         WHERE type = 'table' \
           AND name NOT LIKE 'sqlite_%' \
         ORDER BY name",
    )
    .load::<SchemaTable>(conn)
    .unwrap();

    let mut graph = BTreeMap::new();
    for table in tables {
        let table_name_literal = quote_string(&table.name);
        let columns = sql_query(format!(
            "SELECT name FROM pragma_table_info({table_name_literal})"
        ))
        .load::<SchemaColumn>(conn)
        .unwrap()
        .into_iter()
        .map(|column| column.name)
        .collect::<HashSet<_>>();

        let foreign_key_rows = sql_query(format!(
            "SELECT id, seq, \"table\" AS parent_table, \
                    \"from\" AS child_column, \"to\" AS parent_column \
             FROM pragma_foreign_key_list({table_name_literal}) \
             ORDER BY id, seq"
        ))
        .load::<SchemaForeignKeyRow>(conn)
        .unwrap();

        let mut grouped_foreign_keys =
            BTreeMap::<i64, (String, Vec<(i64, String, String)>)>::new();
        for row in foreign_key_rows {
            let parent_column =
                row.parent_column.unwrap_or_else(|| "id".to_string());
            let entry = grouped_foreign_keys
                .entry(row.id)
                .or_insert_with(|| (row.parent_table, Vec::new()));
            entry.1.push((row.seq, row.child_column, parent_column));
        }

        let foreign_keys = grouped_foreign_keys
            .into_values()
            .map(|(parent_table, mut column_pairs)| {
                column_pairs.sort_by_key(|(seq, _, _)| *seq);
                ForeignKey {
                    parent_table,
                    column_pairs: column_pairs
                        .into_iter()
                        .map(|(_, child_column, parent_column)| {
                            (child_column, parent_column)
                        })
                        .collect(),
                }
            })
            .collect();

        graph.insert(
            table.name,
            TableSchema {
                columns,
                foreign_keys,
            },
        );
    }

    graph
}

fn direct_tournament_path(
    table_name: &str,
    table: &TableSchema,
) -> Option<TournamentPath> {
    table
        .columns
        .contains("tournament_id")
        .then(|| TournamentPath {
            description: format!("{table_name}.tournament_id"),
            joins: Vec::new(),
        })
}

fn prepend_foreign_key_path(
    table_name: &str,
    foreign_key: &ForeignKey,
    parent_path: TournamentPath,
) -> TournamentPath {
    let mut joins = Vec::with_capacity(parent_path.joins.len() + 1);
    joins.push(foreign_key.clone());
    joins.extend(parent_path.joins);

    let through_columns = foreign_key
        .column_pairs
        .iter()
        .map(|(child_column, _)| child_column.as_str())
        .collect::<Vec<_>>()
        .join("+");
    let description = format!(
        "{table_name}.{through_columns} -> {}",
        parent_path.description
    );

    TournamentPath { description, joins }
}

fn canonical_tournament_path(
    table_name: &str,
    graph: &SchemaGraph,
) -> Option<TournamentPath> {
    let mut visiting = HashSet::new();
    canonical_tournament_path_inner(table_name, graph, &mut visiting)
}

fn canonical_tournament_path_inner(
    table_name: &str,
    graph: &SchemaGraph,
    visiting: &mut HashSet<String>,
) -> Option<TournamentPath> {
    let Some(table) = graph.get(table_name) else {
        return None;
    };

    if let Some(path) = direct_tournament_path(table_name, table) {
        return Some(path);
    }

    visiting.insert(table_name.to_string());
    let mut candidates = Vec::new();
    for foreign_key in &table.foreign_keys {
        if visiting.contains(&foreign_key.parent_table) {
            continue;
        }

        if let Some(parent_path) = canonical_tournament_path_inner(
            &foreign_key.parent_table,
            graph,
            visiting,
        ) {
            candidates.push(prepend_foreign_key_path(
                table_name,
                foreign_key,
                parent_path,
            ));
        }
    }
    visiting.remove(table_name);

    candidates
        .into_iter()
        .min_by_key(|path| (path.joins.len(), path.description.clone()))
}

fn tournament_paths_for_table(
    table_name: &str,
    graph: &SchemaGraph,
) -> Vec<TournamentPath> {
    let Some(table) = graph.get(table_name) else {
        return Vec::new();
    };

    let mut paths = BTreeMap::new();
    if let Some(path) = direct_tournament_path(table_name, table) {
        paths.insert(path.description.clone(), path);
    }

    for foreign_key in &table.foreign_keys {
        if let Some(parent_path) =
            canonical_tournament_path(&foreign_key.parent_table, graph)
        {
            let path =
                prepend_foreign_key_path(table_name, foreign_key, parent_path);
            paths.insert(path.description.clone(), path);
        }
    }

    paths.into_values().collect()
}

fn tournament_checks_for_table(
    table_name: &str,
    graph: &SchemaGraph,
) -> Vec<CrossTournamentCheck> {
    let Some(canonical_path) = canonical_tournament_path(table_name, graph)
    else {
        return Vec::new();
    };

    tournament_paths_for_table(table_name, graph)
        .into_iter()
        .filter(|path| path != &canonical_path)
        .map(|path| {
            build_cross_tournament_check(table_name, &canonical_path, &path)
        })
        .collect()
}

fn render_join_path(
    path: &TournamentPath,
    path_index: usize,
) -> (String, String) {
    if path.joins.is_empty() {
        return ("".to_string(), "child.tournament_id".to_string());
    }

    let mut joins = Vec::new();
    let mut previous_alias = "child".to_string();
    for (step_index, foreign_key) in path.joins.iter().enumerate() {
        let alias = format!("path{path_index}_step{step_index}");
        let conditions = foreign_key
            .column_pairs
            .iter()
            .map(|(child_column, parent_column)| {
                format!(
                    "{}.{} = {}.{}",
                    previous_alias,
                    quote_identifier(child_column),
                    alias,
                    quote_identifier(parent_column)
                )
            })
            .collect::<Vec<_>>()
            .join(" AND ");

        joins.push(format!(
            "JOIN {} {alias} ON {conditions}",
            quote_identifier(&foreign_key.parent_table),
        ));
        previous_alias = alias;
    }

    (
        joins.join(" "),
        format!("{previous_alias}.{}", quote_identifier("tournament_id")),
    )
}

fn build_cross_tournament_check(
    table_name: &str,
    left_path: &TournamentPath,
    right_path: &TournamentPath,
) -> CrossTournamentCheck {
    let (left_joins, left_tournament_id) = render_join_path(left_path, 0);
    let (right_joins, right_tournament_id) = render_join_path(right_path, 1);
    let relationship = format!(
        "{}: {} matches {}",
        table_name, left_path.description, right_path.description
    );
    let sql = format!(
        "SELECT COUNT(*) AS mismatch_count \
         FROM {} child \
         {} \
         {} \
         WHERE {} != {}",
        quote_identifier(table_name),
        left_joins,
        right_joins,
        left_tournament_id,
        right_tournament_id,
    );

    CrossTournamentCheck { relationship, sql }
}

fn build_cross_tournament_checks(
    conn: &mut diesel::SqliteConnection,
) -> Vec<CrossTournamentCheck> {
    let graph = read_schema_graph(conn);
    let mut checks = Vec::new();

    for table_name in graph.keys() {
        checks.extend(tournament_checks_for_table(table_name, &graph));
    }

    checks
}

fn build_cross_tournament_assertion_plan(
    conn: &mut diesel::SqliteConnection,
) -> CrossTournamentAssertionPlan {
    let checks = build_cross_tournament_checks(conn);
    let sql = (!checks.is_empty()).then(|| {
        checks
            .iter()
            .map(|check| {
                format!(
                    "SELECT {} AS relationship, \
                            ({}) AS mismatch_count",
                    quote_string(&check.relationship),
                    check.sql,
                )
            })
            .collect::<Vec<_>>()
            .join(" UNION ALL ")
    });

    CrossTournamentAssertionPlan { sql }
}

fn cross_tournament_assertion_plan(
    conn: &mut diesel::SqliteConnection,
) -> &'static CrossTournamentAssertionPlan {
    static PLAN: OnceLock<CrossTournamentAssertionPlan> = OnceLock::new();
    PLAN.get_or_init(|| build_cross_tournament_assertion_plan(conn))
}

fn assert_no_cross_tournament_references(conn: &mut diesel::SqliteConnection) {
    let Some(sql) = cross_tournament_assertion_plan(conn).sql.as_ref() else {
        return;
    };

    for mismatch in sql_query(sql)
        .load::<CrossTournamentMismatch>(conn)
        .unwrap()
    {
        assert_eq!(
            mismatch.mismatch_count,
            0,
            "{} has {mismatch_count} cross-tournament references",
            mismatch.relationship,
            mismatch_count = mismatch.mismatch_count,
        );
    }
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

fn assert_draw_judge_allocation_invariants(
    conn: &mut diesel::SqliteConnection,
) {
    let judge_allocations = judges_of_debate::table
        .inner_join(
            debates::table.on(debates::id.eq(judges_of_debate::debate_id)),
        )
        .inner_join(rounds::table.on(rounds::id.eq(debates::round_id)))
        .select((
            judges_of_debate::debate_id,
            judges_of_debate::judge_id,
            judges_of_debate::status,
            debates::tournament_id,
            rounds::seq,
        ))
        .load::<(String, String, String, String, i64)>(conn)
        .unwrap();

    let mut chairs_by_debate = HashMap::<String, String>::new();
    let mut judges_seen_in_seq = HashSet::new();
    for (debate_id, judge_id, status, tournament_id, round_seq) in
        judge_allocations
    {
        assert!(
            matches!(status.as_str(), "C" | "P" | "T"),
            "judge {judge_id} has invalid role {status} in debate {debate_id}",
        );

        if status == "C" {
            let previous_chair =
                chairs_by_debate.insert(debate_id.clone(), judge_id.clone());
            assert!(
                previous_chair.is_none(),
                "debate {debate_id} has multiple chairs: {} and {judge_id}",
                previous_chair.unwrap_or_default(),
            );
        }

        assert!(
            judges_seen_in_seq.insert((
                tournament_id.clone(),
                round_seq,
                judge_id.clone()
            )),
            "judge {judge_id} is allocated more than once in tournament {tournament_id} round sequence {round_seq}",
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
