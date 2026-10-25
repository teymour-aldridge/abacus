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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Ca,
    Equity,
    Tab,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ca => "ca",
            Self::Equity => "equity",
            Self::Tab => "tab",
        }
    }

    fn permissions(self) -> &'static [Permission] {
        match self {
            Self::Ca => {
                &[Permission::ManageJudgeAlloc, Permission::ViewConflicts]
            }
            Self::Equity => &[
                Permission::ManageParticipantConflicts,
                Permission::ViewConflicts,
            ],
            Self::Tab => &[],
        }
    }
}
impl TournamentAccess {
    pub fn resolve(
        tournament_id: &str,
        user_id: Option<&str>,
        conn: &mut impl diesel::connection::LoadConnection<
            Backend = diesel::sqlite::Sqlite,
        >,
    ) -> Result<Self, crate::util_resp::FailureResponse> {
        use crate::schema::tournament_membership;
        use diesel::prelude::*;
        let Some(user_id) = user_id else {
            return Ok(Self {
                tournament_id: tournament_id.into(),
                superuser: false,
                grants: vec![],
            });
        };
        let stored_roles = tournament_membership::table
            .filter(tournament_membership::user_id.eq(user_id))
            .filter(tournament_membership::tournament_id.eq(tournament_id))
            .select(tournament_membership::role)
            .load::<String>(conn)?;
        let roles = stored_roles
            .iter()
            .map(|role| match role.as_str() {
                "ca" => Ok(Role::Ca),
                "equity" => Ok(Role::Equity),
                "tab" => Ok(Role::Tab),
                _ => Err(crate::util_resp::FailureResponse::ServerError(())),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            tournament_id: tournament_id.into(),
            superuser: roles.contains(&Role::Tab),
            grants: roles
                .iter()
                .flat_map(|role| role.permissions().iter().copied())
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
