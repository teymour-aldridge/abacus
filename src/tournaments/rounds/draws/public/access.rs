use crate::{
    auth::User,
    permission::Permission,
    tournaments::{Tournament, rounds::Round},
    util_resp::FailureResponse,
};
use diesel::{connection::LoadConnection, sqlite::Sqlite};

pub struct DrawAccess {
    tournament_id: String,
    private: bool,
    superuser: bool,
    judge_alloc: bool,
    room_alloc: bool,
    show_draws: bool,
}
impl DrawAccess {
    pub fn resolve(
        tournament: &Tournament,
        user: Option<&User<true>>,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<Self, FailureResponse> {
        let capabilities = crate::permission::TournamentAccess::resolve(
            &tournament.id,
            user.map(|u| u.id.as_str()),
            conn,
        )?;
        Ok(Self {
            tournament_id: tournament.id.clone(),
            private: capabilities.allows(Permission::ViewDraw),
            superuser: capabilities.is_superuser(),
            judge_alloc: capabilities.allows(Permission::ManageJudgeAlloc),
            room_alloc: capabilities.allows(Permission::ManageRoomAlloc),
            show_draws: tournament.show_draws,
        })
    }
    pub fn can_generate(&self) -> bool {
        self.superuser
    }
    pub fn can_allocate(&self) -> bool {
        self.judge_alloc || self.room_alloc
    }

    pub fn round(&self, round: &Round) -> RoundDrawAccess {
        let belongs = round.tournament_id == self.tournament_id;
        let teams_public = belongs && self.show_draws && round.is_draw_public();
        let panel_public =
            belongs && self.show_draws && round.draw_status == "released_full";
        let exists = belongs && round.draw_status != "none";
        RoundDrawAccess {
            teams: exists && (self.private || teams_public),
            panel: exists && (self.private || panel_public),
            annotate: belongs && self.private,
            teams_public,
            panel_public,
        }
    }
}

#[derive(Clone, Copy)]
pub struct RoundDrawAccess {
    teams: bool,
    panel: bool,
    annotate: bool,
    teams_public: bool,
    panel_public: bool,
}
impl RoundDrawAccess {
    pub fn can_see_draw(&self) -> bool {
        self.teams || self.panel
    }
    pub fn can_see_teams(&self) -> bool {
        self.teams
    }
    pub fn can_see_panel(&self) -> bool {
        self.panel
    }
    pub fn annotate(&self) -> bool {
        self.annotate
    }
    pub fn teams_public(&self) -> bool {
        self.teams_public
    }
    pub fn panel_public(&self) -> bool {
        self.panel_public
    }
}
