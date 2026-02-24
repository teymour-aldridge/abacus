use std::collections::HashMap;

use super::Round;
use crate::schema::ballots;
use crate::schema::debates;
use crate::schema::judges;
use crate::schema::judges_of_debate;
use crate::schema::motions_of_round;
use crate::schema::rooms as schema_rooms;
use crate::schema::speakers;
use crate::schema::speakers_of_team;
use crate::schema::teams;
use crate::schema::teams_of_debate;
use crate::tournaments::participants::DebateJudge;
use crate::tournaments::participants::Judge;
use crate::tournaments::participants::Speaker;
use crate::tournaments::rounds::Motion;
use crate::tournaments::rounds::ballots::BallotRepr;
use crate::tournaments::teams::Team;

use diesel::connection::LoadConnection;
use diesel::prelude::*;
use diesel::sql_types::BigInt;
use diesel::sqlite::Sqlite;
use diesel::{QueryableByName, prelude::Queryable};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

pub mod manage;
pub mod public;
pub mod rooms;

#[derive(Queryable, Serialize, Deserialize, Debug, Clone)]
pub struct Room {
    id: String,
    tournament_id: String,
    pub name: String,
    pub url: Option<String>,
    priority: i64,
    number: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct RoundDrawRepr {
    pub round: Round,
    pub debates: Vec<DebateRepr>,
}

impl RoundDrawRepr {
    pub fn of_round(
        round: Round,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Self {
        let id = round.id.clone();
        RoundDrawRepr {
            round,
            debates: {
                debates::table
                    .filter(debates::round_id.eq(id))
                    // todo: assign integer ID to debates?
                    .order_by(debates::id.asc())
                    .select(debates::id)
                    .load::<String>(conn)
                    .unwrap()
                    .into_iter()
                    .map(|id| DebateRepr::fetch(&id, conn))
                    .collect()
            },
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct DebateRepr {
    pub debate: Debate,
    pub room: Option<Room>,
    pub teams_of_debate: Vec<DebateTeam>,
    // todo: teams and speakers should be placed in a separate struct (we can
    // also load the private URLs as part of this struct)
    pub teams: HashMap<String, Team>,
    pub speakers_of_team: HashMap<String, Vec<Speaker>>,
    pub judges_of_debate: Vec<DebateJudge>,
    pub judges: HashMap<String, Judge>,
    pub motions: HashMap<String, Motion>,
}

impl DebateRepr {
    #[tracing::instrument(skip(conn))]
    pub fn fetch(
        id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Self {
        Self::try_fetch(id, conn).unwrap()
    }

    #[tracing::instrument(skip(conn))]
    // todo: this method executes far too many database queries and its
    // efficiency can thus be improved
    pub fn try_fetch(
        id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<Self, diesel::result::Error> {
        let debate = debates::table
            .filter(debates::id.eq(&id))
            .first::<Debate>(conn)?;

        let room = match &debate.room_id {
            Some(room_id) => Some(
                schema_rooms::table
                    .filter(schema_rooms::id.eq(room_id))
                    .first::<Room>(conn)?,
            ),
            None => None,
        };

        let debate_teams: Vec<DebateTeam> = teams_of_debate::table
            .filter(teams_of_debate::debate_id.eq(&debate.id))
            // e.g.
            // OG (seq=0, side=0)
            // OO (seq=0, side=1)
            // CG (seq=1, side=0)
            // CO (seq=1, side=1)
            .order_by((
                teams_of_debate::debate_id,
                2.into_sql::<BigInt>() * teams_of_debate::seq
                    + teams_of_debate::side,
            ))
            .load::<DebateTeam>(conn)?;

        let teams = teams::table
            .filter(teams::tournament_id.eq(&debate.tournament_id))
            .filter(
                teams::id.eq_any(
                    debate_teams
                        .iter()
                        .map(|dt| dt.team_id.clone())
                        .collect_vec(),
                ),
            )
            .load::<Team>(conn)?;

        let speakers_of_team = speakers_of_team::table
            .filter(
                speakers_of_team::team_id.eq_any(
                    teams.iter().map(|team| team.id.clone()).collect_vec(),
                ),
            )
            .select((speakers_of_team::team_id, speakers_of_team::speaker_id))
            .load::<(String, String)>(&mut *conn)?;

        let speakers = speakers::table
            .filter(
                speakers::id.eq_any(
                    speakers_of_team
                        .iter()
                        .map(|(_, id)| id.clone())
                        .collect_vec(),
                ),
            )
            .load::<Speaker>(&mut *conn)?;

        let judges_of_debate = judges_of_debate::table
            .filter(judges_of_debate::debate_id.eq(&debate.id))
            .order_by((
                diesel::dsl::case_when(
                    judges_of_debate::status.eq("C"),
                    1.into_sql::<diesel::sql_types::BigInt>(),
                )
                .otherwise(2.into_sql::<diesel::sql_types::BigInt>()),
                {
                    let name = judges::table
                        .filter(judges::id.eq(judges_of_debate::judge_id))
                        .select(judges::name)
                        .single_value()
                        .assume_not_null();
                    name.asc()
                },
            ))
            .load::<DebateJudge>(conn)?;

        let judges = judges::table
            .filter(judges::tournament_id.eq(&debate.tournament_id))
            .filter(
                judges::id.eq_any(
                    judges_of_debate
                        .iter()
                        .map(|debate_judge| debate_judge.judge_id.clone())
                        .collect_vec(),
                ),
            )
            .load::<Judge>(conn)?
            .into_iter()
            .map(|judge| (judge.id.clone(), judge))
            .collect();

        let motions: HashMap<_, _> = motions_of_round::table
            .filter(motions_of_round::round_id.eq(debate.round_id.clone()))
            .load::<Motion>(&mut *conn)
            .unwrap()
            .into_iter()
            .map(|t| (t.id.clone(), t))
            .collect();

        Ok(Self {
            debate,
            room,
            teams_of_debate: debate_teams,
            teams: teams
                .into_iter()
                .map(|team| (team.id.clone(), team))
                .collect(),
            speakers_of_team: {
                let mut map = HashMap::new();
                for (team_id, speaker_id) in speakers_of_team {
                    let speaker =
                        speakers.iter().find(|s| s.id == speaker_id).unwrap();
                    map.entry(team_id)
                        .and_modify(|speakers: &mut Vec<Speaker>| {
                            speakers.push(speaker.clone())
                        })
                        .or_insert(vec![speaker.clone()]);
                }
                map
            },
            judges_of_debate,
            judges,
            motions,
        })
    }

    /// Retrieve the most recent ballot submitted by each judge for this debate.
    // TODO: select latest using SQL rather than ad-hoc Rust impl of what should be
    //       query logic
    #[tracing::instrument(skip(conn))]
    pub fn latest_ballots(
        &self,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Vec<BallotRepr> {
        let all_ballots = self.ballot_history(conn);

        let mut latest = HashMap::new();
        for ballot in all_ballots {
            let judge_id = &ballot.metadata.judge_id;
            // Since they are ordered by submitted_at desc, the first one we see is the latest
            if !latest.contains_key(judge_id) {
                latest.insert(judge_id.clone(), ballot);
            }
        }
        latest.into_values().collect()
    }

    /// Retrieve all the ballots that have been submitted for this debate, including history.
    /// Ordered by submitted_at descending (newest first).
    #[tracing::instrument(skip(conn))]
    pub fn ballot_history(
        &self,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Vec<BallotRepr> {
        ballots::table
            .filter(ballots::debate_id.eq(&self.debate.id))
            .select(ballots::id)
            .order_by(ballots::submitted_at.desc())
            .load::<String>(conn)
            .unwrap()
            .into_iter()
            .map(|id| BallotRepr::fetch(&id, conn))
            .collect()
    }

    pub fn team_of_side_and_seq(&self, side: i64, seq: i64) -> &DebateTeam {
        self.teams_of_debate
            .iter()
            .find(|team| team.side == side && team.seq == seq)
            .unwrap()
    }
}

pub enum DrawStatus {
    Draft,
    Confirmed,
    Released,
}

#[derive(QueryableByName, Queryable, Debug, Clone, Serialize)]
#[diesel(table_name = debates)]
pub struct Debate {
    pub id: String,
    pub tournament_id: String,
    pub round_id: String,
    pub room_id: Option<String>,
    pub number: i64,
    pub status: String,
}

#[derive(QueryableByName, Queryable, Debug, Clone, Serialize)]
#[diesel(table_name = teams_of_debate)]
/// This struct represents a single row in the `teams_of_debate` table.
pub struct DebateTeam {
    pub id: String,
    pub tournament_id: String,
    pub debate_id: String,
    pub team_id: String,
    pub side: i64,
    pub seq: i64,
}
