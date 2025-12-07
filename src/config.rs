use axum::{
    Router,
    routing::{get, post},
};
use axum_extra::extract::cookie::Key;
use diesel::{
    SqliteConnection,
    prelude::*,
    r2d2::{ConnectionManager, Pool},
};
use diesel_migrations::MigrationHarness;
use hypertext::prelude::*;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use crate::{
    MIGRATIONS,
    // auth::{
    //     User,
    //     login::{do_login, login_page},
    //     register::{do_register, register_page},
    // },
    auth::User,
    // msg::Msg,
    schema::{tournament_members, tournaments},
    state::{Conn, DbPool},
    template::Page,
    tournaments::Tournament,
    util_resp::{StandardResponse, success},
    widgets::actions::Actions,
    // widgets::actions::Actions,
};

async fn home(
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    if let Some(user) = user {
        let tournaments_list = tournaments::table
            .inner_join(
                tournament_members::table.on(tournament_members::tournament_id
                    .eq(tournaments::id)
                    .and(tournament_members::user_id.eq(&user.id))),
            )
            .select(tournaments::all_columns)
            .order_by(tournaments::created_at.desc())
            .load::<Tournament>(&mut *conn)
            .unwrap(); // In production avoid unwrap

        let tournaments_html = maud! {
            h1 {"My tournaments"}
            @if tournaments_list.is_empty() {
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
                        @for (i, tournament) in tournaments_list.iter().enumerate() {
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
                    div class="p-3" {
                        Actions options=(&[("/tournaments/create", "Create tournament")]);
                        (tournaments_html)
                    }
                })
                .render(),
        )
    } else {
        success(
            Page::new()
                .user_opt(None::<User<true>>)
                .body(maud! {
                    div class="container-fluid p-3" {
                        h1 { "Abacus" }
                        p { "Welcome to Abacus." }
                    }
                })
                .render(),
        )
    }
}

pub async fn run() {
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| ":memory:".to_string());

    let pool: DbPool = Pool::builder()
        .max_size(if db_url == ":memory:" { 1 } else { 10 })
        .build(ConnectionManager::<SqliteConnection>::new(db_url))
        .unwrap();

    // Run migrations
    {
        let conn = pool.get().unwrap();
        // wrapper for migration harness
        let mut conn = conn;
        conn.run_pending_migrations(MIGRATIONS).unwrap();
    }

    let secret_str = std::env::var("SECRET_KEY").ok();
    let key = if let Some(secret) = secret_str.filter(|s| s.len() >= 64) {
        Key::from(secret.as_bytes())
    } else {
        if cfg!(debug_assertions) {
            Key::from(&[0; 64])
        } else {
            Key::generate()
        }
    };

    let (tx, _rx) = tokio::sync::broadcast::channel::<crate::msg::Msg>(1000);

    let state = crate::state::AppState { pool, key };

    let app = Router::new()
        .route("/", get(home))
        .route("/login", get(crate::auth::login::login_page).post(crate::auth::login::do_login))
        .route("/register", get(crate::auth::register::register_page).post(crate::auth::register::do_register))
        .route("/tournaments/create", get(crate::tournaments::create::create_tournament_page).post(crate::tournaments::create::do_create_tournament))
        .route("/tournaments/:id", get(crate::tournaments::view::view_tournament_page))
        
        // Participants
        .route("/tournaments/:id/participants/team", get(crate::tournaments::participants::manage::manage_team::manage_team_page))
        .route("/tournaments/:id/participants/team/create", get(crate::tournaments::participants::manage::create_team::create_teams_page).post(crate::tournaments::participants::manage::create_team::do_create_team))
        .route("/tournaments/:id/participants/team/edit", get(crate::tournaments::participants::manage::manage_team::edit_team_details_page).post(crate::tournaments::participants::manage::manage_team::do_edit_team_details))
        
        .route("/tournaments/:id/participants/judge/create", get(crate::tournaments::participants::manage::create_judge::create_judge_page).post(crate::tournaments::participants::manage::create_judge::do_create_judge))
        .route("/tournaments/:id/participants/judge/edit", get(crate::tournaments::participants::manage::manage_judge::edit_judge_details_page).post(crate::tournaments::participants::manage::manage_judge::do_edit_judge_details))
        
        .route("/tournaments/:id/participants/speaker/create", get(crate::tournaments::participants::manage::create_speaker::create_speaker_page).post(crate::tournaments::participants::manage::create_speaker::do_create_speaker))
        
        .route("/tournaments/:id/participants/private_urls", get(crate::tournaments::participants::manage::manage_private_urls::view_private_urls))
        // WebSocket for participants
        .route("/tournaments/:id/participants/updates", get(crate::tournaments::participants::manage::tournament_participant_updates))

        // Configuration
        .route("/tournaments/:id/config", get(crate::tournaments::manage::config::view_tournament_configuration).post(crate::tournaments::manage::config::update_tournament_configuration))
        .route("/tournaments/:id/manage", get(crate::tournaments::manage::view::admin_view_tournament))

        // Feedback
        .route("/tournaments/:id/feedback/manage", get(crate::tournaments::feedback::manage::config::manage_feedback_page))
        .route("/tournaments/:id/feedback/manage/add", post(crate::tournaments::feedback::manage::config::add_feedback_question))
        .route("/tournaments/:id/feedback/manage/delete", post(crate::tournaments::feedback::manage::config::delete_feedback_question))
        .route("/tournaments/:id/feedback/manage/up", post(crate::tournaments::feedback::manage::config::move_feedback_question_up))
        .route("/tournaments/:id/feedback/manage/down", post(crate::tournaments::feedback::manage::config::move_feedback_question_down))
        .route("/tournaments/:id/feedback/manage/:question_id/edit", get(crate::tournaments::feedback::manage::config::edit_feedback_question_page).post(crate::tournaments::feedback::manage::config::edit_feedback_question))
        .route("/tournaments/:id/feedback/table", get(crate::tournaments::feedback::manage::table::feedback_table_page))
        .route("/tournaments/:id/privateurls/:private_url/rounds/:round_id/feedback/submit", get(crate::tournaments::feedback::public::submit::submit_feedback_page).post(crate::tournaments::feedback::public::submit::do_submit_feedback))

        // Rounds
        .route("/tournaments/:id/rounds", get(crate::tournaments::rounds::manage::manage_rounds_page))
        .route("/tournaments/:id/rounds/create", get(crate::tournaments::rounds::manage::create::create_new_round_of_specific_category_page).post(crate::tournaments::rounds::manage::create::do_create_new_round_of_specific_category))
        .route("/tournaments/:id/rounds/create_top", post(crate::tournaments::rounds::manage::create::create_new_round))
        .route("/tournaments/:id/rounds/:round_id", get(crate::tournaments::rounds::manage::view::view_tournament_rounds_page))
        .route("/tournaments/:id/rounds/:round_id/edit", get(crate::tournaments::rounds::manage::edit::edit_round_page).post(crate::tournaments::rounds::manage::edit::do_edit_round))
        
        // Availability
        .route("/tournaments/:id/rounds/availability", get(crate::tournaments::rounds::manage::availability::manage_round_availability))
        .route("/tournaments/:id/rounds/:round_seq/availability/judges", get(crate::tournaments::rounds::manage::availability::judges::view_judge_availability))
        .route("/tournaments/:id/rounds/:round_seq/availability/judges/ws", get(crate::tournaments::rounds::manage::availability::judges::judge_availability_updates))
        .route("/tournaments/:id/rounds/:round_id/update_judge_availability", post(crate::tournaments::rounds::manage::availability::judges::update_judge_availability)) // Uses round_id in POST URL in `judges.rs`
        
        .route("/tournaments/:id/rounds/:round_seq/availability/teams", get(crate::tournaments::rounds::manage::availability::teams::view_team_availability))
        .route("/tournaments/:id/rounds/:round_seq/availability/teams/ws", get(crate::tournaments::rounds::manage::availability::teams::team_availability_updates)) 
        .route("/tournaments/:id/rounds/:round_id/update_team_eligibility", post(crate::tournaments::rounds::manage::availability::teams::update_team_eligibility))
        .route("/tournaments/:id/rounds/:round_id/availability/teams/all", post(crate::tournaments::rounds::manage::availability::teams::update_eligibility_for_all))

        // Draw Edit
        .route("/tournaments/:id/rounds/draws/edit", get(crate::tournaments::rounds::manage::draw_edit::edit_multiple_draws_page).post(crate::tournaments::rounds::manage::draw_edit::submit_cmd))
        .route("/tournaments/:id/rounds/draws/edit/ws", get(crate::tournaments::rounds::manage::draw_edit::draw_updates))
        
        // Draw Generation
        .route("/tournaments/:id/rounds/:round_id/draw/create", get(crate::tournaments::rounds::draws::manage::create::generate_draw_page).post(crate::tournaments::rounds::draws::manage::create::do_generate_draw))
        
        // Standings
        .route("/tournaments/:id/standings/teams", get(crate::tournaments::standings::manage::admin_team_standings::admin_view_team_standings))
        .route("/tournaments/:id/tab/team", get(crate::tournaments::standings::public::public_team_tab_page))

        // Public Draw
        .route("/tournaments/:id/draw", get(crate::tournaments::rounds::draws::public::view::view_active_draw_page))
        
        // Ballots
        .route("/tournaments/:id/rounds/:seq/ballots", get(crate::tournaments::rounds::ballots::manage::overview::admin_ballot_of_seq_overview))
        .route("/tournaments/:id/privateurls/:private_url", get(crate::tournaments::privateurls::view::private_url_page))
        .route("/tournaments/:id/privateurls/:url/rounds/:round_id/submit", get(crate::tournaments::rounds::ballots::public::submit::submit_ballot_page).post(crate::tournaments::rounds::ballots::public::submit::do_submit_ballot))
        
        .layer(axum::Extension(tx))
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(axum::middleware::from_fn(
                    crate::state::transaction_middleware,
                )),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
