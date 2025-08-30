use crate::schema::tournament_ballots;
use crate::schema::tournament_debate_teams;
use crate::schema::tournament_debates;
use crate::tournaments::rounds::ballots::BallotRepr;

use chrono::NaiveDateTime;
use diesel::connection::LoadConnection;
use diesel::prelude::*;
use diesel::sqlite::Sqlite;
use diesel::{QueryableByName, prelude::Queryable};
use serde::{Deserialize, Serialize};

pub mod manage;

#[derive(Queryable, Serialize, Deserialize)]
pub struct Draw {
    id: String,
    tournament_id: String,
    round_id: String,
    status: String,
    released_at: Option<NaiveDateTime>,
}

impl Draw {
    pub fn status(&self) -> DrawStatus {
        match self.status.as_str() {
            "D" => DrawStatus::Draft,
            "C" => DrawStatus::Confirmed,
            "R" => DrawStatus::Released,
            _ => unreachable!(),
        }
    }
}

pub struct DrawRepr {
    pub draw: Draw,
    pub debates: Vec<DebateRepr>,
}

impl DrawRepr {
    pub fn of_draw(
        draw: Draw,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) -> Self {
        let id = draw.id.clone();
        DrawRepr {
            draw,
            debates: {
                tournament_debates::table
                    .filter(tournament_debates::draw_id.eq(id))
                    // todo: assign integer ID to debates?
                    .order_by(tournament_debates::id.asc())
                    .select(tournament_debates::id)
                    .load::<String>(conn)
                    .unwrap()
                    .into_iter()
                    .map(|id| DebateRepr::fetch(&id, conn))
                    .collect()
            },
        }
    }
}

pub struct DebateRepr {
    pub debate: Debate,
    pub teams: Vec<DebateTeam>,
}

impl DebateRepr {
    pub fn fetch(
        id: &str,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) -> Self {
        let debate = tournament_debates::table
            .filter(tournament_debates::id.eq(&id))
            .first::<Debate>(conn)
            .unwrap();

        let teams = tournament_debate_teams::table
            .filter(tournament_debate_teams::debate_id.eq(&debate.id))
            // e.g.
            // OG (seq=0, side=0)
            // OO (seq=0, side=1)
            // CG (seq=1, side=0)
            // CO (seq=1, side=1)
            .order_by((
                tournament_debate_teams::seq.asc(),
                tournament_debate_teams::side.asc(),
            ))
            .load::<DebateTeam>(conn)
            .unwrap();

        Self { debate, teams }
    }

    /// Retrieve all the ballots that have been submitted for this debate.
    pub fn ballots(
        &self,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) -> Vec<BallotRepr> {
        tournament_ballots::table
            .filter(tournament_ballots::debate_id.eq(&self.debate.id))
            .select(tournament_ballots::id)
            .load::<String>(conn)
            .unwrap()
            .into_iter()
            .map(|id| BallotRepr::fetch(&id, conn))
            .collect()
    }
}

pub enum DrawStatus {
    Draft,
    Confirmed,
    Released,
}

#[derive(QueryableByName, Queryable)]
#[diesel(table_name = tournament_debates)]
pub struct Debate {
    id: String,
    pub tournament_id: String,
    pub draw_id: String,
    pub room_id: Option<String>,
}

#[derive(QueryableByName, Queryable)]
#[diesel(table_name = tournament_debate_teams)]
/// This struct represents a single row in the `tournament_debate_teams` table.
pub struct DebateTeam {
    pub id: String,
    pub debate_id: String,
    pub team_id: String,
    pub side: i64,
    pub seq: i64,
}
