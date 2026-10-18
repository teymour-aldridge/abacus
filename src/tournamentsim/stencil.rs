//! Short create-then-use sequences appended to an existing workload prefix.

use super::{
    WorkloadInput,
    inputs::{Action, LATEST_OBJECT_INDEX},
};

#[derive(Clone, Copy)]
pub(super) enum Stencil {
    EditTeam,
    AddSpeakerToTeam,
    EditJudge,
    AddMotionToRound,
    AddRoomToCategory,
}

pub(super) const STENCILS: [Stencil; 5] = [
    Stencil::EditTeam,
    Stencil::AddSpeakerToTeam,
    Stencil::EditJudge,
    Stencil::AddMotionToRound,
    Stencil::AddRoomToCategory,
];

impl Stencil {
    pub(super) fn sample(
        self,
        prefix: &WorkloadInput,
        nonce: usize,
    ) -> Option<Vec<Action>> {
        let mut logged_in = false;
        let mut tournament_exists = false;
        for action in &prefix.actions {
            match action {
                Action::RegisterUser { .. } | Action::LoginUser { .. } => {
                    logged_in = true;
                }
                Action::LogoutUser => logged_in = false,
                Action::CreateTournament { .. } => tournament_exists = true,
                _ => {}
            }
        }
        if !logged_in || !tournament_exists {
            return None;
        }

        let tournament_idx = LATEST_OBJECT_INDEX;
        let latest = LATEST_OBJECT_INDEX;
        let actions = match self {
            Stencil::EditTeam => vec![
                Action::CreateTeam {
                    tournament_idx,
                    name: format!("Stencil team {nonce}"),
                    institution_idx: None,
                },
                Action::EditTeam {
                    tournament_idx,
                    team_idx: latest,
                    name: format!("Edited stencil team {nonce}"),
                    institution_idx: None,
                },
            ],
            Stencil::AddSpeakerToTeam => vec![
                Action::CreateTeam {
                    tournament_idx,
                    name: format!("Speaker team {nonce}"),
                    institution_idx: None,
                },
                Action::CreateSpeaker {
                    tournament_idx,
                    team_idx: latest,
                    name: format!("Stencil speaker {nonce}"),
                    email: format!("stencil-speaker-{nonce}@example.org"),
                },
            ],
            Stencil::EditJudge => vec![
                Action::CreateJudge {
                    tournament_idx,
                    name: format!("Stencil judge {nonce}"),
                    email: format!("stencil-judge-{nonce}@example.org"),
                    institution_idx: None,
                },
                Action::EditJudge {
                    tournament_idx,
                    judge_idx: latest,
                    name: format!("Edited stencil judge {nonce}"),
                    email: format!("stencil-judge-{nonce}@example.org"),
                    institution_idx: None,
                },
            ],
            Stencil::AddMotionToRound => vec![
                Action::CreateRound {
                    tournament_idx,
                    name: format!("Stencil round {nonce}"),
                    category_idx: None,
                    seq: (nonce % 199 + 1) as u32,
                },
                Action::CreateMotion {
                    tournament_idx,
                    round_idx: latest,
                    motion: format!("Stencil motion {nonce}"),
                    infoslide: None,
                },
            ],
            Stencil::AddRoomToCategory => vec![
                Action::CreateRoomCategory {
                    tournament_idx,
                    name: None,
                    private_name: format!("Stencil category {nonce}"),
                    public_name: format!("Stencil category {nonce}"),
                    description: "Stencil".into(),
                },
                Action::CreateRoom {
                    tournament_idx,
                    name: format!("Stencil room {nonce}"),
                    priority: 0,
                },
                Action::AddRoomToCategory {
                    tournament_idx,
                    category_idx: latest,
                    room_idx: latest,
                },
            ],
        };
        Some(actions)
    }
}
