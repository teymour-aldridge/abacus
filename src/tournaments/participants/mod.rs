use std::collections::HashSet;

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    schema::{
        institutions, judges, judges_of_debate, speakers, speakers_of_team,
        teams,
    },
    tournaments::teams::Team,
    util_resp::{FailureResponse, err_not_found},
};

pub mod manage;
pub mod public;

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
        let judge = judges::table
            .filter(
                judges::tournament_id
                    .eq(&tournament_id)
                    .and(judges::private_url.eq(&private_url)),
            )
            .first::<Judge>(conn)
            .optional()
            .unwrap();

        if let Some(judge) = judge {
            return Ok(Self::Judge(judge));
        }

        let speaker = speakers::table
            .filter(
                speakers::tournament_id
                    .eq(&tournament_id)
                    .and(speakers::private_url.eq(&private_url)),
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
#[diesel(table_name = judges)]
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

impl Judge {
    pub fn of_private_url(
        private_url: &str,
        tournament_id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<Judge, FailureResponse> {
        judges::table
            .filter(
                judges::private_url
                    .eq(&private_url)
                    .and(judges::tournament_id.eq(&tournament_id)),
            )
            .first::<Judge>(&mut *conn)
            .optional()
            .unwrap()
            .ok_or(err_not_found().unwrap_err())
    }
}

#[derive(Queryable, QueryableByName, Serialize, Deserialize, Clone, Debug)]
#[diesel(check_for_backend(Sqlite))]
#[diesel(table_name = judges_of_debate)]
pub struct DebateJudge {
    pub id: String,
    pub tournament_id: String,
    pub debate_id: String,
    pub judge_id: String,
    pub status: String,
}

#[derive(Queryable, Serialize, Deserialize, Clone, Debug)]
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

#[derive(Debug, Clone)]
pub struct TournamentParticipants {
    pub teams: IndexMap<String, Team>,
    pub speakers: IndexMap<String, Speaker>,
    pub judges: IndexMap<String, Judge>,
    pub team_speakers: IndexMap<String, HashSet<String>>,
    pub institutions: IndexMap<String, Institution>,
}

impl TournamentParticipants {
    // TODO: how expensive is this method to call? Should profile and if
    // necessary make changes.
    #[tracing::instrument(skip(conn))]
    pub fn load(
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> TournamentParticipants {
        let teams: IndexMap<String, Team> = teams::table
            .filter(teams::tournament_id.eq(&tid))
            .order_by(teams::number.asc())
            .load::<Team>(conn)
            .unwrap()
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();

        let speakers = speakers::table
            .filter(speakers::tournament_id.eq(&tid))
            .load::<Speaker>(conn)
            .unwrap()
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();

        let team_speakers = {
            let list = speakers_of_team::table
                // .inner_join(teams::table)
                .filter(diesel::dsl::exists(
                    speakers::table.filter(speakers::tournament_id.eq(&tid)),
                ))
                .select((
                    speakers_of_team::team_id,
                    speakers_of_team::speaker_id,
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

        let judges = judges::table
            .filter(judges::tournament_id.eq(&tid))
            .order_by(judges::name.asc())
            .load::<Judge>(conn)
            .unwrap()
            .into_iter()
            .map(|j| (j.id.clone(), j))
            .collect();

        let institutions = institutions::table
            .filter(institutions::tournament_id.eq(&tid))
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

    pub fn canonical_name_of_team(&self, team: &Team) -> String {
        team.institution_id
            .as_ref()
            .map(|inst| {
                let code = &self.institutions.get(inst).unwrap().code;
                format!("{code} {}", team.name)
            })
            .unwrap_or_else(|| team.name.clone())
    }
}
