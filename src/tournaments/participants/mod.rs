use std::collections::HashSet;

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    schema::{
        tournament_debate_judges, tournament_institutions, tournament_judges,
        tournament_speakers, tournament_team_speakers, tournament_teams,
    },
    tournaments::teams::Team,
    util_resp::FailureResponse,
};

pub mod manage;

pub enum Participant {
    Speaker(Speaker),
    Judge(Judge),
}

impl Participant {
    pub fn of_private_url_and_tournament(
        tournament_id: &str,
        private_url: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<Self, FailureResponse> {
        let judge = tournament_judges::table
            .filter(
                tournament_judges::tournament_id
                    .eq(&tournament_id)
                    .and(tournament_judges::private_url.eq(&private_url)),
            )
            .first::<Judge>(conn)
            .optional()
            .unwrap();

        if let Some(judge) = judge {
            return Ok(Self::Judge(judge));
        }

        let speaker = tournament_speakers::table
            .filter(
                tournament_speakers::tournament_id
                    .eq(&tournament_id)
                    .and(tournament_speakers::private_url.eq(&private_url)),
            )
            .first::<Speaker>(conn)
            .optional()
            .unwrap();

        if let Some(speaker) = speaker {
            return Ok(Self::Speaker(speaker));
        } else {
            return Err(FailureResponse::NotFound(()));
        }
    }
}

#[derive(Queryable, Serialize, Deserialize, Clone, Debug, Hash)]
pub struct Speaker {
    pub id: String,
    pub tournament_id: String,
    // todo: this should be optional
    pub name: String,
    pub email: String,
    pub private_url: String,
}

#[derive(Queryable, QueryableByName, Serialize, Deserialize, Clone, Debug)]
#[diesel(check_for_backend(Sqlite))]
#[diesel(table_name = tournament_judges)]
pub struct Judge {
    pub id: String,
    pub tournament_id: String,
    pub name: String,
    // todo: this should be optional
    pub email: String,
    pub institution_id: Option<String>,
    pub private_url: String,
    pub number: i64,
}

#[derive(Queryable, QueryableByName, Serialize, Deserialize, Clone, Debug)]
#[diesel(check_for_backend(Sqlite))]
#[diesel(table_name = tournament_debate_judges)]
pub struct DebateJudge {
    pub debate_id: String,
    pub judge_id: String,
    pub status: String,
}

#[derive(Queryable, Serialize, Deserialize, Clone)]
pub struct Institution {
    id: String,
    tournament_id: String,
    name: String,
    code: String,
}

#[derive(Queryable, Serialize, Deserialize, Clone)]
pub struct BaseParticipant {
    id: String,
    tournament_id: String,
    private_url: String,
}

pub struct TournamentParticipants {
    pub teams: IndexMap<String, Team>,
    pub speakers: IndexMap<String, Speaker>,
    pub judges: IndexMap<String, Judge>,
    pub team_speakers: IndexMap<String, HashSet<String>>,
    pub institutions: IndexMap<String, Institution>,
}

impl TournamentParticipants {
    pub fn load(
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> TournamentParticipants {
        let teams: IndexMap<String, Team> = tournament_teams::table
            .filter(tournament_teams::tournament_id.eq(&tid))
            .order_by(tournament_teams::number.asc())
            .load::<Team>(conn)
            .unwrap()
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();

        let speakers = tournament_speakers::table
            .filter(tournament_speakers::tournament_id.eq(&tid))
            .load::<Speaker>(conn)
            .unwrap()
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();

        let team_speakers = {
            let list = tournament_team_speakers::table
                // .inner_join(tournament_teams::table)
                .filter(diesel::dsl::exists(
                    tournament_speakers::table
                        .filter(tournament_speakers::tournament_id.eq(&tid)),
                ))
                .select((
                    tournament_team_speakers::team_id,
                    tournament_team_speakers::speaker_id,
                ))
                .load::<(String, String)>(conn)
                .unwrap();

            let mut teams = IndexMap::with_capacity(teams.len());

            for (team, speaker) in list {
                teams
                    .entry(team.clone())
                    .and_modify(|s: &mut HashSet<String>| {
                        s.insert(speaker.clone());
                    })
                    .or_insert({
                        let mut set = HashSet::new();
                        set.insert(speaker);
                        set
                    });
            }

            teams
        };

        let judges = tournament_judges::table
            .filter(tournament_judges::tournament_id.eq(&tid))
            .order_by(tournament_judges::name.asc())
            .load::<Judge>(conn)
            .unwrap()
            .into_iter()
            .map(|j| (j.id.clone(), j))
            .collect();

        let institutions = tournament_institutions::table
            .filter(tournament_institutions::tournament_id.eq(&tid))
            .load::<Institution>(conn)
            .unwrap()
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();

        Self {
            teams,
            speakers,
            team_speakers,
            judges,
            institutions,
        }
    }
}
