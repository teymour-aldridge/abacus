use diesel::r2d2::{ConnectionManager, Pool};
use diesel_migrations::MigrationHarness;
use once_cell::sync::Lazy;
use scheduled_thread_pool::ScheduledThreadPool;
use std::sync::Arc;

use crate::tournamentsim::{
    WorkloadInput, assertions::assert_tournament_properties, inputs::FuzzState,
};

static R2D2_THREAD_POOL: Lazy<Arc<ScheduledThreadPool>> = Lazy::new(|| {
    Arc::new(
        ScheduledThreadPool::builder()
            .num_threads(3)
            .thread_name_pattern("r2d2-fuzz-{}")
            .build(),
    )
});

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkloadReport {
    pub executed_actions: usize,
}

pub fn run_workload(input: &WorkloadInput) -> WorkloadReport {
    let pool = build_database();
    let app = crate::config::create_app(pool.clone());
    let mut client = axum_test::TestServer::new(app).unwrap();
    client.do_save_cookies();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut state = FuzzState::default();
    let mut report = WorkloadReport::default();
    rt.block_on(async {
        for action in input.actions.iter().cloned() {
            action.run(&pool, &mut client, &mut state).await;
            report.executed_actions += 1;
            assert_tournament_properties(&pool);
        }
    });
    report
}

fn build_database() -> Pool<ConnectionManager<diesel::SqliteConnection>> {
    let pool = Pool::builder()
        .max_size(1)
        .thread_pool(R2D2_THREAD_POOL.clone())
        .build(ConnectionManager::<diesel::SqliteConnection>::new(
            ":memory:",
        ))
        .unwrap();

    let mut conn = pool.get().unwrap();
    conn.run_pending_migrations(crate::MIGRATIONS).unwrap();
    drop(conn);
    pool
}
