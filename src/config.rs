use axum::{
    Router,
    extract::MatchedPath,
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
            div class="row" {
                div class="col-md-8" {
                    h1 class="h1 fw-bold mb-4" { "My Tournaments" }

                    @if tournaments_list.is_empty() {
                        p class="lead" { "You are not a member of any tournaments yet." }
                    } @else {
                        div class="list-group list-group-flush border-top" {
                            @for tournament in &tournaments_list {
                                div class="list-group-item py-4" {
                                    h3 class="h3 mb-2" {
                                        a href=(format!("/tournaments/{}", tournament.id)) class="text-decoration-none text-dark hover-underline fw-bold" {
                                            (tournament.name)
                                        }
                                    }
                                    p class="text-secondary mb-1" {
                                        "Abbreviation: " (tournament.abbrv)
                                    }
                                    small class="text-secondary" {
                                        "Created " (tournament.created_at.format("%d %B %Y").to_string())
                                    }
                                }
                            }
                        }
                    }
                }
                div class="col-md-4" {
                    div class="bg-light p-4 border-top border-4 border-primary" {
                        h2 class="h4 fw-bold mb-3" { "Actions" }
                        p class="mb-3" { "Start a new tournament to begin tabulating." }
                        a href="/tournaments/create" class="btn btn-primary d-block w-100 fw-bold" {
                            "Create new tournament"
                        }
                    }
                }
            }
        };

        success(
            Page::new()
                .user(user)
                .body(maud! {
                    div class="container py-4" {
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
                    div class="container py-5" {
                        div class="text-center py-5" {
                            h1 class="display-4 fw-bold mb-4" { "Abacus" }
                            p class="lead text-secondary mb-5" { "Tabulation software for debating tournaments." }
                            div class="d-flex justify-content-center gap-3" {
                                a href="/login" class="btn btn-primary btn-lg px-4" { "Login" }
                                a href="/register" class="btn btn-outline-secondary btn-lg px-4" { "Register" }
                            }
                        }
                    }
                })
                .render(),
        )
    }
}

pub async fn run() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| ":memory:".to_string());

    let pool: DbPool = Pool::builder()
        .max_size(if db_url == ":memory:" { 1 } else { 10 })
        .build(ConnectionManager::<SqliteConnection>::new(db_url))
        .unwrap();

    {
        let conn = pool.get().unwrap();
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
        .route("/tournaments/:id/participants", get(crate::tournaments::participants::manage::manage_tournament_participants))
        .route("/tournaments/:id/participants/ws", get(crate::tournaments::participants::manage::tournament_participant_updates))
        .route("/tournaments/:id/participants/privateurls", get(crate::tournaments::participants::manage::manage_private_urls::view_private_urls))

        // Teams
        .route("/tournaments/:id/teams/create", get(crate::tournaments::participants::manage::create_team::create_teams_page).post(crate::tournaments::participants::manage::create_team::do_create_team))
        .route("/tournaments/:id/teams/:team_id", get(crate::tournaments::participants::manage::manage_team::manage_team_page))
        .route("/tournaments/:id/teams/:team_id/edit", get(crate::tournaments::participants::manage::manage_team::edit_team_details_page).post(crate::tournaments::participants::manage::manage_team::do_edit_team_details))
        .route("/tournaments/:id/teams/:team_id/speakers/create", get(crate::tournaments::participants::manage::create_speaker::create_speaker_page).post(crate::tournaments::participants::manage::create_speaker::do_create_speaker))

        // Judges
        .route("/tournaments/:id/judges/create", get(crate::tournaments::participants::manage::create_judge::create_judge_page).post(crate::tournaments::participants::manage::create_judge::do_create_judge))
        .route("/tournaments/:id/judges/:judge_id/edit", get(crate::tournaments::participants::manage::manage_judge::edit_judge_details_page).post(crate::tournaments::participants::manage::manage_judge::do_edit_judge_details))

        // Legacy routes (for backwards compatibility)
        .route("/tournaments/:id/participants/team", get(crate::tournaments::participants::manage::manage_team::manage_team_page))
        .route("/tournaments/:id/participants/team/create", get(crate::tournaments::participants::manage::create_team::create_teams_page).post(crate::tournaments::participants::manage::create_team::do_create_team))
        .route("/tournaments/:id/participants/team/edit", get(crate::tournaments::participants::manage::manage_team::edit_team_details_page).post(crate::tournaments::participants::manage::manage_team::do_edit_team_details))
        .route("/tournaments/:id/participants/judge/create", get(crate::tournaments::participants::manage::create_judge::create_judge_page).post(crate::tournaments::participants::manage::create_judge::do_create_judge))
        .route("/tournaments/:id/participants/judge/edit", get(crate::tournaments::participants::manage::manage_judge::edit_judge_details_page).post(crate::tournaments::participants::manage::manage_judge::do_edit_judge_details))
        .route("/tournaments/:id/participants/speaker/create", get(crate::tournaments::participants::manage::create_speaker::create_speaker_page).post(crate::tournaments::participants::manage::create_speaker::do_create_speaker))
        .route("/tournaments/:id/participants/updates", get(crate::tournaments::participants::manage::tournament_participant_updates))

        // Configuration
        .route("/tournaments/:id/configuration", get(crate::tournaments::manage::config::view_tournament_configuration).post(crate::tournaments::manage::config::update_tournament_configuration))
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
        .route("/tournaments/:id/rounds/:round_seq/setup", get(crate::tournaments::rounds::manage::setup::setup_round_page))
        .route("/tournaments/:id/rounds/create", get(crate::tournaments::rounds::manage::create::create_new_round_of_specific_category_page).post(crate::tournaments::rounds::manage::create::do_create_new_round_of_specific_category))
        .route("/tournaments/:id/rounds/create_top", post(crate::tournaments::rounds::manage::create::create_new_round))
        .route("/tournaments/:id/rounds/:round_seq", get(crate::tournaments::rounds::manage::view::view_tournament_rounds_page))
        .route("/tournaments/:id/rounds/:round_seq/draw", get(crate::tournaments::rounds::draws::public::view::view_active_draw_page))
        .route("/tournaments/:id/rounds/:round_seq/draw/manage", get(crate::tournaments::rounds::manage::draw_view::view_draws_page))
        .route("/tournaments/:id/rounds/:round_id/edit", get(crate::tournaments::rounds::manage::edit::edit_round_page).post(crate::tournaments::rounds::manage::edit::do_edit_round))
        .route("/tournaments/:id/rounds/:round_seq/briefing", get(crate::tournaments::rounds::manage::briefing::get_briefing_room))
        .route("/tournaments/:id/rounds/:id/draws/setreleased", post(crate::tournaments::rounds::manage::briefing::set_draw_published))
        .route("/tournaments/:id/rounds/:round_seq/results", get(crate::tournaments::rounds::results::view_results_page))
        .route("/tournaments/:id/rounds/:round_seq/results/manage", get(crate::tournaments::rounds::manage::results::manage_results_page))
        .route("/tournaments/:id/rounds/:round_id/complete", post(crate::tournaments::rounds::manage::results::set_round_completed))
        .route("/tournaments/:id/rounds/:round_id/results/publish", post(crate::tournaments::rounds::manage::results::set_results_published))

        // Availability
        .route("/tournaments/:id/rounds/:round_seq/availability/judges", get(crate::tournaments::rounds::manage::availability::judges::view_judge_availability))
        .route("/tournaments/:id/rounds/:round_seq/availability/judges/ws", get(crate::tournaments::rounds::manage::availability::judges::judge_availability_updates))
        .route("/tournaments/:id/rounds/:round_id/update_judge_availability", post(crate::tournaments::rounds::manage::availability::judges::update_judge_availability))
        .route("/tournaments/:id/rounds/:round_id/availability/judges/all", post(crate::tournaments::rounds::manage::availability::judges::update_judge_availability_for_all))
        .route("/tournaments/:id/rounds/:round_seq/availability/teams", get(crate::tournaments::rounds::manage::availability::teams::view_team_availability))
        .route("/tournaments/:id/rounds/:round_seq/availability/teams/ws", get(crate::tournaments::rounds::manage::availability::teams::team_availability_updates))        .route("/tournaments/:id/rounds/:round_id/update_team_eligibility", post(crate::tournaments::rounds::manage::availability::teams::update_team_eligibility))
        .route("/tournaments/:id/rounds/:round_id/availability/teams/all", post(crate::tournaments::rounds::manage::availability::teams::update_eligibility_for_all))

        // Draw Edit
        .route("/tournaments/:id/rounds/draws/edit", get(crate::tournaments::rounds::manage::draw_edit::edit_multiple_draws_page).post(crate::tournaments::rounds::manage::draw_edit::submit_cmd))
        .route("/tournaments/:id/rounds/draws/edit/ws", get(crate::tournaments::rounds::manage::draw_edit::draw_updates))
        .route("/tournaments/:id/rounds/draws/edit/move", post(crate::tournaments::rounds::manage::draw_edit::move_judge))
        .route("/tournaments/:id/rounds/draws/edit/role", post(crate::tournaments::rounds::manage::draw_edit::change_judge_role))

        // Draw Generation
        .route("/tournaments/:id/rounds/:round_id/draws/create", get(crate::tournaments::rounds::draws::manage::create::generate_draw_page).post(crate::tournaments::rounds::draws::manage::create::do_generate_draw))

        // Standings
        .route("/tournaments/:id/standings/teams", get(crate::tournaments::standings::manage::admin_team_standings::admin_view_team_standings))
        .route("/tournaments/:id/tab/team", get(crate::tournaments::standings::public::public_team_tab_page))

        // Public Draw


        // Public Participants


        // Public Motions
        .route("/tournaments/:id/motions", get(crate::tournaments::motions::public_motions_page))

        // Ballots
        .route("/tournaments/:id/rounds/:round_seq/ballots", get(crate::tournaments::rounds::ballots::manage::overview::admin_ballot_of_seq_overview))
        .route("/tournaments/:id/debates/:debate_id/ballots", get(crate::tournaments::rounds::ballots::manage::view_ballot_set::view_ballot_set_page))
        .route("/tournaments/:id/privateurls/:private_url", get(crate::tournaments::privateurls::view::private_url_page))
        .route("/tournaments/:id/privateurls/:url/rounds/:round_id/submit", get(crate::tournaments::rounds::ballots::public::submit::submit_ballot_page).post(crate::tournaments::rounds::ballots::public::submit::do_submit_ballot))

        .layer(axum::Extension(tx))
        .layer(axum::Extension(state.pool.clone()))
        .with_state(state)
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http()
                    .make_span_with(|request: &axum::extract::Request<_>| {
                        let matched_path = request
                            .extensions()
                            .get::<MatchedPath>()
                            .map(MatchedPath::as_str);

                        tracing::info_span!(
                            "http_request",
                            method = ?request.method(),
                            matched_path,
                            some_other_field = tracing::field::Empty,
                        )
                    }))
                .layer(axum::middleware::from_fn(
                    crate::state::transaction_middleware,
                )),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8000")
        .await
        .unwrap();
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
