use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum Permission {
    /// Permission to manage judge<->team conflicts as well as judge<->judge
    /// conflicts.
    #[serde(rename = "participant_conflicts_manage")]
    ManageParticipantConflicts,
    /// Manage all participant details.
    #[serde(rename = "participant_manage")]
    ManageParticipants,
    /// Permission to view judge<->team conflicts as well as team<->team
    /// conflicts.
    #[serde(rename = "conflicts_view")]
    ViewConflicts,
    /// Permission to manage judge allocations. Usually assigned to CA teams.
    /// Assigning this permission will
    #[serde(rename = "judge_alloc_manage")]
    ManageJudgeAlloc,
    /// Permission to change room allocations (i.e. which debate will take place
    /// in which room).
    #[serde(rename = "room_alloc_manage")]
    ManageRoomAlloc,
    /// Permission to view the draw. Note that in many cases other permissions
    /// will be required to see specific parts of the draw.
    #[serde(rename = "draw_view")]
    ViewDraw,
}
