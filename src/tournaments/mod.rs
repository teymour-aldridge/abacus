#[cfg(not(debug_assertions))]
pub const WEBSOCKET_SCHEME: &str = "wss://";

#[cfg(debug_assertions)]
pub const WEBSOCKET_SCHEME: &str = "ws://";

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};

use crate::{
    permission::Permission,
    schema::{
        groups, members_of_group, org, permissions_of_group, tournaments,
    },
    tournaments::{
        config::{PullupMetric, RankableTeamMetric},
        rounds::{Round, ballots::aggregate::BallotAggregationMethod},
    },
    util_resp::{FailureResponse, unauthorized},
};

pub mod categories;
pub mod config;
pub mod create;
pub mod feedback;
pub mod manage;
pub mod motions;
pub mod participants;
pub mod privateurls;
pub mod public;
pub mod rooms;
pub mod rounds;
pub mod snapshots;
pub mod standings;
pub mod teams;
pub mod view;

#[derive(Queryable, Clone, Debug)]
pub struct Tournament {
    pub id: String,
    pub name: String,
    pub abbrv: String,
    pub slug: String,
    pub created_at: chrono::NaiveDateTime,
    pub team_tab_public: bool,
    pub speaker_tab_public: bool,
    pub standings_public: bool,
    pub show_round_results: bool,
    pub show_draws: bool,
    pub teams_per_side: i64,
    pub substantive_speakers: i64,
    pub reply_speakers: bool,
    pub reply_must_speak: bool,
    pub max_substantive_speech_index_for_reply: Option<i64>,
    pub pool_ballot_setup: String,
    pub elim_ballot_setup: String,
    pub margin_includes_dissenters: bool,
    pub require_prelim_substantive_speaks: bool,
    pub require_prelim_speaker_order: bool,
    pub require_elim_substantive_speaks: bool,
    pub require_elim_speaker_order: bool,
    pub substantive_speech_min_speak: Option<f32>,
    pub substantive_speech_max_speak: Option<f32>,
    pub substantive_speech_step: Option<f32>,
    pub reply_speech_min_speak: Option<f32>,
    pub reply_speech_max_speak: Option<f32>,
    pub institution_penalty: i64,
    pub history_penalty: i64,
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

pub enum RoundKind {
    Elim,
    Prelim,
}

impl Tournament {
    pub fn agg_method_for_current_round(
        &self,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> BallotAggregationMethod {
        match self.current_round_type(conn) {
            RoundKind::Elim => {
                if self.elim_is_consensus() {
                    BallotAggregationMethod::Consensus
                } else {
                    BallotAggregationMethod::Individual
                }
            }
            RoundKind::Prelim => {
                if self.pool_is_consensus() {
                    BallotAggregationMethod::Consensus
                } else {
                    BallotAggregationMethod::Individual
                }
            }
        }
    }

    // todo: obviously retrieving all the rounds first is not necessary here
    pub fn current_round_type(
        &self,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> RoundKind {
        match Round::current_rounds(&self.id, conn)
            .get(0)
            .unwrap()
            .kind
            .as_str()
        {
            "P" => RoundKind::Prelim,
            "E" => RoundKind::Elim,
            _ => unreachable!(),
        }
    }

    /// Note: concurrent rounds will always have the same scoring rules.
    pub fn current_round_is_consensus(
        &self,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> bool {
        match self.current_round_type(conn) {
            RoundKind::Elim => self.elim_is_consensus(),
            RoundKind::Prelim => self.pool_is_consensus(),
        }
    }

    pub fn current_round_requires_speaker_order(
        &self,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> bool {
        match self.current_round_type(conn) {
            RoundKind::Elim => self.require_elim_speaker_order,
            RoundKind::Prelim => self.require_prelim_speaker_order,
        }
    }

    pub fn current_round_requires_speaks(
        &self,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> bool {
        match self.current_round_type(conn) {
            RoundKind::Elim => self.require_elim_substantive_speaks,
            RoundKind::Prelim => self.require_prelim_substantive_speaks,
        }
    }

    // todo: consistent naming of preliminary/in-rounds/pool
    pub fn pool_is_consensus(&self) -> bool {
        self.pool_ballot_setup == "consensus"
    }

    pub fn elim_is_consensus(&self) -> bool {
        self.elim_ballot_setup == "consensus"
    }

    pub fn pool_uses_speaks(&self) -> bool {
        self.require_prelim_substantive_speaks
    }

    pub fn round_requires_speaker_order(&self, round: &Round) -> bool {
        if round.is_elim() {
            self.require_elim_speaker_order
        } else {
            self.require_prelim_speaker_order
        }
    }

    pub fn round_requires_speaks(&self, round: &Round) -> bool {
        if round.is_elim() {
            self.require_elim_substantive_speaks
        } else {
            self.require_prelim_substantive_speaks
        }
    }

    pub fn max_substantive_speak(&self) -> Option<rust_decimal::Decimal> {
        self.substantive_speech_max_speak
            .and_then(rust_decimal::Decimal::from_f32_retain)
    }

    pub fn min_substantive_speak(&self) -> Option<rust_decimal::Decimal> {
        self.substantive_speech_min_speak
            .and_then(rust_decimal::Decimal::from_f32_retain)
    }

    pub fn speak_step(&self) -> Option<rust_decimal::Decimal> {
        self.substantive_speech_step
            .and_then(rust_decimal::Decimal::from_f32_retain)
    }

    pub fn check_score_valid(
        &self,
        score: rust_decimal::Decimal,
        is_reply: bool,
        speaker_name: String,
    ) -> Result<(), String> {
        if !is_reply {
            if let Some(min) = self.min_substantive_speak() {
                if score < min {
                    return Err(format!(
                        "Score of {score} for {speaker_name} is lower than the minimum permissible speak {min}.",
                    ));
                }
            }

            if let Some(max) = self.max_substantive_speak() {
                if max < score {
                    return Err(format!(
                        "Score of {score} for {speaker_name} is greater than the maximum permissible speak {max}.",
                    ));
                }
            }

            if let Some(step) = self.speak_step() {
                if step != rust_decimal::Decimal::ZERO
                    && score % step != rust_decimal::Decimal::ZERO
                {
                    return Err(format!(
                        "Score of {score} for {speaker_name} does not match requirement \
                         that the score be a multiple of {step}.",
                    ));
                }
            }
        } else {
            todo!("validation for reply speaks")
        }

        Ok(())
    }

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
            org::table.filter(
                org::user_id
                    .eq(user_id)
                    .and(org::tournament_id.eq(&self.id))
                    .and(org::is_superuser.eq(true)),
            ),
        ))
        .get_result::<bool>(conn)
        .unwrap()
        {
            true => Ok(()),
            false => unauthorized().map(|_| ()),
        }
    }

    #[tracing::instrument(skip(conn))]
    pub fn check_user_has_permission(
        &self,
        user_id: &str,
        permission: Permission,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<(), FailureResponse> {
        let has_permission = diesel::dsl::select(diesel::dsl::exists(
            org::table
                .filter(org::user_id.eq(user_id))
                .filter(org::tournament_id.eq(&self.id))
                .inner_join(
                    groups::table.on(diesel::dsl::exists(
                        members_of_group::table.filter(
                            members_of_group::group_id
                                .eq(groups::id)
                                .and(members_of_group::member_id.eq(org::id)),
                        ),
                    )),
                )
                .inner_join(
                    permissions_of_group::table
                        .on(permissions_of_group::group_id.eq(groups::id)),
                )
                .filter(
                    permissions_of_group::permission
                        .eq(serde_json::to_string(&permission).unwrap()),
                )
                .select(true.into_sql::<diesel::sql_types::Bool>()),
        ));
        let is_superuser = org::table
            .filter(org::user_id.eq(user_id).and(org::is_superuser.eq(true)))
            .select(true.into_sql::<diesel::sql_types::Bool>());
        let select = diesel::dsl::select(diesel::dsl::exists(
            has_permission.union(is_superuser),
        ));

        let get_result = select.get_result::<bool>(conn).unwrap();

        tracing::trace!(
            "User has permission {:?} = {}",
            permission,
            get_result
        );

        match get_result {
            true => Ok(()),
            false => unauthorized().map(|_| ()),
        }
    }

    pub fn speaker_position_name(
        &self,
        side: i64,
        seq: i64,
        speaker_position: i64,
    ) -> &'static str {
        match (self.teams_per_side, side, seq, speaker_position) {
            (1, 0, 0, 0) => "1st Prop",
            (1, 0, 0, 1) => "2nd Prop",
            (1, 0, 0, 2) => "3rd Prop",
            (1, 0, 0, 3) => "Prop Reply",
            (1, 1, 0, 0) => "1st Opp",
            (1, 1, 0, 1) => "2nd Opp",
            (1, 1, 0, 2) => "3rd Opp",
            (1, 1, 0, 3) => "Opp Reply",
            (2, 0, 0, 0) => "PM",
            (2, 0, 0, 1) => "DPM",
            (2, 1, 0, 0) => "LO",
            (2, 1, 0, 1) => "DLO",
            (2, 0, 1, 0) => "MG",
            (2, 0, 1, 1) => "GW",
            (2, 1, 1, 0) => "MO",
            (2, 1, 1, 1) => "OW",
            _ => unreachable!("invalid position provided"),
        }
    }
}
