use std::collections::HashMap;

use super::Round;
use crate::schema::tournament_ballots;
use crate::schema::tournament_debate_judges;
use crate::schema::tournament_debate_teams;
use crate::schema::tournament_debates;
use crate::schema::tournament_judges;
use crate::schema::tournament_rooms;
use crate::schema::tournament_round_motions;
use crate::schema::tournament_speakers;
use crate::schema::tournament_team_speakers;
use crate::schema::tournament_teams;
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
                tournament_debates::table
                    .filter(tournament_debates::round_id.eq(id))
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
        let debate = tournament_debates::table
            .filter(tournament_debates::id.eq(&id))
            .first::<Debate>(conn)?;

        let room = match &debate.room_id {
            Some(room_id) => Some(
                tournament_rooms::table
                    .filter(tournament_rooms::id.eq(room_id))
                    .first::<Room>(conn)?,
            ),
            None => None,
        };

        let debate_teams: Vec<DebateTeam> = tournament_debate_teams::table
            .filter(tournament_debate_teams::debate_id.eq(&debate.id))
            // e.g.
            // OG (seq=0, side=0)
            // OO (seq=0, side=1)
            // CG (seq=1, side=0)
            // CO (seq=1, side=1)
            .order_by((
                tournament_debate_teams::debate_id,
                2.into_sql::<BigInt>() * tournament_debate_teams::seq
                    + tournament_debate_teams::side,
            ))
            .load::<DebateTeam>(conn)?;

        let teams = tournament_teams::table
            .filter(tournament_teams::tournament_id.eq(&debate.tournament_id))
            .filter(
                tournament_teams::id.eq_any(
                    debate_teams
                        .iter()
                        .map(|dt| dt.team_id.clone())
                        .collect_vec(),
                ),
            )
            .load::<Team>(conn)?;

        let tournament_team_speakers =
            tournament_team_speakers::table
                .filter(tournament_team_speakers::team_id.eq_any(
                    teams.iter().map(|team| team.id.clone()).collect_vec(),
                ))
                .select((
                    tournament_team_speakers::team_id,
                    tournament_team_speakers::speaker_id,
                ))
                .load::<(String, String)>(&mut *conn)?;

        let tournament_speakers = tournament_speakers::table
            .filter(
                tournament_speakers::id.eq_any(
                    tournament_team_speakers
                        .iter()
                        .map(|(_, id)| id.clone())
                        .collect_vec(),
                ),
            )
            .load::<Speaker>(&mut *conn)?;

        let judges_of_debate = tournament_debate_judges::table
            .filter(tournament_debate_judges::debate_id.eq(&debate.id))
            .order_by((
                diesel::dsl::case_when(
                    tournament_debate_judges::status.eq("C"),
                    1.into_sql::<diesel::sql_types::BigInt>(),
                )
                .otherwise(2.into_sql::<diesel::sql_types::BigInt>()),
                {
                    let name = tournament_judges::table
                        .filter(
                            tournament_judges::id
                                .eq(tournament_debate_judges::judge_id),
                        )
                        .select(tournament_judges::name)
                        .single_value()
                        .assume_not_null();
                    name.asc()
                },
            ))
            .load::<DebateJudge>(conn)?;

        let judges = tournament_judges::table
            .filter(tournament_judges::tournament_id.eq(&debate.tournament_id))
            .filter(
                tournament_judges::id.eq_any(
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

        let motions: HashMap<_, _> = tournament_round_motions::table
            .filter(
                tournament_round_motions::round_id.eq(debate.round_id.clone()),
            )
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
                for (team_id, speaker_id) in tournament_team_speakers {
                    let speaker = tournament_speakers
                        .iter()
                        .find(|s| s.id == speaker_id)
                        .unwrap();
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

    /// Retrieve all the ballots that have been submitted for this debate.
    #[tracing::instrument(skip(conn))]
    pub fn ballots(
        &self,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Vec<BallotRepr> {
        tournament_ballots::table
            .filter(tournament_ballots::debate_id.eq(&self.debate.id))
            .select(tournament_ballots::id)
            .order_by(tournament_ballots::submitted_at.desc())
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
#[diesel(table_name = tournament_debates)]
pub struct Debate {
    pub id: String,
    pub tournament_id: String,
    pub round_id: String,
    pub room_id: Option<String>,
    pub number: i64,
    pub status: String,
}

#[derive(QueryableByName, Queryable, Debug, Clone, Serialize)]
#[diesel(table_name = tournament_debate_teams)]
/// This struct represents a single row in the `tournament_debate_teams` table.
pub struct DebateTeam {
    pub id: String,
    pub tournament_id: String,
    pub debate_id: String,
    pub team_id: String,
    pub side: i64,
    pub seq: i64,
}
