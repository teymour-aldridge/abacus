use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use serde::{Deserialize, Serialize};

use crate::{schema::tournament_teams, util_resp::FailureResponse};

#[derive(Serialize, Deserialize, Queryable, Clone, Debug)]
pub struct Team {
    pub id: String,
    pub tournament_id: String,
    pub name: String,
    pub institution_id: Option<String>,
    pub number: i64,
}

impl Team {
    #[tracing::instrument(skip(conn))]
    pub fn fetch(
        team_id: &str,
        tournament_id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<Team, FailureResponse> {
        let ret = tournament_teams::table
            .filter(
                tournament_teams::id
                    .eq(team_id)
                    .and(tournament_teams::tournament_id.eq(tournament_id)),
            )
            .first::<Team>(&mut *conn)
            .optional()
            .unwrap()
            .ok_or(FailureResponse::NotFound(()));

        tracing::trace!("ok? {}", ret.is_ok());

        ret
    }
}
