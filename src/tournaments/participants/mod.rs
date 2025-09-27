use std::collections::HashSet;

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{
    schema::{
        tournament_debate_judges, tournament_institutions, tournament_judges,
        tournament_participants, tournament_speakers, tournament_team_speakers,
        tournament_teams,
    },
    tournaments::teams::Team,
};

pub mod manage;

#[derive(Queryable, Serialize, Deserialize, Clone, Debug, Hash)]
pub struct Speaker {
    pub id: String,
    pub tournament_id: String,
    pub name: String,
    pub email: String,
    pub participant_id: String,
}

#[derive(Queryable, QueryableByName, Serialize, Deserialize, Clone)]
#[diesel(check_for_backend(Sqlite))]
#[diesel(table_name = tournament_judges)]
pub struct Judge {
    pub id: String,
    pub tournament_id: String,
    pub name: String,
    pub institution_id: Option<String>,
    pub participant_id: String,
    pub number: i64,
}

#[derive(Queryable, QueryableByName, Serialize, Deserialize, Clone)]
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
    pub team_speakers: IndexMap<String, HashSet<String>>,
    pub institutions: IndexMap<String, Institution>,
    pub base_participants: IndexMap<String, BaseParticipant>,
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
                .inner_join(tournament_teams::table)
                .filter(tournament_teams::tournament_id.eq(&tid))
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

        let institutions = tournament_institutions::table
            .filter(tournament_institutions::tournament_id.eq(&tid))
            .load::<Institution>(conn)
            .unwrap()
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();

        let base_participants = tournament_participants::table
            .filter(tournament_participants::tournament_id.eq(&tid))
            .load::<BaseParticipant>(conn)
            .unwrap()
            .into_iter()
            .map(|record| (record.id.clone(), record))
            .collect();

        Self {
            teams,
            speakers,
            team_speakers,
            institutions,
            base_participants,
        }
    }
}
