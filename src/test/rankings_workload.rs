//! Crash workload. Inputs roughly correctly formed data into the system and
//! attempts to make it crash. This is connected to a fuzzing harness (which
//! can be found in the `fuzz/` directory).

use std::collections::HashMap;

use arbitrary::{Arbitrary, Unstructured};

use axum::{Router, extract::Request, http::header::COOKIE};
use diesel::{
    SqliteConnection,
    connection::LoadConnection,
    prelude::*,
    r2d2::{ConnectionManager, Pool},
    sqlite::Sqlite,
};
use diesel_migrations::MigrationHarness;
use rand::{SeedableRng, seq::SliceRandom};
use rust_decimal::prelude::FromPrimitive;
use serde::Deserialize;
use tower::{Service, ServiceExt};

use crate::{
    MIGRATIONS,
    config::create_app,
    schema::{
        tournament_debate_teams, tournament_debates, tournament_judges,
        tournament_round_motions, tournament_rounds, tournament_speakers,
        tournament_teams, tournaments,
    },
    state::DbPool,
    tournaments::{
        config::{PullupMetric, RankableTeamMetric, SpeakerMetric},
        manage::config::TournamentConfig,
    },
};

// This is a macro rather than a function because the `assert!` panic
// then directly notes the span of the call site (rather than requiring
// a look at the stack trace to find it).
macro_rules! assert_res_ok {
    ($response:expr) => {
        assert!(
            $response.status().is_success()
                || $response.status().is_redirection(),
            "response status = {:?}, str = {}",
            $response.status(),
            {
                let body_bytes =
                    axum::body::to_bytes($response.into_body(), usize::MAX)
                        .await
                        .unwrap();
                let body_str = String::from_utf8_lossy(&body_bytes).to_string();
                body_str
            }
        );
    };
}

const ADMIN_USERNAME: &str = "admin";
const ADMIN_PASSWORD: &str = "password";
const TOURNAMENT_NAME: &str = "Test Tournament";

#[derive(Debug, Deserialize)]
pub struct Email(pub String);

impl<'a> Arbitrary<'a> for Email {
    fn arbitrary(
        u: &mut arbitrary::Unstructured<'a>,
    ) -> arbitrary::Result<Self> {
        let name = u.arbitrary::<usize>()?;
        let domain = u.arbitrary::<usize>()?;

        let tld = match u.arbitrary::<usize>()? % 3 {
            0 => "com",
            1 => "net",
            2 => "org",
            _ => unreachable!(),
        };

        Ok(Self(format!("{name}@{domain}.{tld}")))
    }
}

#[derive(Debug, Deserialize)]
pub struct InitData {
    teams: Vec<Team>,
    judges: Vec<Judge>,
    in_rounds: usize,
    conf: TournamentConfig,
}

impl InitData {
    pub fn make_arbitrary<'a>(
        u: &mut Unstructured<'a>,
        conf: &TournamentConfig,
    ) -> arbitrary::Result<Self> {
        let teams_per_room = conf.teams_per_side as usize * 2;
        let num_teams = u.int_in_range(2..=10)? * teams_per_room;

        let teams = (0..num_teams)
            .map(|i| {
                let team_name = format!("Team {}", i);
                let speakers = (0..conf.substantive_speakers)
                    .map(|j| {
                        Ok(Speaker {
                            name: format!("{} Speaker {}", &team_name, j),
                            email: u.arbitrary()?,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Team {
                    name: team_name,
                    speakers,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let num_judges = u
            .int_in_range(num_teams / 2..=num_teams * 2)
            .unwrap()
            .max(1);
        let judges = (0..num_judges)
            .map(|i| {
                Ok(Judge {
                    name: format!("Judge {}", i),
                    email: u.arbitrary()?,
                })
            })
            .collect::<Result<_, _>>()?;

        let in_rounds = u.int_in_range(1..=5)?;

        Ok(InitData {
            teams,
            judges,
            in_rounds,
            conf: conf.clone(),
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct Judge {
    name: String,
    email: Email,
}

#[derive(Debug, Deserialize)]
pub struct Team {
    speakers: Vec<Speaker>,
    name: String,
}

#[derive(Debug, Deserialize)]
pub struct Speaker {
    name: String,
    email: Email,
}

#[derive(Debug, Deserialize)]
pub struct RoundData {
    pub draw: Vec<RoomOfDraw>,
}

impl RoundData {
    pub fn make_arbitrary<'a>(
        u: &mut Unstructured<'a>,
        conf: &TournamentConfig,
        teams: &[Team],
        judges: &[Judge],
    ) -> Self {
        let teams_per_room = conf.teams_per_side as usize * 2;
        if teams_per_room == 0 {
            return Self { draw: vec![] };
        }
        let num_rooms = teams.len() / teams_per_room;

        let mut team_names: Vec<String> =
            teams.iter().map(|t| t.name.clone()).collect();
        let mut rng: rand::rngs::StdRng =
            rand::rngs::StdRng::seed_from_u64(u.arbitrary::<u64>().unwrap());
        team_names.shuffle(&mut rng);

        let mut available_judge_names: Vec<String> =
            judges.iter().map(|j| j.name.clone()).collect();
        available_judge_names.shuffle(&mut rng);

        if judges.is_empty() {
            panic!("Cannot make arbitrary RoundData with no judges.");
        }

        let team_chunks = team_names.chunks_exact(teams_per_room);
        let mut rooms = Vec::with_capacity(num_rooms);

        let teams_by_name: HashMap<_, _> =
            teams.iter().map(|t| (t.name.clone(), t)).collect();

        for team_chunk in team_chunks {
            let room_teams = team_chunk.to_vec();

            let chair_name = if let Some(name) = available_judge_names.pop() {
                name
            } else {
                let judge_names: Vec<String> =
                    judges.iter().map(|j| j.name.clone()).collect();
                u.choose(&judge_names).unwrap().clone()
            };

            let num_panelists = u
                .int_in_range(0..=std::cmp::min(2, available_judge_names.len()))
                .unwrap();
            let panelists = (0..num_panelists)
                .filter_map(|_| available_judge_names.pop())
                .collect::<Vec<_>>();

            let num_trainees = u
                .int_in_range(0..=std::cmp::min(1, available_judge_names.len()))
                .unwrap();
            let trainees = (0..num_trainees)
                .filter_map(|_| available_judge_names.pop())
                .collect::<Vec<_>>();

            let mut room_judges = vec![chair_name.clone()];
            room_judges.extend(panelists.clone());

            let mut ballots: HashMap<String, BallotForSubmission> =
                HashMap::new();

            let mut ballot_entries_cache = None;

            let create_ballot_entries = |u: &mut Unstructured| {
                let mut new_entries = vec![];
                for team_name in &room_teams {
                    let team = teams_by_name.get(team_name).unwrap();
                    let mut speaker_scores = vec![];
                    for speaker in &team.speakers {
                        let min = conf.substantive_speech_min_speak;
                        let max = conf.substantive_speech_max_speak;
                        let step = conf.substantive_speech_step;

                        assert_ne!(min, max);

                        let range = ((max - min) / step).floor() as u64;
                        let score_step =
                            u.int_in_range(1..=(range - 1)).unwrap();
                        let score = min + (score_step as f32 * step);

                        speaker_scores.push((
                            speaker.name.clone(),
                            rust_decimal::Decimal::from_f32(score)
                                .unwrap_or_default(),
                        ));
                    }
                    new_entries.push(speaker_scores);
                }
                new_entries
            };

            for judge_name in room_judges.iter() {
                let entries = if conf.pool_ballot_setup == "consensus" {
                    if ballot_entries_cache.is_none() {
                        ballot_entries_cache = Some(create_ballot_entries(u));
                    }
                    ballot_entries_cache.as_ref().unwrap().clone()
                } else {
                    create_ballot_entries(u)
                };

                ballots.insert(
                    judge_name.clone(),
                    BallotForSubmission { entries },
                );
            }

            rooms.push(RoomOfDraw {
                teams: room_teams,
                c: chair_name,
                p: panelists,
                t: trainees,
                ballots,
            });
        }

        Self { draw: rooms }
    }
}

#[derive(Debug, Deserialize)]
pub struct RoomOfDraw {
    /// List of team names.
    pub teams: Vec<String>,
    /// Chair judge (by name)
    pub c: String,
    /// Panelist judges (by name)
    pub p: Vec<String>,
    /// Trainee judges (by name)
    pub t: Vec<String>,
    /// Ballots keyed by judge name
    pub ballots: HashMap<String, BallotForSubmission>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BallotForSubmission {
    /// This is essentially a map of
    /// [[(speaker_name, score)]]
    pub entries: Vec<Vec<(String, rust_decimal::Decimal)>>,
}

#[derive(Debug, Deserialize)]
pub struct Workload {
    init: InitData,
    in_rounds: Vec<RoundData>,
    // no outrounds for this workload
}

impl Workload {
    pub fn make_arbitrary<'a>(
        u: &mut Unstructured<'a>,
        conf: &TournamentConfig,
    ) -> arbitrary::Result<Self> {
        let len = u.int_in_range(1..=5)?;
        let mut init = InitData::make_arbitrary(u, conf)?;
        init.in_rounds = len;

        Ok(Self {
            in_rounds: {
                (0..len)
                    .into_iter()
                    .map(|_| {
                        RoundData::make_arbitrary(
                            u,
                            conf,
                            &init.teams,
                            &init.judges,
                        )
                    })
                    .collect()
            },
            init,
        })
    }
}

impl<'a> Arbitrary<'a> for Workload {
    fn arbitrary(
        u: &mut arbitrary::Unstructured<'a>,
    ) -> arbitrary::Result<Self> {
        let format_bp_or_wsdc = u.arbitrary::<bool>().unwrap();

        let config = match format_bp_or_wsdc {
            true => {
                // bp
                TournamentConfig {
                    team_tab_public: true,
                    speaker_tab_public: true,
                    standings_public: true,
                    show_round_results: true,
                    show_draws: true,
                    teams_per_side: 2,
                    substantive_speakers: 2,
                    reply_speakers: false,
                    reply_must_speak: false,
                    max_substantive_speech_index_for_reply: None,
                    pool_ballot_setup: "consensus".to_string(),
                    elim_ballot_setup: "consensus".to_string(),
                    require_elim_ballot_substantive_speaks: false,
                    institution_penalty: 0,
                    history_penalty: 0,
                    pullup_metrics: serde_json::to_string(&[
                        PullupMetric::Random,
                    ])
                    .unwrap(),
                    repeat_pullup_penalty: 0,
                    team_standings_metrics: serde_json::to_string(&[
                        RankableTeamMetric::Wins,
                        RankableTeamMetric::NTimesAchieved(3),
                        RankableTeamMetric::NTimesAchieved(2),
                        RankableTeamMetric::NTimesAchieved(1),
                        RankableTeamMetric::DrawStrengthByWins,
                    ])
                    .unwrap(),
                    speaker_standings_metrics: serde_json::to_string(&[
                        SpeakerMetric::Avg,
                        SpeakerMetric::StdDev,
                    ])
                    .unwrap(),
                    exclude_from_speaker_standings_after: None,
                    substantive_speech_min_speak: 50.0f32,
                    // todo: validate that max_speak = k * step + min_speak (?)
                    substantive_speech_max_speak: 99.0f32,
                    substantive_speech_step: 1.0f32,
                    reply_speech_min_speak: None,
                    reply_speech_max_speak: None,
                }
            }
            false => {
                // wsdc

                // todo: check, because this isn't correct
                TournamentConfig {
                    team_tab_public: true,
                    speaker_tab_public: true,
                    standings_public: true,
                    show_round_results: true,
                    show_draws: true,
                    teams_per_side: 1,
                    substantive_speakers: 3,
                    reply_speakers: true,
                    reply_must_speak: false,
                    max_substantive_speech_index_for_reply: Some(2),
                    pool_ballot_setup: "consensus".to_string(),
                    elim_ballot_setup: "consensus".to_string(),
                    require_elim_ballot_substantive_speaks: false,
                    institution_penalty: 0,
                    history_penalty: 0,
                    pullup_metrics: serde_json::to_string(&[
                        PullupMetric::Random,
                    ])
                    .unwrap(),
                    repeat_pullup_penalty: 0,
                    team_standings_metrics: serde_json::to_string(&[
                        RankableTeamMetric::Wins,
                        RankableTeamMetric::DrawStrengthByWins,
                    ])
                    .unwrap(),
                    speaker_standings_metrics: serde_json::to_string(&[
                        SpeakerMetric::Avg,
                        SpeakerMetric::StdDev,
                    ])
                    .unwrap(),
                    exclude_from_speaker_standings_after: None,
                    substantive_speech_min_speak: 60.0f32,
                    // todo: validate that max_speak = k * step + min_speak (?)
                    substantive_speech_max_speak: 80.0f32,
                    substantive_speech_step: 0.5f32,
                    reply_speech_min_speak: Some(30.0f32),
                    reply_speech_max_speak: Some(40.0f32),
                }
            }
        };

        Self::make_arbitrary(u, &config)
    }
}

impl Workload {
    pub async fn run(&self) -> (Router, DbPool) {
        let span = tracing::span!(
            tracing::Level::INFO,
            "workload_run",
            num_rounds = self.in_rounds.len()
        );
        let _guard = span.enter();

        let span = tracing::span!(tracing::Level::INFO, "pool_setup");
        let pool: DbPool = {
            let _guard = span.enter();
            Pool::builder()
                .max_size(1)
                .build(ConnectionManager::<SqliteConnection>::new(":memory:"))
                .unwrap()
        };

        {
            let span = tracing::span!(tracing::Level::INFO, "migrations");
            let _guard = span.enter();
            let mut conn = pool.get().unwrap();
            conn.run_pending_migrations(MIGRATIONS).unwrap();
        }

        assert_eq!(pool.state().idle_connections, 1);

        let app = create_app(pool.clone());

        let span = tracing::span!(tracing::Level::INFO, "tournament_setup");
        let _guard = span.enter();
        let session_cookie = self.setup(app.clone(), pool.clone()).await;
        drop(_guard);

        // Simulate each in-round
        for seq in 1..=self.in_rounds.len() {
            self.simulate_round(
                seq,
                app.clone(),
                pool.clone(),
                &session_cookie,
            )
            .await;
        }

        (app, pool)
    }

    pub async fn setup(&self, app: Router, pool: DbPool) -> String {
        let span = tracing::span!(tracing::Level::INFO, "setup");
        let _guard = span.enter();

        let mut app = app.into_service();
        let app = app.ready().await.unwrap();

        self.create_admin_user(&mut app.clone(), &pool).await;
        let session_cookie = self.login_as_admin(&mut app.clone()).await;
        let tournament_id = self
            .create_tournament(&mut app.clone(), &pool, &session_cookie)
            .await;
        self.configure_tournament(
            &mut app.clone(),
            &pool,
            &tournament_id,
            &session_cookie,
        )
        .await;
        self.create_judges(
            &mut app.clone(),
            &pool,
            &tournament_id,
            &session_cookie,
        )
        .await;
        self.create_teams(
            &mut app.clone(),
            &pool,
            &tournament_id,
            &session_cookie,
        )
        .await;
        self.create_rounds(
            &mut app.clone(),
            &pool,
            &tournament_id,
            &session_cookie,
        )
        .await;

        session_cookie
    }

    async fn create_admin_user(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        pool: &DbPool,
    ) {
        let span = tracing::span!(tracing::Level::DEBUG, "register_admin");
        let _guard = span.enter();

        let register_request = Request::builder()
            .method("POST")
            .uri("/register")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(axum::body::Body::from(
                serde_urlencoded::to_string(&[
                    ("username", ADMIN_USERNAME),
                    ("email", "admin@test.com"),
                    ("password", ADMIN_PASSWORD),
                    ("password2", ADMIN_PASSWORD),
                ])
                .unwrap(),
            ))
            .unwrap();

        assert_eq!(pool.state().idle_connections, 1);

        let response = app.call(register_request).await.unwrap();
        assert!(
            response.status().is_success()
                || response.status().is_redirection()
        );
    }

    async fn login_as_admin(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
    ) -> String {
        let span = tracing::span!(tracing::Level::DEBUG, "login_admin");
        let _guard = span.enter();

        let login_request = Request::builder()
            .method("POST")
            .uri("/login")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(axum::body::Body::from(
                serde_urlencoded::to_string(&[
                    ("id", ADMIN_USERNAME),
                    ("password", ADMIN_PASSWORD),
                ])
                .unwrap(),
            ))
            .unwrap();

        let response = app
            .ready()
            .await
            .unwrap()
            .call(login_request)
            .await
            .unwrap();

        response
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    async fn create_tournament(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        pool: &DbPool,
        session_cookie: &str,
    ) -> String {
        let span = tracing::span!(tracing::Level::DEBUG, "create_tournament");
        let _guard = span.enter();

        let create_tournament_request = Request::builder()
            .method("POST")
            .uri("/tournaments/create")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(COOKIE, session_cookie)
            .body(axum::body::Body::from(
                serde_urlencoded::to_string(&[
                    ("name", TOURNAMENT_NAME),
                    ("abbrv", "TT"),
                    ("slug", "test_tournament"),
                ])
                .unwrap(),
            ))
            .unwrap();

        assert_eq!(pool.state().idle_connections, 1);

        let response = app
            .ready()
            .await
            .unwrap()
            .call(create_tournament_request)
            .await
            .unwrap();
        assert_res_ok!(response);

        // Drop the connection before querying
        drop(response);

        let tournament_id: String = {
            let mut conn = pool.get().unwrap();
            tournaments::table
                .filter(tournaments::name.eq(TOURNAMENT_NAME))
                .select(tournaments::id)
                .first(&mut conn)
                .unwrap()
        };

        tournament_id
    }

    async fn configure_tournament(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        _pool: &DbPool,
        tournament_id: &str,
        session_cookie: &str,
    ) {
        let span =
            tracing::span!(tracing::Level::DEBUG, "configure_tournament");
        let _guard = span.enter();

        let update_config_request = Request::builder()
            .method("POST")
            .uri(format!("/tournaments/{}/configuration", tournament_id))
            .header("content-type", "application/x-www-form-urlencoded")
            .header(COOKIE, session_cookie)
            .body(axum::body::Body::from(
                serde_urlencoded::to_string(&[(
                    "config",
                    toml::to_string(&self.init.conf).unwrap(),
                )])
                .unwrap(),
            ))
            .unwrap();

        let response = app
            .ready()
            .await
            .unwrap()
            .call(update_config_request)
            .await
            .unwrap();
        assert_res_ok!(response);
    }

    async fn create_judges(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        pool: &DbPool,
        tournament_id: &str,
        session_cookie: &str,
    ) {
        let span = tracing::span!(
            tracing::Level::DEBUG,
            "create_judges",
            num_judges = self.init.judges.len()
        );
        let _guard = span.enter();

        for workload_judge in &self.init.judges {
            assert_eq!(pool.state().idle_connections, 1);

            let create_judge_request = Request::builder()
                .method("POST")
                .uri(format!(
                    "/tournaments/{}/participants/judge/create",
                    tournament_id
                ))
                .header("content-type", "application/x-www-form-urlencoded")
                .header(COOKIE, session_cookie)
                .body(axum::body::Body::from(
                    serde_urlencoded::to_string(&[
                        ("name", &workload_judge.name),
                        ("email", &workload_judge.email.0),
                        ("institution_id", &"-----".to_string()),
                    ])
                    .unwrap(),
                ))
                .unwrap();

            let response = app
                .ready()
                .await
                .unwrap()
                .call(create_judge_request)
                .await
                .unwrap();
            assert_res_ok!(response);
        }
    }

    async fn create_teams(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        pool: &DbPool,
        tournament_id: &str,
        session_cookie: &str,
    ) {
        let span = tracing::span!(
            tracing::Level::DEBUG,
            "create_teams",
            num_teams = self.init.teams.len()
        );
        let _guard = span.enter();

        for workload_team in &self.init.teams {
            let create_team_request = Request::builder()
                .method("POST")
                .uri(format!(
                    "/tournaments/{}/participants/team/create",
                    tournament_id
                ))
                .header("content-type", "application/x-www-form-urlencoded")
                .header(COOKIE, session_cookie)
                .body(axum::body::Body::from(
                    serde_urlencoded::to_string(&[
                        ("name", &workload_team.name),
                        ("institution_id", &"-----".to_string()),
                    ])
                    .unwrap(),
                ))
                .unwrap();

            let response = app
                .ready()
                .await
                .unwrap()
                .call(create_team_request)
                .await
                .unwrap();
            assert_res_ok!(response);

            let app_team_id: String = {
                let mut conn = pool.get().unwrap();
                tournament_teams::table
                    .filter(tournament_teams::tournament_id.eq(tournament_id))
                    .filter(tournament_teams::name.eq(&workload_team.name))
                    .select(tournament_teams::id)
                    .first(&mut conn)
                    .unwrap()
            };

            for workload_speaker in &workload_team.speakers {
                let create_speaker_request = Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/tournaments/{}/teams/{}/speakers/create",
                        tournament_id, app_team_id
                    ))
                    .header("content-type", "application/x-www-form-urlencoded")
                    .header(COOKIE, session_cookie)
                    .body(axum::body::Body::from(
                        serde_urlencoded::to_string(&[
                            ("name", &workload_speaker.name),
                            ("email", &workload_speaker.email.0),
                        ])
                        .unwrap(),
                    ))
                    .unwrap();

                let response = app
                    .ready()
                    .await
                    .unwrap()
                    .call(create_speaker_request)
                    .await
                    .unwrap();
                assert_res_ok!(response);
            }
        }
    }

    async fn create_rounds(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        pool: &DbPool,
        tournament_id: &str,
        session_cookie: &str,
    ) {
        let span = tracing::span!(
            tracing::Level::DEBUG,
            "create_rounds",
            num_rounds = self.init.in_rounds
        );
        let _guard = span.enter();

        for workload_round_num in 1..=self.init.in_rounds {
            let create_round_request = Request::builder()
                .method("POST")
                .uri(format!(
                    "/tournaments/{}/rounds/in_round/create",
                    tournament_id
                ))
                .header("content-type", "application/x-www-form-urlencoded")
                .header(COOKIE, session_cookie)
                .body(axum::body::Body::from(
                    serde_urlencoded::to_string(&[
                        ("name", format!("Round {}", workload_round_num)),
                        ("seq", workload_round_num.to_string()),
                    ])
                    .unwrap(),
                ))
                .unwrap();

            let response = app
                .ready()
                .await
                .unwrap()
                .call(create_round_request)
                .await
                .unwrap();
            assert_res_ok!(response);

            let app_round_id: String = {
                let mut conn = pool.get().unwrap();
                tournament_rounds::table
                    .filter(tournament_rounds::tournament_id.eq(tournament_id))
                    .filter(
                        tournament_rounds::seq.eq(workload_round_num as i64),
                    )
                    .select(tournament_rounds::id)
                    .first(&mut conn)
                    .unwrap()
            };

            let mut conn = pool.get().unwrap();
            diesel::insert_into(tournament_round_motions::table)
                .values((
                    tournament_round_motions::id
                        .eq(uuid::Uuid::new_v4().to_string()),
                    tournament_round_motions::tournament_id.eq(tournament_id),
                    tournament_round_motions::round_id.eq(&app_round_id),
                    tournament_round_motions::motion
                        .eq("This House would do something."),
                ))
                .execute(&mut conn)
                .unwrap();
        }
    }

    /// Helper: Check in all judges for a round
    async fn check_in_judges(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        tournament_id: &str,
        round_id: &str,
        session_cookie: &str,
    ) {
        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/tournaments/{}/rounds/{}/availability/judges/all?check=in",
                tournament_id, round_id
            ))
            .header(COOKIE, session_cookie)
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_res_ok!(response);
    }

    /// Helper: Check in all teams for a round
    async fn check_in_teams(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        tournament_id: &str,
        round_id: &str,
        session_cookie: &str,
    ) {
        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/tournaments/{}/rounds/{}/availability/teams/all?check=in",
                tournament_id, round_id
            ))
            .header(COOKIE, session_cookie)
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_res_ok!(response);
    }

    /// Helper: Create the draw for a round
    async fn create_draw(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        tournament_id: &str,
        round_id: &str,
        session_cookie: &str,
    ) {
        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/tournaments/{}/rounds/{}/draws/create",
                tournament_id, round_id
            ))
            .header("content-type", "application/x-www-form-urlencoded")
            .header(COOKIE, session_cookie)
            .body(axum::body::Body::empty())
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_res_ok!(response);
    }

    fn check_draw_has_same_number_of_teams_as_workload(
        &self,
        round_id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) {
        let team_count = self.init.teams.len();

        let app_debate_count = tournament_debates::table
            .filter(tournament_debates::round_id.eq(round_id))
            .count()
            .get_result::<i64>(conn)
            .unwrap();
        assert_eq!(
            app_debate_count as usize,
            (team_count as usize)
                / ((self.init.conf.teams_per_side * 2) as usize)
        );

        let app_team_count = tournament_debate_teams::table
            .inner_join(
                tournament_debates::table.on(tournament_debates::id
                    .eq(tournament_debate_teams::debate_id)
                    .and(tournament_debates::round_id.eq(round_id))),
            )
            .count()
            .get_result::<i64>(conn)
            .unwrap();
        assert_eq!(app_team_count as usize, team_count);
    }

    /// Helper: Move a team to a specific position
    async fn move_team(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        tournament_id: &str,
        round_id: &str,
        team1_id: &str,
        team2_id: &str,
        session_cookie: &str,
    ) {
        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/tournaments/{}/rounds/draws/edit/move_team",
                tournament_id
            ))
            .header("content-type", "application/x-www-form-urlencoded")
            .header(COOKIE, session_cookie)
            .body(axum::body::Body::from(
                serde_urlencoded::to_string(&[
                    ("team1_id", team1_id),
                    ("team2_id", team2_id),
                    ("rounds", round_id),
                ])
                .unwrap(),
            ))
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert!(
            response.status().is_success()
                || response.status().is_redirection(),
            "Failed to move team: {:?}",
            response.status()
        );
    }

    /// Helper: Move a judge to a debate with a specific role
    async fn move_judge(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        tournament_id: &str,
        round_id: &str,
        judge_id: &str,
        role: &str,
        debate_id: &str,
        session_cookie: &str,
    ) {
        let form_data: Vec<(&str, String)> = vec![
            ("judge_id", judge_id.to_string()),
            ("role", role.to_string()),
            ("rounds", round_id.to_string()),
            ("to_debate_id", debate_id.to_string()),
        ];

        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/tournaments/{}/rounds/draws/edit/move",
                tournament_id
            ))
            .header("content-type", "application/x-www-form-urlencoded")
            .header(COOKIE, session_cookie)
            .body(axum::body::Body::from(
                serde_urlencoded::to_string(&form_data).unwrap(),
            ))
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_res_ok!(response);
    }

    /// Helper: Release the draw for a round
    async fn release_draw(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        tournament_id: &str,
        round_id: &str,
        session_cookie: &str,
    ) {
        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/tournaments/{}/rounds/{}/draws/setreleased",
                tournament_id, round_id
            ))
            .header("content-type", "application/x-www-form-urlencoded")
            .header(COOKIE, session_cookie)
            .body(axum::body::Body::from(
                serde_urlencoded::to_string(&[("status", "released_full")])
                    .unwrap(),
            ))
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_res_ok!(response);
    }

    /// Helper: Submit a ballot for a judge
    async fn submit_ballot(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        tournament_id: &str,
        round_id: &str,
        private_url: &str,
        speakers: &[String],
        scores: &[String],
    ) {
        let mut form_data: Vec<(&str, String)> = Vec::new();
        for speaker in speakers {
            form_data.push(("speakers", speaker.clone()));
        }
        for score in scores {
            form_data.push(("scores", score.clone()));
        }

        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/tournaments/{}/privateurls/{}/rounds/{}/submit",
                tournament_id, private_url, round_id
            ))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(axum::body::Body::from(
                serde_urlencoded::to_string(&form_data).unwrap(),
            ))
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert_res_ok!(response);
    }

    /// Helper: Mark a round as complete
    async fn complete_round(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        tournament_id: &str,
        round_id: &str,
        session_cookie: &str,
    ) {
        let request = Request::builder()
            .method("POST")
            .uri(format!(
                "/tournaments/{}/rounds/{}/complete",
                tournament_id, round_id
            ))
            .header("content-type", "application/x-www-form-urlencoded")
            .header(COOKIE, session_cookie)
            .body(axum::body::Body::from(
                serde_urlencoded::to_string(&[("completed", "true")]).unwrap(),
            ))
            .unwrap();

        let response = app.ready().await.unwrap().call(request).await.unwrap();
        assert!(
            response.status().is_success()
                || response.status().is_redirection(),
            "Failed to complete round: {:?}",
            response.status()
        );
    }

    /// Helper: Build a map of judge names to their private URLs
    fn build_judge_name_to_private_url(
        &self,
        pool: &DbPool,
        tournament_id: &str,
    ) -> std::collections::HashMap<String, String> {
        let mut conn = pool.get().unwrap();
        tournament_judges::table
            .filter(tournament_judges::tournament_id.eq(tournament_id))
            .select((tournament_judges::name, tournament_judges::private_url))
            .load::<(String, String)>(&mut conn)
            .unwrap()
            .into_iter()
            .collect()
    }

    /// Helper: Build a map of speaker names to their IDs
    fn build_speaker_name_to_id(
        &self,
        pool: &DbPool,
        tournament_id: &str,
    ) -> std::collections::HashMap<String, String> {
        let mut conn = pool.get().unwrap();
        tournament_speakers::table
            .filter(tournament_speakers::tournament_id.eq(tournament_id))
            .select((tournament_speakers::name, tournament_speakers::id))
            .load::<(String, String)>(&mut conn)
            .unwrap()
            .into_iter()
            .collect()
    }

    /// Helper: Prepare ballot data (speaker IDs and scores) from ballot entries
    fn prepare_ballot_data(
        &self,
        workload_ballot: &BallotForSubmission,
        app_speaker_name_to_id: &std::collections::HashMap<String, String>,
        conf: &TournamentConfig,
    ) -> (Vec<String>, Vec<String>) {
        let mut app_speakers: Vec<String> = Vec::new();
        let mut app_scores: Vec<String> = Vec::new();

        let teams_per_side = conf.teams_per_side as usize;
        let speakers_per_team = conf.substantive_speakers as usize;

        // Since we've verified that the database state matches room.teams,
        // we can directly use positional indexing: workload_ballot.entries[i] corresponds to room.teams[i]
        // The form expects: for seq in 0..teams_per_side, for row in 0..speakers_per_team,
        // submit left team (2*seq) then right team (2*seq+1)
        for seq in 0..teams_per_side {
            for row in 0..speakers_per_team {
                // Left team (index 2*seq in room.teams)
                let team_idx_left = 2 * seq;
                if team_idx_left < workload_ballot.entries.len()
                    && row < workload_ballot.entries[team_idx_left].len()
                {
                    let (workload_speaker_name, workload_score) =
                        &workload_ballot.entries[team_idx_left][row];
                    let app_speaker_id = app_speaker_name_to_id
                        .get(workload_speaker_name)
                        .unwrap_or_else(|| {
                            panic!(
                                "Speaker '{}' not found",
                                workload_speaker_name
                            )
                        });
                    app_speakers.push(app_speaker_id.clone());
                    app_scores.push(workload_score.to_string());
                }

                // Right team (index 2*seq+1 in room.teams)
                let team_idx_right = 2 * seq + 1;
                if team_idx_right < workload_ballot.entries.len()
                    && row < workload_ballot.entries[team_idx_right].len()
                {
                    let (workload_speaker_name, workload_score) =
                        &workload_ballot.entries[team_idx_right][row];
                    let app_speaker_id = app_speaker_name_to_id
                        .get(workload_speaker_name)
                        .unwrap_or_else(|| {
                            panic!(
                                "Speaker '{}' not found",
                                workload_speaker_name
                            )
                        });
                    app_speakers.push(app_speaker_id.clone());
                    app_scores.push(workload_score.to_string());
                }
            }
        }

        (app_speakers, app_scores)
    }

    /// Helper: Edit the draw to match the provided RoundData
    ///
    /// This function:
    /// 1. Moves teams to their correct positions in each debate
    /// 2. Assigns judges (chairs, panelists, trainees) to debates
    /// 3. Verifies that the final draw state matches the expected RoundData
    /// Helper: Build a map of team names to their IDs
    fn build_team_name_to_id(
        &self,
        pool: &DbPool,
        tournament_id: &str,
    ) -> std::collections::HashMap<String, String> {
        let mut conn = pool.get().unwrap();
        tournament_teams::table
            .filter(tournament_teams::tournament_id.eq(tournament_id))
            .select((tournament_teams::name, tournament_teams::id))
            .load::<(String, String)>(&mut conn)
            .unwrap()
            .into_iter()
            .collect()
    }

    /// Helper: Build a map of judge names to their IDs
    fn build_judge_name_to_id(
        &self,
        pool: &DbPool,
        tournament_id: &str,
    ) -> std::collections::HashMap<String, String> {
        let mut conn = pool.get().unwrap();
        tournament_judges::table
            .filter(tournament_judges::tournament_id.eq(tournament_id))
            .select((tournament_judges::name, tournament_judges::id))
            .load::<(String, String)>(&mut conn)
            .unwrap()
            .into_iter()
            .collect()
    }

    /// Helper: Get current teams in a debate by position
    fn get_debate_teams_by_position(
        &self,
        pool: &DbPool,
        debate_id: &str,
    ) -> Vec<(String, String, i64, i64)> {
        let mut conn = pool.get().unwrap();
        tournament_debate_teams::table
            .filter(tournament_debate_teams::debate_id.eq(debate_id))
            .select((
                tournament_debate_teams::id,
                tournament_debate_teams::team_id,
                tournament_debate_teams::side,
                tournament_debate_teams::seq,
            ))
            .load(&mut conn)
            .unwrap()
    }

    /// Helper: Assign teams to their correct positions in a debate
    async fn assign_teams_to_debate(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        pool: &DbPool,
        tournament_id: &str,
        round_id: &str,
        debate_id: &str,
        workload_room_teams: &[String],
        session_cookie: &str,
    ) {
        let app_team_name_to_id =
            self.build_team_name_to_id(pool, tournament_id);

        for (i, workload_team_name) in workload_room_teams.iter().enumerate() {
            let expected_seq = (i / 2) as i64;
            let expected_side = (i % 2) as i64;

            let app_target_team_id =
                app_team_name_to_id.get(workload_team_name).unwrap_or_else(
                    || panic!("Team '{}' not found", workload_team_name),
                );

            let app_current_teams =
                self.get_debate_teams_by_position(pool, debate_id);
            let app_current_in_position =
                app_current_teams.iter().find(|(_, _, side, seq)| {
                    *side == expected_side && *seq == expected_seq
                });

            if let Some((_, app_current_team_id, _, _)) =
                app_current_in_position
            {
                if app_current_team_id != app_target_team_id {
                    self.move_team(
                        app,
                        tournament_id,
                        round_id,
                        app_target_team_id,
                        app_current_team_id,
                        session_cookie,
                    )
                    .await;
                }
            }
        }
    }

    /// Helper: Assign judges to their roles in a debate
    async fn assign_judges_to_debate(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        pool: &DbPool,
        tournament_id: &str,
        round_id: &str,
        debate_id: &str,
        workload_room: &RoomOfDraw,
        session_cookie: &str,
    ) {
        let app_judge_name_to_id =
            self.build_judge_name_to_id(pool, tournament_id);

        if let Some(app_judge_id) = app_judge_name_to_id.get(&workload_room.c) {
            self.move_judge(
                app,
                tournament_id,
                round_id,
                app_judge_id,
                "C",
                &debate_id.to_string(),
                session_cookie,
            )
            .await;
        }

        for workload_panel_name in &workload_room.p {
            if let Some(app_judge_id) =
                app_judge_name_to_id.get(workload_panel_name)
            {
                self.move_judge(
                    app,
                    tournament_id,
                    round_id,
                    app_judge_id,
                    "P",
                    &debate_id.to_string(),
                    session_cookie,
                )
                .await;
            }
        }

        for workload_trainee_name in &workload_room.t {
            if let Some(app_judge_id) =
                app_judge_name_to_id.get(workload_trainee_name)
            {
                self.move_judge(
                    app,
                    tournament_id,
                    round_id,
                    app_judge_id,
                    "T",
                    &debate_id.to_string(),
                    session_cookie,
                )
                .await;
            }
        }
    }

    /// Helper: Verify the draw matches the expected RoundData
    fn verify_draw_state(
        &self,
        pool: &DbPool,
        workload_round_data: &RoundData,
        app_debates: &[(String, i64)],
    ) {
        let mut conn = pool.get().unwrap();

        for (room_idx, workload_room) in
            workload_round_data.draw.iter().enumerate()
        {
            let (app_debate_id, _) = &app_debates[room_idx];

            let app_actual_teams: Vec<(String, i64, i64)> =
                tournament_debate_teams::table
                    .filter(
                        tournament_debate_teams::debate_id.eq(app_debate_id),
                    )
                    .inner_join(
                        tournament_teams::table.on(tournament_teams::id
                            .eq(tournament_debate_teams::team_id)),
                    )
                    .select((
                        tournament_teams::name,
                        tournament_debate_teams::side,
                        tournament_debate_teams::seq,
                    ))
                    .load(&mut conn)
                    .unwrap();

            for (i, workload_expected_team_name) in
                workload_room.teams.iter().enumerate()
            {
                let expected_seq = (i / 2) as i64;
                let expected_side = (i % 2) as i64;

                let app_actual_team = app_actual_teams
                    .iter()
                    .find(|(_, side, seq)| *side == expected_side && *seq == expected_seq)
                    .unwrap_or_else(|| {
                        panic!(
                            "No team found at position (side={}, seq={}) in debate {} (room {})",
                            expected_side, expected_seq, app_debate_id, room_idx
                        )
                    });

                assert_eq!(
                    &app_actual_team.0,
                    workload_expected_team_name,
                    "Team mismatch at position (side={}, seq={}) in debate {} (room {}): expected '{}', found '{}'",
                    expected_side,
                    expected_seq,
                    app_debate_id,
                    room_idx,
                    workload_expected_team_name,
                    app_actual_team.0
                );
            }
        }
    }

    /// Helper: Edit the draw to match the provided RoundData
    ///
    /// This function:
    /// 1. Assigns teams to their correct positions in each debate
    /// 2. Assigns judges (chairs, panelists, trainees) to debates
    /// 3. Verifies the final draw state matches expectations
    async fn modify_draw_to_match_workload(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        pool: &DbPool,
        tournament_id: &str,
        round_id: &str,
        workload_round_data: &RoundData,
        app_debates: &[(String, i64)],
        session_cookie: &str,
    ) {
        for (room_idx, workload_room) in
            workload_round_data.draw.iter().enumerate()
        {
            if room_idx >= app_debates.len() {
                panic!(
                    "RoundData has {} rooms but only {} debates exist",
                    workload_round_data.draw.len(),
                    app_debates.len()
                );
            }

            let (app_debate_id, _app_debate_number) = &app_debates[room_idx];

            self.assign_teams_to_debate(
                app,
                pool,
                tournament_id,
                round_id,
                app_debate_id,
                &workload_room.teams,
                session_cookie,
            )
            .await;

            self.assign_judges_to_debate(
                app,
                pool,
                tournament_id,
                round_id,
                app_debate_id,
                workload_room,
                session_cookie,
            )
            .await;
        }

        self.verify_draw_state(pool, workload_round_data, app_debates);
    }

    /// Simulates a complete round of the tournament.
    ///
    /// This function:
    /// 1. Fetches tournament and round IDs
    /// 2. Checks in judges and teams
    /// 3. Creates the draw
    /// 4. Edits the draw to match the provided RoundData
    /// 5. Releases the draw
    /// 6. Submits ballots from all judges
    /// 7. Marks the round as complete
    pub async fn simulate_round(
        &self,
        seq: usize,
        app: Router,
        pool: DbPool,
        session_cookie: &str,
    ) {
        let span =
            tracing::span!(tracing::Level::INFO, "simulate_round", seq = seq);
        let _guard = span.enter();

        let workload_round_data = &self.in_rounds[seq - 1]; // seq is 1-indexed
        let mut app = app.into_service();
        let mut app = app.ready().await.unwrap();

        let (app_tournament_id, app_round_id) =
            self.fetch_tournament_and_round_ids(&pool, seq).await;

        self.check_in_all_participants(
            &mut app,
            &app_tournament_id,
            &app_round_id,
            session_cookie,
        )
        .await;

        let app_debates = self
            .create_and_verify_draw(
                &mut app,
                &pool,
                &app_tournament_id,
                &app_round_id,
                session_cookie,
            )
            .await;

        self.modify_draw_to_match_workload(
            &mut app,
            &pool,
            &app_tournament_id,
            &app_round_id,
            workload_round_data,
            &app_debates,
            session_cookie,
        )
        .await;

        self.release_draw(
            &mut app,
            &app_tournament_id,
            &app_round_id,
            session_cookie,
        )
        .await;

        self.submit_all_ballots(
            &mut app,
            &pool,
            &app_tournament_id,
            &app_round_id,
            workload_round_data,
        )
        .await;

        self.complete_round(
            &mut app,
            &app_tournament_id,
            &app_round_id,
            session_cookie,
        )
        .await;
    }

    /// Fetch the tournament and round IDs from the database
    async fn fetch_tournament_and_round_ids(
        &self,
        pool: &DbPool,
        seq: usize,
    ) -> (String, String) {
        let mut conn = pool.get().unwrap();

        let app_tournament_id: String = tournaments::table
            .select(tournaments::id)
            .first(&mut conn)
            .unwrap();

        let app_round_id: String = tournament_rounds::table
            .filter(tournament_rounds::tournament_id.eq(&app_tournament_id))
            .filter(tournament_rounds::seq.eq(seq as i64))
            .select(tournament_rounds::id)
            .first(&mut conn)
            .unwrap();

        (app_tournament_id, app_round_id)
    }

    /// Check in all judges and teams for the round
    async fn check_in_all_participants(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        tournament_id: &str,
        round_id: &str,
        session_cookie: &str,
    ) {
        {
            let span = tracing::span!(tracing::Level::DEBUG, "check_in_judges");
            let _guard = span.enter();
            self.check_in_judges(app, tournament_id, round_id, session_cookie)
                .await;
        }

        {
            let span = tracing::span!(tracing::Level::DEBUG, "check_in_teams");
            let _guard = span.enter();
            self.check_in_teams(app, tournament_id, round_id, session_cookie)
                .await;
        }
    }

    /// Create the draw and verify it has the correct number of teams
    async fn create_and_verify_draw(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        pool: &DbPool,
        tournament_id: &str,
        round_id: &str,
        session_cookie: &str,
    ) -> Vec<(String, i64)> {
        {
            let span = tracing::span!(tracing::Level::DEBUG, "create_draw");
            let _guard = span.enter();
            assert_eq!(pool.state().idle_connections, 1);
            self.create_draw(app, tournament_id, round_id, session_cookie)
                .await;
            {
                let mut conn = pool.get().unwrap();
                self.check_draw_has_same_number_of_teams_as_workload(
                    round_id, &mut conn,
                );
            }
        }

        let app_debates: Vec<(String, i64)> = {
            let mut conn = pool.get().unwrap();
            tournament_debates::table
                .filter(tournament_debates::round_id.eq(round_id))
                .order_by(tournament_debates::number.asc())
                .select((tournament_debates::id, tournament_debates::number))
                .load(&mut conn)
                .unwrap()
        };

        app_debates
    }

    /// Submit ballots from all judges for the round
    async fn submit_all_ballots(
        &self,
        app: &mut impl Service<
            Request,
            Response = axum::response::Response,
            Error = std::convert::Infallible,
        >,
        pool: &DbPool,
        tournament_id: &str,
        round_id: &str,
        workload_round_data: &RoundData,
    ) {
        let span = tracing::span!(
            tracing::Level::DEBUG,
            "submit_ballots",
            num_debates = workload_round_data.draw.len()
        );
        let _guard = span.enter();

        // Compute useful mappings
        let app_judge_name_to_private_url =
            self.build_judge_name_to_private_url(pool, tournament_id);
        let app_speaker_name_to_id =
            self.build_speaker_name_to_id(pool, tournament_id);

        for (room_idx, workload_room) in
            workload_round_data.draw.iter().enumerate()
        {
            let room_span = tracing::span!(
                tracing::Level::TRACE,
                "submit_room_ballots",
                room = room_idx,
                num_judges = workload_room.ballots.len()
            );
            let _room_guard = room_span.enter();

            for (workload_judge_name, workload_ballot) in &workload_room.ballots
            {
                let app_private_url = app_judge_name_to_private_url
                    .get(workload_judge_name)
                    .unwrap_or_else(|| {
                        panic!("Judge '{}' not found", workload_judge_name)
                    });

                let (app_speakers, app_scores) = self.prepare_ballot_data(
                    workload_ballot,
                    &app_speaker_name_to_id,
                    &self.init.conf,
                );

                let judge_span = tracing::span!(
                    tracing::Level::TRACE,
                    "submit_ballot",
                    judge = workload_judge_name,
                    num_speakers = app_speakers.len()
                );
                let _judge_guard = judge_span.enter();

                self.submit_ballot(
                    app,
                    tournament_id,
                    round_id,
                    app_private_url,
                    &app_speakers,
                    &app_scores,
                )
                .await;
            }
        }
    }
}

#[cfg(test)]
pub mod regressions {
    use crate::test::rankings_workload::Workload;

    #[tokio::test]
    async fn crash_r1() {
        // let _default_guard = tracing::subscriber::set_global_default(
        //     tracing_subscriber::fmt::Subscriber::builder()
        //         .compact()
        //         .with_max_level(tracing::Level::TRACE)
        //         .finish(),
        // );

        tracing::info!("Starting test!");

        let input: Workload =
            serde_json::from_str(include_str!("rankings_regressions/r1.json"))
                .unwrap();

        let _ = input.run().await;
    }
}
