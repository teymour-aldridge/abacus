#[cfg(not(test))]
pub const WEBSOCKET_SCHEME: &str = "wss://";

#[cfg(test)]
pub const WEBSOCKET_SCHEME: &str = "ws://";

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};

use crate::{
    schema::{tournament_members, tournaments},
    util_resp::FailureResponse,
};

pub mod categories;
pub mod config;
pub mod create;
pub mod manage;
pub mod participants;
pub mod public;
pub mod rounds;
pub mod snapshots;
pub mod standings;
pub mod teams;

#[derive(Queryable, Clone, Debug)]
pub struct Tournament {
    pub id: String,
    pub name: String,
    pub abbrv: String,
    pub slug: String,
    pub created_at: chrono::NaiveDateTime,
    pub team_tab_public: bool,
    pub speaker_tab_public: bool,
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
    pub pullup_metrics: String,
    pub repeat_pullup_penalty: i64,
    pub team_standings_metrics: String,
    pub speaker_standings_metrics: String,
    pub exclude_from_speaker_standings_after: Option<i64>,
}

pub enum UserRole {
    Tab,
    Equity,
    CAP,
}

impl Tournament {
    pub fn fetch(
        id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<Tournament, FailureResponse> {
        tournaments::table
            .filter(tournaments::id.eq(id))
            .first::<Tournament>(conn)
            .map_err(|err| match err {
                diesel::result::Error::NotFound => {
                    FailureResponse::NotFound(())
                }
                _ => FailureResponse::ServerError(()),
            })
    }

    pub fn check_user_is_tab_dir(
        &self,
        user_id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<(), FailureResponse> {
        match self.get_user_role(user_id, conn) {
            Some(UserRole::Tab) => Ok(()),
            _ => Err(FailureResponse::Unauthorized(())),
        }
    }

    /// Gets the most significant user role
    pub fn get_user_role(
        &self,
        user_id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Option<UserRole> {
        let (is_ca, is_equity, is_tab) = tournament_members::table
            .filter(tournament_members::user_id.eq(user_id))
            .select((
                tournament_members::is_ca,
                tournament_members::is_equity,
                tournament_members::is_superuser,
            ))
            .first::<(bool, bool, bool)>(conn)
            .optional()
            .unwrap()
            .unwrap_or((false, false, false));

        Some(if is_tab {
            UserRole::Tab
        } else if is_ca {
            UserRole::CAP
        } else if is_equity {
            UserRole::Equity
        } else {
            return None;
        })
    }
}
