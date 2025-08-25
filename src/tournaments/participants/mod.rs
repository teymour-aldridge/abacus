use std::collections::{HashMap, HashSet};

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use serde::{Deserialize, Serialize};

use crate::{
    schema::{
        tournament_institutions, tournament_participants, tournament_speakers,
        tournament_team_speakers, tournament_teams,
    },
    tournaments::teams::Team,
};

pub mod manage;

#[derive(Queryable, Serialize, Deserialize, Clone)]
pub struct Speaker {
    id: String,
    tournament_id: String,
    name: String,
    email: String,
    participant_id: String,
}

#[derive(Queryable, Serialize, Deserialize, Clone)]
pub struct Judge {
    id: String,
    tournament_id: String,
    name: String,
    institution_id: String,
    participant_id: String,
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
    teams: HashMap<String, Team>,
    speakers: HashMap<String, Speaker>,
    team_speakers: HashMap<String, HashSet<String>>,
    institutions: HashMap<String, Institution>,
    base_participants: HashMap<String, BaseParticipant>,
}

#[derive(Serialize)]
pub struct STeam {
    id: String,
    name: String,
    /// Institution name
    inst: Option<String>,
}

#[derive(Serialize)]
pub struct SSpeaker {
    id: String,
    name: String,
    email: String,
    private_url: String,
}

#[derive(Serialize)]
pub struct NestedDataItem {
    #[serde(flatten)]
    team: STeam,
    speakers: Vec<SSpeaker>,
}

impl TournamentParticipants {
    fn for_tabulator(self) -> Vec<NestedDataItem> {
        self.teams
            .into_iter()
            .map(|(_id, team)| NestedDataItem {
                team: STeam {
                    id: team.id.clone(),
                    name: team.name,
                    inst: {
                        team.institution_id.map(|id| {
                            self.institutions
                                .get(&id)
                                .expect("failed to create participant")
                                .name
                                .clone()
                        })
                    },
                },
                speakers: {
                    // note: the map from team to speakers might not be created if
                    // there are no speakers, but we might want to always create
                    // this and panic if it doesn't exist
                    let empty = HashSet::new();
                    let speakers = self.team_speakers.get(&team.id).unwrap_or(&empty);
                    speakers
                        .into_iter()
                        .map(|speaker| {
                            let speaker = self.speakers.get(speaker).unwrap().clone();
                            SSpeaker {
                                id: speaker.id,
                                name: speaker.name,
                                email: speaker.email,
                                private_url: self
                                    .base_participants
                                    .get(&speaker.participant_id)
                                    .unwrap()
                                    .private_url
                                    .clone(),
                            }
                        })
                        .collect()
                },
            })
            .collect::<Vec<_>>()
    }

    pub fn load(
        tid: &str,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) -> TournamentParticipants {
        let teams: HashMap<String, Team> = tournament_teams::table
            .filter(tournament_teams::tournament_id.eq(&tid))
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

            let mut teams = HashMap::with_capacity(teams.len());

            for (team, speaker) in list {
                teams
                    .entry(team.clone())
                    .and_modify(|s: &mut HashSet<String>| {
                        s.insert(speaker);
                    })
                    .or_insert({
                        let mut set = HashSet::new();
                        set.insert(team);
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
