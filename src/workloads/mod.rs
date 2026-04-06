//! Test workloads for Abacus.

use crate::workloads::fuzzer_inputs::Action;
use crate::workloads::fuzzer_inputs::FuzzState;

mod fuzzer_inputs;

#[test]
pub fn fuzz() {
    let test_function = make_test_function();

    let result = fuzzcheck::fuzz_test(test_function)
        .default_mutator()
        .serde_serializer()
        .default_sensor_and_pool_with_custom_filter(|_, _| true)
        .arguments_from_cargo_fuzzcheck()
        .launch();
    assert!(!result.found_test_failure);
}

fn make_test_function() -> impl Fn(&[fuzzer_inputs::Action]) {
    use crate::workloads::fuzzer_inputs::Action;
    use diesel_migrations::MigrationHarness;
    use std::sync::Arc;

    let r2d2_thread_pool =
        Arc::new(scheduled_thread_pool::ScheduledThreadPool::new(8));

    let test_function = move |actions: &[Action]| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            use diesel::{
                SqliteConnection,
                r2d2::{ConnectionManager, Pool},
            };

            let pool = Pool::builder()
                .max_size(1)
                .thread_pool(r2d2_thread_pool.clone())
                .build(ConnectionManager::<SqliteConnection>::new(":memory:"))
                .unwrap();

            {
                let mut conn = pool.get().unwrap();
                conn.run_pending_migrations(crate::MIGRATIONS).unwrap();
            }

            let client = crate::config::create_app(pool.clone());

            let mut client = axum_test::TestServer::new(client).unwrap();
            let mut state = FuzzState::default();

            for action in actions {
                action.clone().run(&pool, &mut client, &mut state).await;
            }
        });
    };
    test_function
}

#[test]
fn fuzz_regression_1() {
    let test_function = make_test_function();
    (test_function)(&[])
}

#[test]
fn fuzz_regression_2() {
    let _ = tracing_subscriber::fmt().try_init();

    let test_function = make_test_function();
    let actions: Vec<Action> = serde_json::from_str(
        r#"[{"CreateTournament":{"name":"","abbrv":"","slug":""}}]"#,
    )
    .unwrap();
    (test_function)(&actions)
}

#[test]
fn fuzz_regression_3() {
    let _ = tracing_subscriber::fmt().try_init();

    let test_function = make_test_function();
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

    let test_function = make_test_function();
    let actions: Vec<Action> = serde_json::from_str(
        r#"[{"RegisterUser":{"username":"O","email":"b@k.org","password":"wombat"}}]"#,
    )
    .unwrap();
    (test_function)(&actions)
}
