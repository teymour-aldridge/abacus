use serde::{Deserialize, Serialize};

use crate::tournaments::rounds::draws::DebateRepr;

#[derive(Serialize, Deserialize)]
/// Ballot form used for judges to submit ballots, and for tab directors to
/// edit them behind the scenes.
///
/// Our HTML form logic requires that this form be parsed with [`serde_qs`]
/// rather than the standard axum (or axum_extra) extractors.
pub struct BallotForm {
    #[serde(default)]
    pub teams: Vec<BallotFormSingleTeamEntry>,
    pub motion_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct BallotFormSingleSpeakerEntry {
    pub id: String,
    pub score: Option<f32>,
}

#[derive(Serialize, Deserialize)]
pub struct BallotFormSingleTeamEntry {
    pub speakers: Vec<BallotFormSingleSpeakerEntry>,
    pub points: Option<usize>,
}

impl BallotForm {
    pub fn all_advancing_team_ids(&self, repr: &DebateRepr) -> Vec<String> {
        self.teams
            .iter()
            .enumerate()
            .filter(|(_, team)| {
                team.points.expect(
                    "should have validated that all teams[i].points.is_some() by now"
                ) == 1
            })
            .map(|(i, _)| repr.teams_of_debate[i].id.clone())
            .collect()
    }
}
