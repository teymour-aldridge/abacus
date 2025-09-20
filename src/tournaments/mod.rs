#[cfg(not(test))]
pub const WEBSOCKET_SCHEME: &str = "wss://";

#[cfg(test)]
pub const WEBSOCKET_SCHEME: &str = "ws://";

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};

use crate::{
    permission::Permission,
    schema::{
        tournament_group_members, tournament_group_permissions,
        tournament_groups, tournament_members, tournaments,
    },
    tournaments::config::{PullupMetric, RankableTeamMetric},
    util_resp::{FailureResponse, unauthorized},
};

pub mod categories;
pub mod config;
pub mod create;
pub mod manage;
pub mod participants;
pub mod privateurls;
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
    pub fn pullup_metrics(&self) -> Vec<PullupMetric> {
        serde_json::from_str(&self.pullup_metrics).unwrap()
    }

    pub fn metrics(&self) -> Vec<RankableTeamMetric> {
        serde_json::from_str(&self.team_standings_metrics).unwrap()
    }

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

    pub fn check_user_is_superuser(
        &self,
        user_id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<(), FailureResponse> {
        match diesel::select(diesel::dsl::exists(
            tournament_members::table.filter(
                tournament_members::user_id
                    .eq(user_id)
                    .and(tournament_members::tournament_id.eq(&self.id))
                    .and(tournament_members::is_superuser.eq(true)),
            ),
        ))
        .get_result::<bool>(conn)
        .unwrap()
        {
            true => Ok(()),
            false => unauthorized().map(|_| ()),
        }
    }

    pub fn check_user_has_permission(
        &self,
        user_id: &str,
        permission: Permission,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<(), FailureResponse> {
        match diesel::dsl::select(diesel::dsl::exists(
            tournament_members::table
                .filter(tournament_members::user_id.eq(user_id))
                .filter(tournament_members::tournament_id.eq(&self.id))
                .inner_join(
                    tournament_groups::table.on(diesel::dsl::exists(
                        tournament_group_members::table.filter(
                            tournament_group_members::group_id
                                .eq(tournament_groups::id)
                                .and(
                                    tournament_group_members::member_id
                                        .eq(tournament_members::id),
                                ),
                        ),
                    )),
                )
                .inner_join(
                    tournament_group_permissions::table
                        .on(tournament_group_permissions::group_id
                            .eq(tournament_groups::id)),
                )
                .filter(
                    tournament_group_permissions::permission
                        .eq(serde_json::to_string(&permission).unwrap()),
                )
                .select(true.into_sql::<diesel::sql_types::Bool>())
                .union(
                    tournament_members::table
                        .filter(
                            tournament_members::user_id
                                .eq(user_id)
                                .and(tournament_members::is_superuser.eq(true)),
                        )
                        .select(true.into_sql::<diesel::sql_types::Bool>()),
                ),
        ))
        .get_result::<bool>(conn)
        .unwrap()
        {
            true => Ok(()),
            false => unauthorized().map(|_| ()),
        }
    }
}
