use crate::schema::*;
use axum::body::Bytes;
use axum_test::{TestResponse, TestServer};
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use fuzzcheck::DefaultMutator;
use serde::{Deserialize, Serialize};

use super::inputs::{
    self, FuzzState, FuzzerBallotForm, MotionStringMutator,
    TabdaDictionaryStringMutator, UsizeMutator, make_usize_mutator,
};

macro_rules! get_id_by_idx {
    ($conn:expr, $query:expr, $idx:expr $(,)?) => {{
        let count = $query.count().get_result::<i64>($conn).unwrap_or(0);
        if count == 0 {
            None
        } else {
            let offset = ($idx as i64) % count;
            $query
                .offset(offset)
                .limit(1)
                .get_result::<String>($conn)
                .ok()
        }
    }};
}

#[derive(DefaultMutator, Clone, Debug, Hash, Serialize, Deserialize)]
pub enum Action {
    // Auth
    RegisterUser {
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        username: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        email: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        password: String,
    },
    LogoutUser,
    LoginUser {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        user_idx: usize,
    },

    // Tournaments
    CreateTournament {
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        abbrv: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        slug: String,
    },
    UpdateTournamentConfiguration {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        abbrv: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slug: Option<String>,
        #[serde(default)]
        show_draws: bool,
        #[serde(default)]
        show_round_results: bool,
        #[serde(default)]
        team_tab_public: bool,
        #[serde(default)]
        speaker_tab_public: bool,
        #[serde(default)]
        standings_public: bool,
        #[serde(default)]
        require_prelim_substantive_speaks: bool,
        #[serde(default)]
        require_prelim_speaker_order: bool,
        #[serde(default)]
        require_elim_substantive_speaks: bool,
        #[serde(default)]
        require_elim_speaker_order: bool,
        #[serde(default)]
        reply_speakers: bool,
        #[serde(default)]
        reply_must_speak: bool,
    },
    ApplyTournamentPreset {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        preset_idx: usize,
    },
    ViewTournamentPage {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        page_idx: usize,
    },

    // Participants
    CreateTeam {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        institution_idx: Option<usize>,
    },
    EditTeam {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        team_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        institution_idx: Option<usize>,
    },
    CreateSpeaker {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        team_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        email: String,
    },
    CreateJudge {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        email: String,
        institution_idx: Option<usize>,
    },
    EditJudge {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        judge_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        email: String,
        institution_idx: Option<usize>,
    },

    // Constraints
    AddConstraint {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        ptype: String,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        pid_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        category_idx: usize,
    },
    RemoveConstraint {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        ptype: String,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        pid_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        constraint_idx: usize,
    },
    MoveConstraint {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        ptype: String,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        pid_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        constraint_idx: usize,
        up: bool,
    },

    // Rooms
    CreateRoom {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        priority: i64,
    },
    DeleteRoom {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        room_idx: usize,
    },
    CreateRoomCategory {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default)]
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        private_name: String,
        #[serde(default)]
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        public_name: String,
        #[serde(default)]
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        description: String,
    },
    DeleteRoomCategory {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        category_idx: usize,
    },
    AddRoomToCategory {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        category_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        room_idx: usize,
    },
    RemoveRoomFromCategory {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        category_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        room_idx: usize,
    },

    // Feedback
    AddFeedbackQuestion {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[serde(alias = "question_text")]
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        question: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        question_type: String,
        #[serde(default)]
        for_judges: bool,
        #[serde(default)]
        for_teams: bool,
    },
    DeleteFeedbackQuestion {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        question_idx: usize,
    },
    EditFeedbackQuestion {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        question_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        question: String,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        question_type: String,
        seq: i64,
    },
    MoveFeedbackQuestion {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        question_idx: usize,
        up: bool,
    },
    SubmitFeedback {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        private_url_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        target_judge_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        question_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        answer: String,
    },

    // Rounds & Draws
    CreateRound {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        category_idx: Option<usize>,
        #[serde(default = "default_round_seq")]
        seq: u32,
    },
    EditRound {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        name: String,
        #[serde(default = "default_round_seq")]
        seq: u32,
    },
    GenerateDraw {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[serde(default)]
        force: bool,
    },
    SetDrawPublished {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[serde(default)]
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        published: Option<bool>,
    },
    SetRoundCompleted {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        completed: bool,
    },
    CreateMotion {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(MotionStringMutator = { MotionStringMutator::new() })]
        motion: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        infoslide: Option<String>,
    },
    PublishMotions {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
    },
    PublishResults {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
    },
    SubmitDrawCommand {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        role: String,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        judge_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        debate_idx: usize,
    },
    MoveJudge {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        judge_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        debate_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        role: String,
        unassign: bool,
    },
    MoveTeam {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        team1_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        team2_idx: usize,
    },
    ChangeJudgeRole {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        judge_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        debate_idx: usize,
        #[field_mutator(TabdaDictionaryStringMutator = { TabdaDictionaryStringMutator::new() })]
        role: String,
    },
    MoveDrawRoom {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        room_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        debate_idx: usize,
        unassign: bool,
    },

    // Availability & Eligibility
    UpdateJudgeAvailability {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        judge_idx: usize,
        available: bool,
    },
    UpdateAllJudgeAvailability {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        available: bool,
    },
    UpdateTeamEligibility {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        team_idx: usize,
        eligible: bool,
    },
    UpdateAllTeamEligibility {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        eligible: bool,
    },

    // Ballots
    SubmitBallot {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        private_url_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        form: FuzzerBallotForm,
    },
    EditBallot {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        debate_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        judge_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        motion_idx: usize,
        expected_version: i64,
    },

    // Additional GET coverage for pages not covered by the legacy ViewTournamentPage arm.
    ViewExtendedTournamentPage {
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        tournament_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        round_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        participant_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        debate_idx: usize,
        #[field_mutator(UsizeMutator = { make_usize_mutator() })]
        page_idx: usize,
    },
}

fn default_round_seq() -> u32 {
    1
}

struct ActionContext<'a> {
    pool: &'a Pool<ConnectionManager<SqliteConnection>>,
    client: &'a mut TestServer,
}

impl<'a> ActionContext<'a> {
    fn new(
        pool: &'a Pool<ConnectionManager<SqliteConnection>>,
        client: &'a mut TestServer,
    ) -> Self {
        Self { pool, client }
    }

    fn conn(
        &self,
    ) -> diesel::r2d2::PooledConnection<ConnectionManager<SqliteConnection>>
    {
        self.pool.get().unwrap()
    }

    async fn get(&mut self, action: &str, path: String) {
        let response = self.client.get(&path).await;
        assert_response_no_5xx(action, &path, &response);
    }

    async fn post_form<F>(&mut self, action: &str, path: String, form: &F)
    where
        F: ?Sized + Serialize,
    {
        let response = self.client.post(&path).form(form).await;
        assert_response_no_5xx(action, &path, &response);
    }

    async fn post_bytes(&mut self, action: &str, path: String, bytes: Vec<u8>) {
        let response = self.client.post(&path).bytes(Bytes::from(bytes)).await;
        assert_response_no_5xx(action, &path, &response);
    }

    fn tournament_id(&self, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            tournaments::table.select(tournaments::id).order_by(tournaments::id),
            idx,
        )
    }

    fn round_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            rounds::table
                .filter(rounds::tournament_id.eq(tournament_id))
                .select(rounds::id)
                .order_by(rounds::id),
            idx,
        )
    }

    fn round_seq(&self, tournament_id: &str, idx: usize) -> Option<i64> {
        let mut conn = self.conn();
        let count = rounds::table
            .filter(rounds::tournament_id.eq(tournament_id))
            .count()
            .get_result::<i64>(&mut *conn)
            .unwrap_or(0);
        if count == 0 {
            None
        } else {
            rounds::table
                .filter(rounds::tournament_id.eq(tournament_id))
                .select(rounds::seq)
                .order_by(rounds::id)
                .offset((idx as i64) % count)
                .limit(1)
                .get_result::<i64>(&mut *conn)
                .ok()
        }
    }

    fn judge_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            judges::table
                .filter(judges::tournament_id.eq(tournament_id))
                .select(judges::id)
                .order_by(judges::id),
            idx,
        )
    }

    fn speaker_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            speakers::table
                .filter(speakers::tournament_id.eq(tournament_id))
                .select(speakers::id)
                .order_by(speakers::id),
            idx,
        )
    }

    fn team_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            teams::table
                .filter(teams::tournament_id.eq(tournament_id))
                .select(teams::id)
                .order_by(teams::id),
            idx,
        )
    }

    fn debate_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            debates::table
                .filter(debates::tournament_id.eq(tournament_id))
                .select(debates::id)
                .order_by(debates::id),
            idx,
        )
    }

    fn debate_id_in_round(&self, round_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            debates::table
                .filter(debates::round_id.eq(round_id))
                .select(debates::id)
                .order_by(debates::id),
            idx,
        )
    }

    fn room_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            rooms::table
                .filter(rooms::tournament_id.eq(tournament_id))
                .select(rooms::id)
                .order_by(rooms::id),
            idx,
        )
    }

    fn room_category_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            room_categories::table
                .filter(room_categories::tournament_id.eq(tournament_id))
                .select(room_categories::id)
                .order_by(room_categories::id),
            idx,
        )
    }

    fn feedback_question_id(
        &self,
        tournament_id: &str,
        idx: usize,
    ) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            feedback_questions::table
                .filter(feedback_questions::tournament_id.eq(tournament_id))
                .select(feedback_questions::id)
                .order_by(feedback_questions::id),
            idx,
        )
    }

    fn motion_id(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        get_id_by_idx!(
            &mut *conn,
            motions_of_round::table
                .filter(motions_of_round::tournament_id.eq(tournament_id))
                .select(motions_of_round::id)
                .order_by(motions_of_round::id),
            idx,
        )
    }

    fn private_url(&self, tournament_id: &str, idx: usize) -> Option<String> {
        let mut conn = self.conn();
        let judge_count = judges::table
            .filter(judges::tournament_id.eq(tournament_id))
            .count()
            .get_result::<i64>(&mut *conn)
            .unwrap_or(0);
        let speaker_count = speakers::table
            .filter(speakers::tournament_id.eq(tournament_id))
            .count()
            .get_result::<i64>(&mut *conn)
            .unwrap_or(0);
        let total = judge_count + speaker_count;
        if total == 0 {
            return None;
        }
        let offset = (idx as i64) % total;
        if offset < judge_count {
            judges::table
                .filter(judges::tournament_id.eq(tournament_id))
                .select(judges::private_url)
                .order_by(judges::id)
                .offset(offset)
                .limit(1)
                .get_result::<String>(&mut *conn)
                .ok()
        } else {
            speakers::table
                .filter(speakers::tournament_id.eq(tournament_id))
                .select(speakers::private_url)
                .order_by(speakers::id)
                .offset(offset - judge_count)
                .limit(1)
                .get_result::<String>(&mut *conn)
                .ok()
        }
    }
}

fn assert_response_no_5xx(action: &str, path: &str, response: &TestResponse) {
    assert!(
        !response.status_code().is_server_error(),
        "{action} POST {path} returned {}\n{}",
        response.status_code(),
        response.text(),
    );
}

fn role_code(value: &str) -> &'static str {
    match value {
        "C" | "chair" | "Chair" => "C",
        "T" | "trainee" | "Trainee" => "T",
        _ => "P",
    }
}

impl Action {
    #[tracing::instrument(skip_all)]
    pub async fn run(
        self,
        pool: &Pool<ConnectionManager<SqliteConnection>>,
        client: &mut TestServer,
        state: &mut FuzzState,
    ) {
        match self {
            Action::RegisterUser { username, email, password } => inputs::Action::RegisterUser { username, email, password }.run(pool, client, state).await,
            Action::LogoutUser => inputs::Action::LogoutUser.run(pool, client, state).await,
            Action::LoginUser { user_idx } => inputs::Action::LoginUser { user_idx }.run(pool, client, state).await,
            Action::CreateTournament { name, abbrv, slug } => inputs::Action::CreateTournament { name, abbrv, slug }.run(pool, client, state).await,
            Action::UpdateTournamentConfiguration { tournament_idx, name, abbrv, slug, show_draws, show_round_results, team_tab_public, speaker_tab_public, standings_public, require_prelim_substantive_speaks, require_prelim_speaker_order, require_elim_substantive_speaks, require_elim_speaker_order, reply_speakers, reply_must_speak } => inputs::Action::UpdateTournamentConfiguration { tournament_idx, name, abbrv, slug, show_draws, show_round_results, team_tab_public, speaker_tab_public, standings_public, require_prelim_substantive_speaks, require_prelim_speaker_order, require_elim_substantive_speaks, require_elim_speaker_order, reply_speakers, reply_must_speak }.run(pool, client, state).await,
            Action::ApplyTournamentPreset { tournament_idx, preset_idx } => inputs::Action::ApplyTournamentPreset { tournament_idx, preset_idx }.run(pool, client, state).await,
            Action::ViewTournamentPage { tournament_idx, round_idx, page_idx } => inputs::Action::ViewTournamentPage { tournament_idx, round_idx, page_idx }.run(pool, client, state).await,
            Action::CreateTeam { tournament_idx, name, institution_idx } => inputs::Action::CreateTeam { tournament_idx, name, institution_idx }.run(pool, client, state).await,
            Action::EditTeam { tournament_idx, team_idx, name, institution_idx } => inputs::Action::EditTeam { tournament_idx, team_idx, name, institution_idx }.run(pool, client, state).await,
            Action::CreateSpeaker { tournament_idx, team_idx, name, email } => inputs::Action::CreateSpeaker { tournament_idx, team_idx, name, email }.run(pool, client, state).await,
            Action::CreateJudge { tournament_idx, name, email, institution_idx } => inputs::Action::CreateJudge { tournament_idx, name, email, institution_idx }.run(pool, client, state).await,
            Action::EditJudge { tournament_idx, judge_idx, name, email, institution_idx } => inputs::Action::EditJudge { tournament_idx, judge_idx, name, email, institution_idx }.run(pool, client, state).await,
            Action::AddConstraint { tournament_idx, ptype, pid_idx, category_idx } => inputs::Action::AddConstraint { tournament_idx, ptype, pid_idx, category_idx }.run(pool, client, state).await,
            Action::RemoveConstraint { tournament_idx, ptype, pid_idx, constraint_idx } => inputs::Action::RemoveConstraint { tournament_idx, ptype, pid_idx, constraint_idx }.run(pool, client, state).await,
            Action::CreateRoom { tournament_idx, name, priority } => inputs::Action::CreateRoom { tournament_idx, name, priority }.run(pool, client, state).await,
            Action::DeleteRoom { tournament_idx, room_idx } => inputs::Action::DeleteRoom { tournament_idx, room_idx }.run(pool, client, state).await,
            Action::CreateRoomCategory { tournament_idx, name, private_name, public_name, description } => inputs::Action::CreateRoomCategory { tournament_idx, name, private_name, public_name, description }.run(pool, client, state).await,
            Action::DeleteRoomCategory { tournament_idx, category_idx } => inputs::Action::DeleteRoomCategory { tournament_idx, category_idx }.run(pool, client, state).await,
            Action::AddRoomToCategory { tournament_idx, category_idx, room_idx } => inputs::Action::AddRoomToCategory { tournament_idx, category_idx, room_idx }.run(pool, client, state).await,
            Action::RemoveRoomFromCategory { tournament_idx, category_idx, room_idx } => inputs::Action::RemoveRoomFromCategory { tournament_idx, category_idx, room_idx }.run(pool, client, state).await,
            Action::AddFeedbackQuestion { tournament_idx, question, question_type, for_judges, for_teams } => inputs::Action::AddFeedbackQuestion { tournament_idx, question, question_type, for_judges, for_teams }.run(pool, client, state).await,
            Action::DeleteFeedbackQuestion { tournament_idx, question_idx } => inputs::Action::DeleteFeedbackQuestion { tournament_idx, question_idx }.run(pool, client, state).await,
            Action::EditFeedbackQuestion { tournament_idx, question_idx, question, question_type, seq } => inputs::Action::EditFeedbackQuestion { tournament_idx, question_idx, question, question_type, seq }.run(pool, client, state).await,
            Action::MoveFeedbackQuestion { tournament_idx, question_idx, up } => inputs::Action::MoveFeedbackQuestion { tournament_idx, question_idx, up }.run(pool, client, state).await,
            Action::CreateRound { tournament_idx, name, category_idx, seq } => inputs::Action::CreateRound { tournament_idx, name, category_idx, seq }.run(pool, client, state).await,
            Action::GenerateDraw { tournament_idx, round_idx, force } => inputs::Action::GenerateDraw { tournament_idx, round_idx, force }.run(pool, client, state).await,
            Action::SetDrawPublished { tournament_idx, round_idx, status, published } => inputs::Action::SetDrawPublished { tournament_idx, round_idx, status, published }.run(pool, client, state).await,
            Action::SetRoundCompleted { tournament_idx, round_idx, completed } => inputs::Action::SetRoundCompleted { tournament_idx, round_idx, completed }.run(pool, client, state).await,
            Action::PublishMotions { tournament_idx, round_idx } => inputs::Action::PublishMotions { tournament_idx, round_idx }.run(pool, client, state).await,
            Action::PublishResults { tournament_idx, round_idx } => inputs::Action::PublishResults { tournament_idx, round_idx }.run(pool, client, state).await,
            Action::UpdateJudgeAvailability { tournament_idx, round_idx, judge_idx, available } => inputs::Action::UpdateJudgeAvailability { tournament_idx, round_idx, judge_idx, available }.run(pool, client, state).await,
            Action::UpdateAllJudgeAvailability { tournament_idx, round_idx, available } => inputs::Action::UpdateAllJudgeAvailability { tournament_idx, round_idx, available }.run(pool, client, state).await,
            Action::UpdateTeamEligibility { tournament_idx, round_idx, team_idx, eligible } => inputs::Action::UpdateTeamEligibility { tournament_idx, round_idx, team_idx, eligible }.run(pool, client, state).await,
            Action::UpdateAllTeamEligibility { tournament_idx, round_idx, eligible } => inputs::Action::UpdateAllTeamEligibility { tournament_idx, round_idx, eligible }.run(pool, client, state).await,
            Action::SubmitBallot { tournament_idx, private_url_idx, round_idx, form } => inputs::Action::SubmitBallot { tournament_idx, private_url_idx, round_idx, form }.run(pool, client, state).await,

            Action::MoveConstraint { tournament_idx, ptype, pid_idx, constraint_idx, up } => {
                let mut ctx = ActionContext::new(pool, client);
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    let participant_type = if ptype == "speaker" || ptype == "speakers" { "speaker" } else { "judge" };
                    let pid = if participant_type == "speaker" {
                        ctx.speaker_id(&tid, pid_idx)
                    } else {
                        ctx.judge_id(&tid, pid_idx)
                    };
                    if let Some(pid) = pid {
                        if let Some(category_id) = ctx.room_category_id(&tid, constraint_idx) {
                            let direction = if up { "up" } else { "down" }.to_string();
                            let form = [("category_id", category_id), ("direction", direction)];
                            ctx.post_form(
                                "MoveConstraint",
                                format!("/tournaments/{}/participants/{}/{}/constraints/move", tid, participant_type, pid),
                                &form,
                            ).await;
                        }
                    }
                }
            }
            Action::EditRound { tournament_idx, round_idx, name, seq } => {
                let mut ctx = ActionContext::new(pool, client);
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let Some(rid) = ctx.round_id(&tid, round_idx) {
                        let form = [("name", name), ("seq", seq.to_string())];
                        ctx.post_form(
                            "EditRound",
                            format!("/tournaments/{}/rounds/{}/edit", tid, rid),
                            &form,
                        ).await;
                    }
                }
            }
            Action::CreateMotion { tournament_idx, round_idx, motion, infoslide } => {
                let mut ctx = ActionContext::new(pool, client);
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let Some(rid) = ctx.round_id(&tid, round_idx) {
                        let form = [
                            ("motion", motion),
                            ("infoslide", infoslide.unwrap_or_default()),
                        ];
                        ctx.post_form(
                            "CreateMotion",
                            format!("/tournaments/{}/rounds/{}/motions/create", tid, rid),
                            &form,
                        ).await;
                    }
                }
            }
            Action::SubmitFeedback { tournament_idx, private_url_idx, round_idx, target_judge_idx, question_idx, answer } => {
                let mut ctx = ActionContext::new(pool, client);
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let (Some(private_url), Some(rid), Some(target_judge_id), Some(question_id)) = (
                        ctx.private_url(&tid, private_url_idx),
                        ctx.round_id(&tid, round_idx),
                        ctx.judge_id(&tid, target_judge_idx),
                        ctx.feedback_question_id(&tid, question_idx),
                    ) {
                        let form = [("target_judge_id".to_string(), target_judge_id), (question_id, answer)];
                        ctx.post_form(
                            "SubmitFeedback",
                            format!("/tournaments/{}/privateurls/{}/rounds/{}/feedback/submit", tid, path_segment(&private_url), rid),
                            &form,
                        ).await;
                    }
                }
            }
            Action::SubmitDrawCommand { tournament_idx, round_idx, role, judge_idx, debate_idx } => {
                let mut ctx = ActionContext::new(pool, client);
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let Some(rid) = ctx.round_id(&tid, round_idx) {
                        let role = role_code(&role);
                        let cmd = format!("{}{} {}", role, judge_idx + 1, debate_idx + 1);
                        let form = [("cmd", cmd)];
                        ctx.post_form(
                            "SubmitDrawCommand",
                            format!("/tournaments/{}/rounds/draws/edit?rounds={}", tid, rid),
                            &form,
                        ).await;
                    }
                }
            }
            Action::MoveJudge { tournament_idx, round_idx, judge_idx, debate_idx, role, unassign } => {
                let mut ctx = ActionContext::new(pool, client);
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let (Some(rid), Some(judge_id)) = (ctx.round_id(&tid, round_idx), ctx.judge_id(&tid, judge_idx)) {
                        let to_debate_id = if unassign { String::new() } else { ctx.debate_id_in_round(&rid, debate_idx).unwrap_or_default() };
                        let form = [
                            ("judge_id", judge_id),
                            ("to_debate_id", to_debate_id),
                            ("role", role_code(&role).to_string()),
                            ("rounds", rid),
                        ];
                        ctx.post_form("MoveJudge", format!("/tournaments/{}/rounds/draws/edit/move", tid), &form).await;
                    }
                }
            }
            Action::MoveTeam { tournament_idx, round_idx, team1_idx, team2_idx } => {
                let mut ctx = ActionContext::new(pool, client);
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let (Some(rid), Some(team1_id), Some(team2_id)) = (
                        ctx.round_id(&tid, round_idx),
                        ctx.team_id(&tid, team1_idx),
                        ctx.team_id(&tid, team2_idx),
                    ) {
                        let form = [("team1_id", team1_id), ("team2_id", team2_id), ("rounds", rid)];
                        ctx.post_form("MoveTeam", format!("/tournaments/{}/rounds/draws/edit/move_team", tid), &form).await;
                    }
                }
            }
            Action::ChangeJudgeRole { tournament_idx, round_idx, judge_idx, debate_idx, role } => {
                let mut ctx = ActionContext::new(pool, client);
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let (Some(rid), Some(judge_id)) = (ctx.round_id(&tid, round_idx), ctx.judge_id(&tid, judge_idx)) {
                        if let Some(debate_id) = ctx.debate_id_in_round(&rid, debate_idx) {
                            let form = [
                                ("judge_id", judge_id),
                                ("debate_id", debate_id),
                                ("role", role_code(&role).to_string()),
                                ("rounds", rid),
                            ];
                            ctx.post_form("ChangeJudgeRole", format!("/tournaments/{}/rounds/draws/edit/role", tid), &form).await;
                        }
                    }
                }
            }
            Action::MoveDrawRoom { tournament_idx, round_idx, room_idx, debate_idx, unassign } => {
                let mut ctx = ActionContext::new(pool, client);
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let (Some(rid), Some(room_id)) = (ctx.round_id(&tid, round_idx), ctx.room_id(&tid, room_idx)) {
                        let to_debate_id = if unassign { String::new() } else { ctx.debate_id_in_round(&rid, debate_idx).unwrap_or_default() };
                        let form = [("room_id", room_id), ("to_debate_id", to_debate_id), ("rounds", rid)];
                        ctx.post_form("MoveDrawRoom", format!("/tournaments/{}/rounds/draws/rooms/edit/move", tid), &form).await;
                    }
                }
            }
            Action::EditBallot { tournament_idx, debate_idx, judge_idx, motion_idx, expected_version } => {
                let mut ctx = ActionContext::new(pool, client);
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    if let (Some(debate_id), Some(judge_id), Some(motion_id)) = (
                        ctx.debate_id(&tid, debate_idx),
                        ctx.judge_id(&tid, judge_idx),
                        ctx.motion_id(&tid, motion_idx),
                    ) {
                        let body = format!("motion_id={}&expected_version={}", path_segment(&motion_id), expected_version);
                        ctx.post_bytes(
                            "EditBallot",
                            format!("/tournaments/{}/debates/{}/judges/{}/edit", tid, debate_id, judge_id),
                            body.into_bytes(),
                        ).await;
                    }
                }
            }
            Action::ViewExtendedTournamentPage { tournament_idx, round_idx, participant_idx, debate_idx, page_idx } => {
                let mut ctx = ActionContext::new(pool, client);
                if let Some(tid) = ctx.tournament_id(tournament_idx) {
                    let rid = ctx.round_id(&tid, round_idx);
                    let round_seq = ctx.round_seq(&tid, round_idx);
                    let team_id = ctx.team_id(&tid, participant_idx);
                    let judge_id = ctx.judge_id(&tid, participant_idx);
                    let speaker_id = ctx.speaker_id(&tid, participant_idx);
                    let debate_id = ctx.debate_id(&tid, debate_idx);
                    let private_url = ctx.private_url(&tid, participant_idx);
                    let path = match page_idx % 16 {
                        0 => team_id.map(|id| format!("/tournaments/{}/teams/{}", tid, id)),
                        1 => team_id.map(|id| format!("/tournaments/{}/teams/{}/edit", tid, id)),
                        2 => team_id.map(|id| format!("/tournaments/{}/teams/{}/speakers/create", tid, id)),
                        3 => judge_id.map(|id| format!("/tournaments/{}/judges/{}/edit", tid, id)),
                        4 => speaker_id.map(|id| format!("/tournaments/{}/participants/speaker/{}/constraints", tid, id)),
                        5 => judge_id.map(|id| format!("/tournaments/{}/participants/judge/{}/constraints", tid, id)),
                        6 => round_seq.map(|seq| format!("/tournaments/{}/rounds/{}/draw", tid, seq)),
                        7 => round_seq.map(|seq| format!("/tournaments/{}/rounds/{}/results", tid, seq)),
                        8 => rid.as_ref().map(|id| format!("/tournaments/{}/rounds/{}/edit", tid, id)),
                        9 => rid.as_ref().map(|id| format!("/tournaments/{}/rounds/draws/edit?rounds={}", tid, id)),
                        10 => rid.as_ref().map(|id| format!("/tournaments/{}/rounds/draws/rooms/edit?rounds={}", tid, id)),
                        11 => rid.as_ref().map(|id| format!("/tournaments/{}/rounds/{}/draws/create", tid, id)),
                        12 => debate_id.as_ref().map(|id| format!("/tournaments/{}/debates/{}/ballots", tid, id)),
                        13 => debate_id.as_ref().map(|id| format!("/tournaments/{}/debates/{}/ballots/none/view", tid, id)),
                        14 => private_url.map(|url| format!("/tournaments/{}/privateurls/{}", tid, path_segment(&url))),
                        15 => debate_id.and_then(|did| judge_id.map(|jid| format!("/tournaments/{}/debates/{}/judges/{}/edit", tid, did, jid))),
                        _ => unreachable!(),
                    };
                    if let Some(path) = path {
                        ctx.get("ViewExtendedTournamentPage", path).await;
                    }
                }
            }
        }
    }
}

fn path_segment(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string()
}
