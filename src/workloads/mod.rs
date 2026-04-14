//! Test workloads for Abacus.

use crate::workloads::fuzzer_inputs::Action;
use crate::workloads::fuzzer_inputs::FuzzState;
use diesel::{
    QueryableByName,
    sql_types::{BigInt, Text},
};
use fuzzcheck::sensors_and_pools::{
    AndPool, AndSensor, DifferentObservations, MaximiseEachCounterPool,
    MostNDiversePool,
};
use fuzzcheck::{DefaultMutator, PoolExt, SaveToStatsFolder, Sensor};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{Arc, Mutex},
};

mod fuzzer_inputs;

const DETERMINISM_REPETITIONS: usize = 3;
const DEFAULT_NON_DET_SEED: u64 = 0;
const DEFAULT_TURMOIL_SEED: u64 = 0;
const MAX_FUZZED_TURMOIL_SEED: u64 = 1_000_000_000;
const WORKLOAD_CACHE_CAPACITY: usize = 256;
const DATABASE_ENTROPY_COUNTERS: usize = 128;
const DATABASE_TABLE_PRESENCE_COUNTERS: usize = DATABASE_ENTROPY_COUNTERS;
const DATABASE_TABLE_PRESENCE_POOL_SIZE: usize = 64;

static R2D2_THREAD_POOL: Lazy<Arc<scheduled_thread_pool::ScheduledThreadPool>> =
    Lazy::new(|| Arc::new(scheduled_thread_pool::ScheduledThreadPool::new(1)));

static WORKLOAD_REPLAY_CACHE: Lazy<Mutex<WorkloadReplayCache>> =
    Lazy::new(|| Mutex::new(WorkloadReplayCache::new(WORKLOAD_CACHE_CAPACITY)));

static DATABASE_ENTROPY_OBSERVATIONS: Lazy<Mutex<Vec<(usize, u64)>>> =
    Lazy::new(|| Mutex::new(Vec::new()));
static DATABASE_TABLE_PRESENCE_OBSERVATIONS: Lazy<Mutex<Vec<(usize, u64)>>> =
    Lazy::new(|| Mutex::new(Vec::new()));

static RESTORED_DB_COUNTER: AtomicU64 = AtomicU64::new(0);
static WORKLOAD_CACHE_STATS: WorkloadCacheStats = WorkloadCacheStats::new();

#[derive(DefaultMutator, Clone, Debug, Hash, Serialize, Deserialize)]
struct WorkloadInput {
    actions: Vec<Action>,
    non_det_seed: u64,
    turmoil_seed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkloadOutput {
    captured_inputs: Vec<crate::non_det::CapturedInput>,
    db_snapshot: Vec<String>,
    fuzzer_state: FuzzState,
    non_det_probe: u64,
}

#[derive(Clone, Debug)]
struct CachedWorkload {
    snapshot: WorkloadSnapshot,
    output: Option<WorkloadOutput>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct WorkloadCacheKey {
    non_det_seed: u64,
    turmoil_seed: u64,
    prefix_len: usize,
    actions_hash: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkloadRunMode {
    Fuzz,
    Determinism,
}

impl WorkloadRunMode {
    fn collects_output(self) -> bool {
        matches!(self, Self::Determinism)
    }
}

#[derive(Clone)]
struct WorkloadSnapshot {
    prefix_len: usize,
    db_bytes: Vec<u8>,
    fuzzer_state: FuzzState,
    non_det: crate::non_det::NonDetSnapshot,
}

impl std::fmt::Debug for WorkloadSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkloadSnapshot")
            .field("prefix_len", &self.prefix_len)
            .field("db_bytes_len", &self.db_bytes.len())
            .field("fuzzer_state", &self.fuzzer_state)
            .finish_non_exhaustive()
    }
}

struct WorkloadCacheStats {
    runs: AtomicU64,
    exact_hits: AtomicU64,
    prefix_hits: AtomicU64,
    prefix_restored_actions: AtomicU64,
    executed_actions: AtomicU64,
    captured_snapshots: AtomicU64,
}

impl WorkloadCacheStats {
    const fn new() -> Self {
        Self {
            runs: AtomicU64::new(0),
            exact_hits: AtomicU64::new(0),
            prefix_hits: AtomicU64::new(0),
            prefix_restored_actions: AtomicU64::new(0),
            executed_actions: AtomicU64::new(0),
            captured_snapshots: AtomicU64::new(0),
        }
    }

    fn record_run(
        &self,
        actions_len: usize,
        restored_prefix_len: usize,
        exact_hit: bool,
    ) {
        let runs = self.runs.fetch_add(1, Ordering::Relaxed) + 1;
        if exact_hit {
            self.exact_hits.fetch_add(1, Ordering::Relaxed);
        }
        if restored_prefix_len > 0 {
            self.prefix_hits.fetch_add(1, Ordering::Relaxed);
            self.prefix_restored_actions
                .fetch_add(restored_prefix_len as u64, Ordering::Relaxed);
        }
        self.executed_actions.fetch_add(
            actions_len.saturating_sub(restored_prefix_len) as u64,
            Ordering::Relaxed,
        );

        if std::env::var_os("TABDA_WORKLOAD_CACHE_STATS").is_some()
            && runs.is_power_of_two()
        {
            eprintln!(
                "workload cache: runs={} exact_hits={} prefix_hits={} restored_actions={} executed_actions={} captured_snapshots={}",
                runs,
                self.exact_hits.load(Ordering::Relaxed),
                self.prefix_hits.load(Ordering::Relaxed),
                self.prefix_restored_actions.load(Ordering::Relaxed),
                self.executed_actions.load(Ordering::Relaxed),
                self.captured_snapshots.load(Ordering::Relaxed),
            );
        }
    }

    fn record_captured_snapshots(&self, count: usize) {
        self.captured_snapshots
            .fetch_add(count as u64, Ordering::Relaxed);
    }
}

#[derive(Debug)]
struct WorkloadReplayCache {
    capacity: usize,
    entries: HashMap<WorkloadCacheKey, CachedWorkload>,
    lru: VecDeque<WorkloadCacheKey>,
}

impl WorkloadReplayCache {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            lru: VecDeque::new(),
        }
    }

    fn get(&mut self, key: &WorkloadCacheKey) -> Option<CachedWorkload> {
        let entry = self.entries.get(key).cloned()?;
        self.touch(key);
        Some(entry)
    }

    fn insert(&mut self, key: WorkloadCacheKey, entry: CachedWorkload) {
        if self.capacity == 0 {
            return;
        }

        if !self.entries.contains_key(&key) {
            self.lru.push_back(key.clone());
        }
        self.entries.insert(key.clone(), entry);
        self.touch(&key);

        while self.entries.len() > self.capacity {
            if let Some(oldest) = self.lru.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }

    fn touch(&mut self, key: &WorkloadCacheKey) {
        self.lru.retain(|candidate| candidate != key);
        self.lru.push_back(key.clone());
    }

    fn get_greatest_prefix(
        &mut self,
        prefix_keys: &[WorkloadCacheKey],
    ) -> Option<CachedWorkload> {
        for key in prefix_keys.iter().rev() {
            if let Some(entry) = self.get(&key) {
                return Some(entry);
            }
        }
        None
    }
}

#[test]
pub fn fuzz() {
    let test_function = make_fuzz_test_function();
    let (coverage_sensor, coverage_pool) =
        fuzzcheck::builder::default_sensor_and_pool().finish();

    let result = fuzzcheck::fuzz_test(test_function)
        .default_mutator()
        .serde_serializer()
        .sensor_and_pool(
            AndSensor(coverage_sensor, database_sensor()),
            coverage_pool.and(database_pool(), None, DifferentObservations),
        )
        .arguments_from_cargo_fuzzcheck()
        .launch();
    assert!(!result.found_test_failure);
}

#[test]
pub fn fuzz_determinism() {
    let test_function = make_determinism_test_function();
    let (coverage_sensor, coverage_pool) =
        fuzzcheck::builder::default_sensor_and_pool().finish();

    let result = fuzzcheck::fuzz_test(test_function)
        .default_mutator()
        .serde_serializer()
        .sensor_and_pool(
            AndSensor(coverage_sensor, database_sensor()),
            coverage_pool.and(database_pool(), None, DifferentObservations),
        )
        .arguments_from_cargo_fuzzcheck()
        .launch();
    assert!(!result.found_test_failure);
}

fn make_fuzz_test_function() -> impl Fn(&[Action]) {
    |input: &[Action]| {
        let input = WorkloadInput {
            actions: input.to_vec(),
            non_det_seed: DEFAULT_NON_DET_SEED,
            turmoil_seed: DEFAULT_TURMOIL_SEED,
        };
        run_workload_in_turmoil(&input, WorkloadRunMode::Fuzz);
    }
}

fn make_determinism_test_function() -> impl Fn(&WorkloadInput) {
    |input: &WorkloadInput| {
        let first =
            run_workload_in_turmoil(input, WorkloadRunMode::Determinism)
                .unwrap();
        for _ in 1..DETERMINISM_REPETITIONS {
            assert_eq!(
                first,
                run_workload_in_turmoil(input, WorkloadRunMode::Determinism)
                    .unwrap()
            );
        }
    }
}

fn make_action_test_function() -> impl Fn(&[fuzzer_inputs::Action]) {
    |actions: &[Action]| {
        let input = WorkloadInput {
            actions: actions.to_vec(),
            non_det_seed: DEFAULT_NON_DET_SEED,
            turmoil_seed: DEFAULT_TURMOIL_SEED,
        };
        run_workload_in_turmoil(&input, WorkloadRunMode::Fuzz);
    }
}

async fn run_actions_async(
    actions: Vec<Action>,
    non_det: crate::non_det::NonDet,
    start: Option<WorkloadSnapshot>,
    collect_output: bool,
    collect_intermediate_snapshots: bool,
) -> WorkloadRunState {
    use diesel_migrations::MigrationHarness;

    let start_prefix_len =
        start.as_ref().map_or(0, |snapshot| snapshot.prefix_len);
    let start_db_bytes =
        start.as_ref().map(|snapshot| snapshot.db_bytes.as_slice());
    let database = build_workload_database(start_db_bytes);
    let pool = database.pool.clone();

    if start.is_none() {
        let mut conn = pool.get().unwrap();
        conn.run_pending_migrations(crate::MIGRATIONS).unwrap();
    }

    let client =
        crate::config::create_app_with_non_det(pool.clone(), non_det.clone());

    let mut client = axum_test::TestServer::new(client).unwrap();
    client.do_save_cookies();
    let mut state =
        start.as_ref().map_or_else(FuzzState::default, |snapshot| {
            snapshot.fuzzer_state.clone()
        });
    let mut snapshots = Vec::new();
    let actions_len = actions.len();

    if collect_intermediate_snapshots && start.is_none() {
        snapshots.push(capture_workload_snapshot(0, &pool, &state, &non_det));
    }

    for (idx, action) in actions.into_iter().enumerate() {
        action.run(&pool, &mut client, &mut state, &non_det).await;
        if collect_intermediate_snapshots {
            snapshots.push(capture_workload_snapshot(
                start_prefix_len + idx + 1,
                &pool,
                &state,
                &non_det,
            ));
        }
    }

    let final_prefix_len = start_prefix_len + actions_len;
    let final_snapshot_is_in_snapshots = snapshots
        .last()
        .is_some_and(|snapshot| snapshot.prefix_len == final_prefix_len);
    let final_snapshot = snapshots
        .last()
        .filter(|snapshot| snapshot.prefix_len == final_prefix_len)
        .cloned()
        .unwrap_or_else(|| {
            capture_workload_snapshot(final_prefix_len, &pool, &state, &non_det)
        });
    WORKLOAD_CACHE_STATS.record_captured_snapshots(
        snapshots.len() + usize::from(!final_snapshot_is_in_snapshots),
    );
    let final_entropy_observations = {
        let mut conn = pool.get().unwrap();
        database_entropy_observations(&mut conn)
    };
    let final_table_presence_observations = {
        let mut conn = pool.get().unwrap();
        database_table_presence_observations(&mut conn)
    };
    let final_db_snapshot = collect_output.then(|| {
        let mut conn = pool.get().unwrap();
        dump_database(&mut conn)
    });

    WorkloadRunState {
        final_snapshot,
        final_entropy_observations,
        final_table_presence_observations,
        final_db_snapshot,
        snapshots,
    }
}

#[derive(Debug)]
struct WorkloadRunState {
    final_snapshot: WorkloadSnapshot,
    final_entropy_observations: Vec<(usize, u64)>,
    final_table_presence_observations: Vec<(usize, u64)>,
    final_db_snapshot: Option<Vec<String>>,
    snapshots: Vec<WorkloadSnapshot>,
}

struct WorkloadDatabase {
    pool: diesel::r2d2::Pool<
        diesel::r2d2::ConnectionManager<diesel::SqliteConnection>,
    >,
    _restored_db_file: Option<RestoredDbFile>,
}

struct RestoredDbFile {
    path: PathBuf,
}

impl Drop for RestoredDbFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn build_workload_database(db_bytes: Option<&[u8]>) -> WorkloadDatabase {
    use diesel::{
        SqliteConnection,
        r2d2::{ConnectionManager, Pool},
    };

    let (database_url, restored_db_file) = if let Some(db_bytes) = db_bytes {
        let path = restored_database_path();
        std::fs::write(&path, db_bytes).unwrap();
        (
            path.to_string_lossy().to_string(),
            Some(RestoredDbFile { path }),
        )
    } else {
        (":memory:".to_string(), None)
    };

    let pool = Pool::builder()
        .max_size(1)
        .thread_pool(R2D2_THREAD_POOL.clone())
        .build(ConnectionManager::<SqliteConnection>::new(database_url))
        .unwrap();

    WorkloadDatabase {
        pool,
        _restored_db_file: restored_db_file,
    }
}

fn restored_database_path() -> PathBuf {
    let counter = RESTORED_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "tabda-workload-{}-{counter}.sqlite",
        std::process::id()
    ))
}

fn capture_workload_snapshot(
    prefix_len: usize,
    pool: &diesel::r2d2::Pool<
        diesel::r2d2::ConnectionManager<diesel::SqliteConnection>,
    >,
    state: &FuzzState,
    non_det: &crate::non_det::NonDet,
) -> WorkloadSnapshot {
    let mut conn = pool.get().unwrap();
    WorkloadSnapshot {
        prefix_len,
        db_bytes: conn.serialize_database_to_buffer().as_slice().to_vec(),
        fuzzer_state: state.clone(),
        non_det: non_det.snapshot(),
    }
}

#[derive(QueryableByName)]
struct SqliteTextValue {
    #[diesel(sql_type = Text)]
    value: String,
}

#[derive(QueryableByName)]
struct SqliteI64Value {
    #[diesel(sql_type = BigInt)]
    value: i64,
}

struct DatabaseEntropySensor;

struct DatabaseTablePresenceSensor;

fn database_sensor()
-> AndSensor<DatabaseEntropySensor, DatabaseTablePresenceSensor> {
    AndSensor(DatabaseEntropySensor, DatabaseTablePresenceSensor)
}

fn database_pool()
-> AndPool<MaximiseEachCounterPool, MostNDiversePool, DifferentObservations> {
    MaximiseEachCounterPool::new(
        "max_each_table_entropy",
        DATABASE_ENTROPY_COUNTERS,
    )
    .and(database_table_presence_pool(), None, DifferentObservations)
}

fn database_table_presence_pool() -> MostNDiversePool {
    MostNDiversePool::new(
        "diverse_non_empty_tables",
        DATABASE_TABLE_PRESENCE_POOL_SIZE,
        DATABASE_TABLE_PRESENCE_COUNTERS,
    )
}

impl SaveToStatsFolder for DatabaseEntropySensor {
    fn save_to_stats_folder(&self) -> Vec<(PathBuf, Vec<u8>)> {
        vec![]
    }
}

impl Sensor for DatabaseEntropySensor {
    type Observations = Vec<(usize, u64)>;

    fn start_recording(&mut self) {
        DATABASE_ENTROPY_OBSERVATIONS.lock().unwrap().clear();
    }

    fn stop_recording(&mut self) {}

    fn get_observations(&mut self) -> Self::Observations {
        DATABASE_ENTROPY_OBSERVATIONS.lock().unwrap().clone()
    }
}

fn record_database_entropy_observations(observations: Vec<(usize, u64)>) {
    *DATABASE_ENTROPY_OBSERVATIONS.lock().unwrap() = observations;
}

impl SaveToStatsFolder for DatabaseTablePresenceSensor {
    fn save_to_stats_folder(&self) -> Vec<(PathBuf, Vec<u8>)> {
        vec![]
    }
}

impl Sensor for DatabaseTablePresenceSensor {
    type Observations = Vec<(usize, u64)>;

    fn start_recording(&mut self) {
        DATABASE_TABLE_PRESENCE_OBSERVATIONS.lock().unwrap().clear();
    }

    fn stop_recording(&mut self) {}

    fn get_observations(&mut self) -> Self::Observations {
        DATABASE_TABLE_PRESENCE_OBSERVATIONS.lock().unwrap().clone()
    }
}

fn record_database_table_presence_observations(
    observations: Vec<(usize, u64)>,
) {
    *DATABASE_TABLE_PRESENCE_OBSERVATIONS.lock().unwrap() = observations;
}

fn run_actions_in_turmoil(
    actions: &[Action],
    non_det_seed: u64,
    turmoil_seed: u64,
    start: Option<WorkloadSnapshot>,
    mode: WorkloadRunMode,
) -> WorkloadRunResult {
    let start_prefix_len =
        start.as_ref().map_or(0, |snapshot| snapshot.prefix_len);
    let non_det = match start.as_ref() {
        Some(snapshot) => {
            crate::non_det::NonDet::from_snapshot(snapshot.non_det.clone())
        }
        None => crate::non_det::NonDet::deterministic_recording(non_det_seed),
    };

    let mut builder = turmoil::Builder::new();
    builder
        .rng_seed(normalize_fuzzed_turmoil_seed(turmoil_seed))
        .enable_random_order();
    let mut sim = builder.build();
    let run_state = Arc::new(Mutex::new(None));

    {
        let actions = actions[start_prefix_len..].to_vec();
        let non_det = non_det.clone();
        let run_state = run_state.clone();
        let start = start.clone();
        sim.client("workload", async move {
            let output = run_actions_async(
                actions,
                non_det,
                start,
                mode.collects_output(),
                mode == WorkloadRunMode::Determinism,
            )
            .await;
            *run_state.lock().unwrap() = Some(output);
            Ok(())
        });
    }

    sim.run().unwrap();

    let run_state = run_state
        .lock()
        .unwrap()
        .take()
        .expect("workload client did not run");
    let final_snapshot = run_state.final_snapshot;

    let output = mode.collects_output().then(|| WorkloadOutput {
        captured_inputs: non_det.captured_inputs(),
        db_snapshot: run_state
            .final_db_snapshot
            .expect("determinism mode should collect a final DB snapshot"),
        fuzzer_state: final_snapshot.fuzzer_state.clone(),
        non_det_probe: non_det.next_probe_u64(),
    });

    WorkloadRunResult {
        output,
        final_snapshot,
        final_entropy_observations: run_state.final_entropy_observations,
        final_table_presence_observations: run_state
            .final_table_presence_observations,
        snapshots: run_state.snapshots,
    }
}

#[derive(Debug)]
struct WorkloadRunResult {
    output: Option<WorkloadOutput>,
    final_snapshot: WorkloadSnapshot,
    final_entropy_observations: Vec<(usize, u64)>,
    final_table_presence_observations: Vec<(usize, u64)>,
    snapshots: Vec<WorkloadSnapshot>,
}

fn run_workload_in_turmoil(
    input: &WorkloadInput,
    mode: WorkloadRunMode,
) -> Option<WorkloadOutput> {
    let prefix_keys = workload_prefix_cache_keys(input);
    let full_cache_key = prefix_keys
        .last()
        .expect("prefix key list always includes the full input")
        .clone();
    let (start, expected_output) = {
        let mut cache = WORKLOAD_REPLAY_CACHE.lock().unwrap();
        let start = cache
            .get_greatest_prefix(&prefix_keys)
            .map(|cached| cached.snapshot);
        let expected_output =
            cache.get(&full_cache_key).and_then(|cached| cached.output);
        (start, expected_output)
    };
    let restored_prefix_len =
        start.as_ref().map_or(0, |snapshot| snapshot.prefix_len);
    let exact_cache_hit = restored_prefix_len == input.actions.len();
    WORKLOAD_CACHE_STATS.record_run(
        input.actions.len(),
        restored_prefix_len,
        exact_cache_hit,
    );

    if mode == WorkloadRunMode::Fuzz && exact_cache_hit {
        if let Some(snapshot) = start {
            record_database_entropy_observations(
                database_entropy_observations_from_snapshot(&snapshot),
            );
            record_database_table_presence_observations(
                database_table_presence_observations_from_snapshot(&snapshot),
            );
        }
        return expected_output;
    }

    let result = run_actions_in_turmoil(
        &input.actions,
        input.non_det_seed,
        input.turmoil_seed,
        start,
        mode,
    );

    if mode == WorkloadRunMode::Determinism {
        if let Some(expected_output) = expected_output {
            assert_eq!(Some(expected_output), result.output);
        }
    }
    record_database_entropy_observations(
        result.final_entropy_observations.clone(),
    );
    record_database_table_presence_observations(
        result.final_table_presence_observations.clone(),
    );

    let mut cache = WORKLOAD_REPLAY_CACHE.lock().unwrap();
    for snapshot in result.snapshots {
        let key = prefix_keys[snapshot.prefix_len].clone();
        let output = if snapshot.prefix_len == input.actions.len() {
            result.output.clone()
        } else {
            None
        };
        cache.insert(key, CachedWorkload { snapshot, output });
    }
    cache.insert(
        full_cache_key,
        CachedWorkload {
            snapshot: result.final_snapshot,
            output: result.output.clone(),
        },
    );
    result.output
}

fn normalize_fuzzed_turmoil_seed(seed: u64) -> u64 {
    seed % MAX_FUZZED_TURMOIL_SEED
}

fn database_entropy_observations_from_snapshot(
    snapshot: &WorkloadSnapshot,
) -> Vec<(usize, u64)> {
    let database = build_workload_database(Some(&snapshot.db_bytes));
    let mut conn = database.pool.get().unwrap();
    database_entropy_observations(&mut conn)
}

fn database_table_presence_observations_from_snapshot(
    snapshot: &WorkloadSnapshot,
) -> Vec<(usize, u64)> {
    let database = build_workload_database(Some(&snapshot.db_bytes));
    let mut conn = database.pool.get().unwrap();
    database_table_presence_observations(&mut conn)
}

fn database_entropy_observations(
    conn: &mut diesel::SqliteConnection,
) -> Vec<(usize, u64)> {
    let tables = workload_table_names(conn);
    assert!(
        tables.len() <= DATABASE_ENTROPY_COUNTERS,
        "database entropy sensor has {} counters but schema has {} tables",
        DATABASE_ENTROPY_COUNTERS,
        tables.len()
    );

    tables
        .into_iter()
        .enumerate()
        .map(|(counter, table_name)| {
            (
                counter,
                table_gini_simpson_proxy(conn, &table_name)
                    .try_into()
                    .unwrap_or(u64::MAX),
            )
        })
        .collect()
}

fn database_table_presence_observations(
    conn: &mut diesel::SqliteConnection,
) -> Vec<(usize, u64)> {
    let tables = workload_table_names(conn);
    assert!(
        tables.len() <= DATABASE_TABLE_PRESENCE_COUNTERS,
        "database table presence sensor has {} counters but schema has {} tables",
        DATABASE_TABLE_PRESENCE_COUNTERS,
        tables.len()
    );

    tables
        .into_iter()
        .enumerate()
        .filter_map(|(counter, table_name)| {
            (table_row_count(conn, &table_name) > 0).then_some((counter, 1))
        })
        .collect()
}

fn non_empty_workload_table_names(
    conn: &mut diesel::SqliteConnection,
) -> Vec<String> {
    workload_table_names(conn)
        .into_iter()
        .filter(|table_name| table_row_count(conn, table_name) > 0)
        .collect()
}

fn table_gini_simpson_proxy(
    conn: &mut diesel::SqliteConnection,
    table_name: &str,
) -> u128 {
    let total_rows = table_row_count(conn, table_name) as u128;
    if total_rows == 0 {
        return 0;
    }

    use diesel::{RunQueryDsl, sql_query};

    let table = quote_sqlite_identifier(table_name);
    let columns = table_columns(conn, table_name);
    let grouped_rows_query = if columns.is_empty() {
        format!("SELECT COUNT(*) AS freq FROM {table}")
    } else {
        let group_by = columns
            .iter()
            .map(|column| quote_sqlite_identifier(column))
            .collect::<Vec<_>>()
            .join(", ");
        format!("SELECT COUNT(*) AS freq FROM {table} GROUP BY {group_by}")
    };
    let sum_of_squares = sql_query(format!(
        "SELECT COALESCE(SUM(freq * freq), 0) AS value FROM ({grouped_rows_query})"
    ))
    .load::<SqliteI64Value>(conn)
    .unwrap()
    .pop()
    .unwrap()
    .value as u128;

    (total_rows * total_rows).saturating_sub(sum_of_squares)
}

fn table_row_count(
    conn: &mut diesel::SqliteConnection,
    table_name: &str,
) -> i64 {
    use diesel::{RunQueryDsl, sql_query};

    let table = quote_sqlite_identifier(table_name);
    sql_query(format!("SELECT COUNT(*) AS value FROM {table}"))
        .load::<SqliteI64Value>(conn)
        .unwrap()
        .pop()
        .unwrap()
        .value
}

fn workload_prefix_cache_keys(input: &WorkloadInput) -> Vec<WorkloadCacheKey> {
    let mut keys = Vec::with_capacity(input.actions.len() + 1);
    let mut actions_hash =
        hash_value(&(input.non_det_seed, input.turmoil_seed));
    keys.push(WorkloadCacheKey {
        non_det_seed: input.non_det_seed,
        turmoil_seed: input.turmoil_seed,
        prefix_len: 0,
        actions_hash,
    });

    for (idx, action) in input.actions.iter().enumerate() {
        actions_hash = hash_value(&(actions_hash, action));
        keys.push(WorkloadCacheKey {
            non_det_seed: input.non_det_seed,
            turmoil_seed: input.turmoil_seed,
            prefix_len: idx + 1,
            actions_hash,
        });
    }

    keys
}

fn hash_value<T: Hash + ?Sized>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn dump_database(conn: &mut diesel::SqliteConnection) -> Vec<String> {
    let mut snapshot = Vec::new();
    for table_name in workload_table_names(conn) {
        let columns = table_columns(conn, &table_name);
        snapshot.push(format!("table:{table_name}"));
        snapshot.push(format!("columns:{}", columns.join(",")));

        let rows = table_rows(conn, &table_name, &columns);
        snapshot.extend(rows.into_iter().map(|row| format!("row:{row}")));
    }
    snapshot
}

fn workload_table_names(conn: &mut diesel::SqliteConnection) -> Vec<String> {
    use diesel::{RunQueryDsl, sql_query};

    sql_query(
        "
        SELECT name AS value
        FROM sqlite_schema
        WHERE type = 'table'
          AND name != '__diesel_schema_migrations'
          AND name NOT LIKE 'sqlite_%'
        ORDER BY name
        ",
    )
    .load::<SqliteTextValue>(conn)
    .unwrap()
    .into_iter()
    .map(|table| table.value)
    .collect()
}

fn table_columns(
    conn: &mut diesel::SqliteConnection,
    table_name: &str,
) -> Vec<String> {
    use diesel::{RunQueryDsl, sql_query};

    let query = format!(
        "SELECT name AS value FROM pragma_table_info({}) ORDER BY cid",
        quote_sqlite_string(table_name)
    );

    sql_query(query)
        .load::<SqliteTextValue>(conn)
        .unwrap()
        .into_iter()
        .map(|column| column.value)
        .collect()
}

fn table_rows(
    conn: &mut diesel::SqliteConnection,
    table_name: &str,
    columns: &[String],
) -> Vec<String> {
    use diesel::{RunQueryDsl, sql_query};

    let table = quote_sqlite_identifier(table_name);
    let query = if columns.is_empty() {
        format!("SELECT '' AS value FROM {table}")
    } else {
        let value_expr = columns
            .iter()
            .map(|column| format!("quote({})", quote_sqlite_identifier(column)))
            .collect::<Vec<_>>()
            .join(" || char(31) || ");
        let order_expr = columns
            .iter()
            .map(|column| quote_sqlite_identifier(column))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "SELECT {value_expr} AS value FROM {table} ORDER BY {order_expr}"
        )
    };

    sql_query(query)
        .load::<SqliteTextValue>(conn)
        .unwrap()
        .into_iter()
        .map(|row| row.value)
        .collect()
}

fn quote_sqlite_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn quote_sqlite_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[test]
fn fuzz_regression_1() {
    let test_function = make_action_test_function();
    (test_function)(&[])
}

#[test]
fn fuzz_regression_2() {
    let _ = tracing_subscriber::fmt().try_init();

    let test_function = make_action_test_function();
    let actions: Vec<Action> = serde_json::from_str(
        r#"[{"CreateTournament":{"name":"","abbrv":"","slug":""}}]"#,
    )
    .unwrap();
    (test_function)(&actions)
}

#[test]
fn fuzz_regression_3() {
    let _ = tracing_subscriber::fmt().try_init();

    let test_function = make_action_test_function();
    let actions: Vec<Action> = serde_json::from_str(
        r#"
        [
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":"m"}},
            {"CreateTournament":{"name":"","abbrv":"3","slug":""}},
            {"CreateTournament":{"name":"","abbrv":"�","slug":""}}
        ]
    "#,
    )
    .unwrap();
    (test_function)(&actions)
}

#[test]
fn fuzz_regression_4() {
    let _ = tracing_subscriber::fmt().try_init();

    let test_function = make_action_test_function();
    let actions: Vec<Action> = serde_json::from_str(
        r#"[{"RegisterUser":{"username":"O","email":"b@k.org","password":"wombat"}}]"#,
    )
    .unwrap();
    (test_function)(&actions)
}

#[test]
fn fuzz_regression_5() {
    let _ = tracing_subscriber::fmt().try_init();

    let test_function = make_action_test_function();
    let actions: Vec<Action> = serde_json::from_str(
        r#"
        [
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"f@q.netcapybaraz@d.edumeerkatquetzalmeerkatfalconr@n.netpantherwcapybaran@f.orgg","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"tq@h.edu","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            "LogoutUser",
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"CreateTournament":{"name":"","abbrv":"","slug":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"","email":"","password":""}},
            {"RegisterUser":{"username":"X","email":"j","password":""}}
        ]
    "#,
    )
    .unwrap();
    (test_function)(&actions)
}

#[test]
fn fuzz_regression_6() {
    let _ = tracing_subscriber::fmt().try_init();

    let test_function = make_action_test_function();
    let actions: Vec<Action> = serde_json::from_str(
        r#"
            [
                {"RegisterUser":{"username":"otter","email":"lynxd@vtdt.orggecko","password":"badger"}},
                {"CreateTournament":{"name":"gecko","abbrv":"otter","slug":"R"}},
                {"CreateTournament":{"name":"gecko","abbrv":"otter","slug":"R"}}
            ]
        "#,
    )
    .unwrap();
    (test_function)(&actions)
}

#[test]
fn fuzz_regression_7() {
    let _ = tracing_subscriber::fmt().try_init();

    let test_function = make_action_test_function();
    let actions: Vec<Action> = serde_json::from_str(
        r#"
            [
                {"RegisterUser":{"username":"otter","email":"vtt@d.net","password":"qn@c.net"}},
                {"CreateTournament":{"name":"otter","abbrv":"ibex","slug":"lynx"}},
                {"CreateRound":{"tournament_idx":4504293825435551403,"name":"meerkat","category_idx":null,"seq":904821097}},{"CreateRound":{"tournament_idx":4504293825435551403,"name":"meerkat","category_idx":null,"seq":904821097}}
            ]
        "#,
    )
    .unwrap();
    (test_function)(&actions)
}

#[test]
fn determinism_regression_1() {
    let input = WorkloadInput {
        actions: serde_json::from_str(
            r#"
        [
            {"RegisterUser":{"username":"alice","email":"alice@example.com","password":"password1"}},
            {"CreateTournament":{"name":"Example Tournament","abbrv":"EX","slug":"example"}}
        ]
        "#,
        )
        .unwrap(),
        non_det_seed: 12345,
        turmoil_seed: 67890,
    };

    let reference =
        run_workload_in_turmoil(&input, WorkloadRunMode::Determinism).unwrap();

    for _ in 0..1000 {
        let new = run_workload_in_turmoil(&input, WorkloadRunMode::Determinism)
            .unwrap();
        assert_eq!(reference, new);
    }
}

#[test]
fn determinism_regression_2() {
    let input = WorkloadInput {
        actions: Vec::new(),
        non_det_seed: 14388258825268085747,
        turmoil_seed: 1105568808781615307,
    };

    let first =
        run_workload_in_turmoil(&input, WorkloadRunMode::Determinism).unwrap();
    let second =
        run_workload_in_turmoil(&input, WorkloadRunMode::Determinism).unwrap();

    assert_eq!(first, second);
}

#[test]
fn database_table_presence_observes_three_non_empty_tables() {
    let actions = vec![
        Action::RegisterUser {
            username: "alice".to_string(),
            email: "alice@example.com".to_string(),
            password: "password1".to_string(),
        },
        Action::LoginUser { user_idx: 0 },
        Action::CreateTournament {
            name: "Example Tournament".to_string(),
            abbrv: "EX".to_string(),
            slug: "example".to_string(),
        },
    ];
    let result = run_actions_in_turmoil(
        &actions,
        DEFAULT_NON_DET_SEED,
        DEFAULT_TURMOIL_SEED,
        None,
        WorkloadRunMode::Fuzz,
    );

    let database =
        build_workload_database(Some(&result.final_snapshot.db_bytes));
    let mut conn = database.pool.get().unwrap();
    let non_empty_tables = non_empty_workload_table_names(&mut conn);

    assert_eq!(
        non_empty_tables,
        vec![
            "org".to_string(),
            "tournament_presets".to_string(),
            "tournaments".to_string(),
            "users".to_string(),
        ]
    );
    assert_eq!(result.final_table_presence_observations.len(), 4);
}
