use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(rename = "judge_alloc_manage")]
    ManageJudgeAlloc,
    /// Permission to change room allocations (i.e. which debate will take place
    #[serde(rename = "room_alloc_manage")]
    ManageRoomAlloc,
    #[serde(rename = "draw_view")]
    ViewDraw,
}

#[derive(Debug, Clone)]
pub struct TournamentAccess {
    tournament_id: String,
    superuser: bool,
    grants: Vec<Permission>,
}
impl TournamentAccess {
    pub fn resolve(
        tournament_id: &str,
        user_id: Option<&str>,
        conn: &mut impl diesel::connection::LoadConnection<
            Backend = diesel::sqlite::Sqlite,
        >,
    ) -> Result<Self, crate::util_resp::FailureResponse> {
        use crate::schema::{
            groups, members_of_group, org, permissions_of_group,
        };
        use diesel::prelude::*;
        let Some(user_id) = user_id else {
            return Ok(Self {
                tournament_id: tournament_id.into(),
                superuser: false,
                grants: vec![],
            });
        };
        let superuser = diesel::select(diesel::dsl::exists(
            org::table
                .filter(org::user_id.eq(user_id))
                .filter(org::tournament_id.eq(tournament_id))
                .filter(org::is_superuser.eq(true)),
        ))
        .get_result::<bool>(conn)?;
        let serialized = org::table
            .filter(org::user_id.eq(user_id))
            .filter(org::tournament_id.eq(tournament_id))
            .inner_join(
                members_of_group::table
                    .on(members_of_group::member_id.eq(org::id)),
            )
            .inner_join(
                groups::table.on(groups::id.eq(members_of_group::group_id)),
            )
            .inner_join(
                permissions_of_group::table
                    .on(permissions_of_group::group_id.eq(groups::id)),
            )
            .filter(groups::tournament_id.eq(tournament_id))
            .filter(permissions_of_group::tournament_id.eq(tournament_id))
            .select(permissions_of_group::permission)
            .load::<String>(conn)?;
        Ok(Self {
            tournament_id: tournament_id.into(),
            superuser,
            grants: serialized
                .iter()
                .filter_map(|value| serde_json::from_str(value).ok())
                .collect(),
        })
    }
    pub fn tournament_id(&self) -> &str {
        &self.tournament_id
    }
    pub fn is_superuser(&self) -> bool {
        self.superuser
    }
    pub fn allows(&self, permission: Permission) -> bool {
        self.is_superuser()
            || self.grants.iter().any(|grant| {
                *grant == permission
                    || permission == Permission::ViewDraw
                        && matches!(
                            grant,
                            Permission::ManageJudgeAlloc
                                | Permission::ManageRoomAlloc
                        )
            })
    }
}
