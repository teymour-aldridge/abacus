use diesel::{
    SqliteConnection,
    prelude::*,
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
    schema::{tournament_members, tournaments},
    state::{Conn, DbPool, TxCommitFairing},
    template::Page,
    tournaments::{
        Tournament,
        create::{create_tournament_page, do_create_tournament},
        manage::config::{
            update_tournament_configuration, view_tournament_configuration,
        },
        participants::manage::{
            create_judge::{create_judge_page, do_create_judge},
            create_speaker::{create_speaker_page, do_create_speaker},
            create_team::{create_teams_page, do_create_team},
            manage_judge::{do_edit_judge_details, edit_judge_details_page},
            manage_team::{
                do_edit_team_details, edit_team_details_page, manage_team_page,
            },
            manage_tournament_participants, tournament_participant_updates,
        },
        rounds::{
            draws::{
                manage::create::{do_generate_draw, generate_draw_page},
                public::view::view_active_draw_page,
            },
            manage::{
                availability::{
                    judges::{
                        update_judge_availability, view_judge_availability,
                    },
                    manage_round_availability,
                    teams::{
                        team_availability_updates, update_team_eligibility,
                        view_team_availability,
                    },
                },
                create::{
                    create_new_round,
                    create_new_round_of_specific_category_page,
                    do_create_new_round_of_specific_category,
                },
                draw_edit::{draw_updates, edit_draw_page, submit_cmd},
                edit::{do_edit_round, edit_round_page},
                manage_rounds_page,
                view::view_tournament_round_page,
            },
        },
        standings::public::public_team_tab_page,
        view::view_tournament_page,
    },
    util_resp::{StandardResponse, success},
    widgets::actions::Actions,
};

#[get("/")]
pub fn home(
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    if let Some(user) = user {
        let tournaments = tournaments::table
            .inner_join(
                tournament_members::table.on(tournament_members::tournament_id
                    .eq(tournaments::id)
                    .and(tournament_members::user_id.eq(&user.id))),
            )
            .select(tournaments::all_columns)
            .order_by(tournaments::created_at.desc())
            .load::<Tournament>(&mut *conn)
            .unwrap();

        let tournaments = maud! {
            h1 {"My tournaments"}
            @if tournaments.is_empty() {
                p {"You are not a member of any tournaments"}
            } @else {
                table class = "table" {
                    thead {
                        tr  {
                            th scope="col" {
                                "#"
                            }
                            th scope="col" {
                                "Tournament name"
                            }
                            th scope="col" {
                                "Actions"
                            }
                        }
                    }
                    tbody {
                        @for (i, tournament) in tournaments.iter().enumerate() {
                            tr {
                                th scope="col" {
                                    (i + 1)
                                }
                                td {
                                    (tournament.name)
                                }
                                td {
                                    a href=(format!("/tournaments/{}", tournament.id)) {
                                        "View"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };

        success(
            Page::new()
                .user(user)
                .body(maud! {
                    Actions options=(&[("/tournaments/create", "Create tournament")]);
                    (tournaments)
                })
                .render(),
        )
    } else {
        success(
            Page::new()
                .user_opt(None::<User<true>>)
                .body(maud! {
                    "Abacus"
                })
                .render(),
        )
    }
}

pub fn make_rocket() -> Rocket<Build> {
    let figment = rocket::Config::figment();

    let figment = if let Ok(secret) = std::env::var("SECRET_KEY") {
        figment.merge(("secret_key", secret))
    } else if cfg!(debug_assertions) {
        figment.merge(("secret_key", "0".repeat(88)))
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
                view_active_draw_page,
                edit_draw_page,
                draw_updates,
                submit_cmd,
                create_new_round,
                create_new_round_of_specific_category_page,
                edit_round_page,
                manage_rounds_page,
                view_tournament_round_page,
                public_team_tab_page,
                do_create_new_round_of_specific_category,
                do_edit_round,
                create_speaker_page,
                do_create_speaker,
                manage_round_availability,
                view_team_availability,
                update_team_eligibility,
                team_availability_updates,
                view_judge_availability,
                update_judge_availability,
                create_judge_page,
                do_create_judge,
                edit_judge_details_page,
                do_edit_judge_details,
                view_tournament_configuration,
                update_tournament_configuration
            ],
        )
}
