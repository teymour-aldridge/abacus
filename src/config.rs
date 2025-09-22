use diesel::{
    SqliteConnection,
    r2d2::{ConnectionManager, Pool},
};
use diesel_migrations::MigrationHarness;
use hypertext::prelude::*;
use rocket::{Build, Rocket, fairing::AdHoc, get, routes};
use tokio::task::spawn_blocking;

use crate::{
    MIGRATIONS,
    auth::{
        User,
        login::{do_login, login_page},
        register::{do_register, register_page},
    },
    msg::Msg,
    state::{DbPool, TxCommitFairing},
    template::Page,
    tournaments::{
        create::{create_tournament_page, do_create_tournament},
        participants::manage::{
            create_team::{create_teams_page, do_create_team},
            manage_team::{
                do_edit_team_details, edit_team_details_page, manage_team_page,
            },
            manage_tournament_participants, tournament_participant_updates,
        },
        rounds::{
            draws::{
                manage::{
                    create::{do_generate_draw, generate_draw_page},
                    edit::{draw_updates, submit_cmd_tab_dir},
                    view::view_draw,
                },
                public::view::view_active_draw_page,
            },
            manage::{
                create::{
                    create_new_round,
                    create_new_round_of_specific_category_page,
                },
                edit::edit_round_page,
                manage_rounds_page,
                view::view_tournament_round_page,
            },
        },
        standings::public::public_team_tab_page,
        view::view_tournament_page,
    },
    util_resp::{StandardResponse, success},
};

#[get("/")]
pub fn home(user: Option<User<true>>) -> StandardResponse {
    success(
        Page::new()
            .user_opt(user)
            .body(maud! {
                ul {
                    li {
                        a href="/tournaments/create" {
                            "Create new tournament"
                        }
                    }
                }
            })
            .render(),
    )
}

// #[derive(Debug)]
// struct Customizer;

// impl<C, E> CustomizeConnection<C, E> for Customizer
// where
//     C: Connection,
// {
//     fn on_acquire(&self, conn: &mut C) -> Result<(), E> {
//         conn.set_instrumentation(SimpleInstrument);

//         Ok(())
//     }

//     fn on_release(&self, conn: C) {}
// }

// struct SimpleInstrument;

// impl Instrumentation for SimpleInstrument {
//     fn on_connection_event(
//         &mut self,
//         event: diesel::connection::InstrumentationEvent<'_>,
//     ) {
//     }
// }

pub fn make_rocket() -> Rocket<Build> {
    let figment = rocket::Config::figment();

    let figment = if let Ok(secret) = std::env::var("SECRET_KEY") {
        figment.merge(("secret_key", secret))
    } else if cfg!(test) {
        figment.merge(("secret_key", "0".repeat(64)))
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

    println!("location = {db_url}");

    let pool: DbPool = Pool::builder()
        .max_size(if db_url == ":memory:" { 1 } else { 10 })
        .build(ConnectionManager::<SqliteConnection>::new(db_url))
        .unwrap();

    let (tx, rx) = tokio::sync::broadcast::channel::<Msg>(1000);

    rocket::custom(figment)
        .manage(pool)
        .manage(rx)
        .manage(tx)
        .attach(TxCommitFairing)
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
        .mount(
            "/",
            routes![
                home,
                login_page,
                do_login,
                register_page,
                do_register,
                create_tournament_page,
                do_create_tournament,
                view_tournament_page,
                create_teams_page,
                do_create_team,
                manage_team_page,
                edit_team_details_page,
                do_edit_team_details,
                manage_tournament_participants,
                tournament_participant_updates,
                generate_draw_page,
                do_generate_draw,
                view_draw,
                view_active_draw_page,
                draw_updates,
                submit_cmd_tab_dir,
                create_new_round,
                create_new_round_of_specific_category_page,
                edit_round_page,
                manage_rounds_page,
                view_tournament_round_page,
                public_team_tab_page,
            ],
        )
}
