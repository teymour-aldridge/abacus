use std::borrow::Borrow;

use diesel::prelude::*;
use rocket::{
    Request, State,
    http::Status,
    outcome::try_outcome,
    request::{self, FromRequest, Outcome},
};
use tokio::task::spawn_blocking;

use crate::{
    schema::tournaments,
    state::{Conn, DbPool},
};

pub mod categories;
pub mod config;
pub mod create;
pub mod participants;
pub mod rounds;
pub mod snapshots;
pub mod teams;
pub mod view;

#[derive(Queryable, Clone, Debug)]
pub struct Tournament {
    pub id: String,
    pub name: String,
    pub abbrv: String,
    pub slug: String,
    pub created_at: chrono::NaiveDateTime,
    pub teams_per_side: i64,
    pub substantive_speakers: i64,
    pub reply_speakers: bool,
    pub reply_must_speak: bool,
    pub max_substantive_speech_index_for_reply: Option<i64>,
    pub pool_ballot_setup: String,
    pub elim_ballot_setup: String,
    pub elim_ballots_require_speaks: bool,
    pub institution_penalty: Option<i64>,
    pub history_penalty: Option<i64>,
    pub team_standings_metrics: String,
    pub speaker_standings_metrics: String,
    pub exclude_from_speaker_standings_after: Option<i64>,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Tournament {
    type Error = ();

    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, ()> {
        let route = if let Some(route) = request.route() {
            route
        } else {
            unreachable!(
                "this route should only be used for requests to tournament views"
            );
        };

        let route = route.name.as_ref().expect("all routes should be named!");

        let tid: String = match route.borrow() {
            "admin_view_tournament"
            | "manage_team_page"
            | "manage_tournament_participants"
            | "create_teams_page"
            | "do_create_team" => request
                .routed_segment(1)
                .expect("failed to retrieve tournament id")
                .to_string(),
            _ => {
                unreachable!(
                    "this route should only be used for requests to tournament views"
                );
            }
        };

        let conn = try_outcome!(
            request.guard::<Conn>().await.map_error(|(t, _)| (t, ()))
        );

        let tournament = spawn_blocking(move || {
            let mut locked = conn.get_sync();

            tournaments::table
                .filter(tournaments::id.eq(tid))
                .first::<Tournament>(&mut *locked)
                .optional()
                .expect("failed to execute query")
        })
        .await
        .unwrap();

        request.local_cache(|| tournament.clone());

        match tournament {
            Some(t) => Outcome::Success(t),
            None => Outcome::Error((Status::NotFound, ())),
        }
    }
}

pub struct TournamentNoTx(pub Tournament);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for TournamentNoTx {
    type Error = ();

    async fn from_request(
        request: &'r Request<'_>,
    ) -> request::Outcome<Self, ()> {
        let route = if let Some(route) = request.route() {
            route
        } else {
            unreachable!(
                "this route should only be used for requests to tournament views"
            );
        };

        let route = route.name.as_ref().expect("all routes should be named!");

        let tid: String = match route.borrow() {
            "admin_view_tournament"
            | "manage_team_page"
            | "manage_tournament_participants"
            | "create_teams_page"
            | "do_create_team" => request
                .routed_segment(1)
                .expect("failed to retrieve tournament id")
                .to_string(),
            _ => {
                unreachable!(
                    "this route should only be used for requests to tournament views"
                );
            }
        };

        let pool: DbPool = try_outcome!(
            request
                .guard::<&State<DbPool>>()
                .await
                .map_error(|(t, _)| (t, ()))
        )
        .inner()
        .clone();

        let tournament = spawn_blocking(move || {
            let mut conn = pool.get().unwrap();

            tournaments::table
                .filter(tournaments::id.eq(tid))
                .first::<Tournament>(&mut conn)
                .optional()
                .expect("failed to execute query")
        })
        .await
        .unwrap();

        request.local_cache(|| tournament.clone());

        match tournament {
            Some(t) => Outcome::Success(TournamentNoTx(t)),
            None => Outcome::Error((Status::NotFound, ())),
        }
    }
}
