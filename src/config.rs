use diesel::{
    SqliteConnection,
    r2d2::{ConnectionManager, Pool},
};
use diesel_migrations::MigrationHarness;
use rocket::{Build, Rocket, fairing::AdHoc};
use tokio::task::spawn_blocking;

use crate::{
    MIGRATIONS,
    msg::Msg,
    state::{DbPool, TxCommitFairing},
};

pub fn make_rocket() -> Rocket<Build> {
    let figment = rocket::Config::figment();

    let figment = if let Ok(secret) = std::env::var("SECRET_KEY") {
        figment.merge(("secret_key", secret))
    } else {
        figment
    };

    #[allow(unexpected_cfgs)]
    let figment = if cfg!(fuzzing) {
        figment.merge(("log_level", "off"))
    } else {
        figment
    };

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| ":memory:".to_string());

    let pool: DbPool =
        Pool::new(ConnectionManager::<SqliteConnection>::new(db_url)).unwrap();

    let (tx, rx) = tokio::sync::broadcast::channel::<Msg>(1000);

    rocket::custom(figment)
        .manage(pool)
        .manage(rx)
        .manage(tx)
        .attach(AdHoc::try_on_ignite("migrations", |rocket| async move {
            let conn = rocket.state::<DbPool>().unwrap().clone();

            let ret = spawn_blocking(move || {
                let mut conn = conn.get().unwrap();
                conn.run_pending_migrations(MIGRATIONS).unwrap();
            })
            .await;

            match ret {
                Ok(_) => Ok(rocket),
                Err(_) => Err(rocket),
            }
        }))
        .attach(TxCommitFairing)
}
