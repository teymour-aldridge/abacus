//! Test workloads for Abacus.

use crate::workloads::fuzzer_inputs::Action;

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
    use tokio::runtime::Runtime;

    let rt = Runtime::new().unwrap();

    use crate::workloads::fuzzer_inputs::Action;

    let test_function = move |actions: &[Action]| {
        rt.block_on(async {
            let _guard = rt.enter();

            use diesel::r2d2::{ConnectionManager, Pool};

            let pool = Pool::builder()
                .max_size(1)
                .build(ConnectionManager::new(":memory:"))
                .unwrap();

            let client = crate::config::create_app(pool.clone());

            let mut client = axum_test::TestServer::new(client).unwrap();

            for action in actions {
                action.clone().run(&pool, &mut client).await;
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
    tracing_subscriber::fmt().init();

    let test_function = make_test_function();
    let actions: Vec<Action> = serde_json::from_str(
        r#"[{"CreateTournament":{"name":"","abbrv":"","slug":""}}]"#,
    )
    .unwrap();
    (test_function)(&actions)
}

#[test]
fn fuzz_regression_3() {
    tracing_subscriber::fmt().init();

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
