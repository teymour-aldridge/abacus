use std::cmp::Reverse;

use chrono::NaiveDateTime;
use diesel::{
    Queryable, connection::LoadConnection, prelude::*, sqlite::Sqlite,
};
use itertools::Itertools;
use rust_decimal::Decimal;

use crate::schema::{tournament_ballots, tournament_speaker_score_entries};

pub mod aggregate;
pub mod manage;
pub mod public;

pub struct BallotRepr {
    ballot: Ballot,
    scores: Vec<BallotScore>,
}

impl BallotRepr {
    pub fn is_isomorphic(&self, other: &BallotRepr) -> bool {
        for score in &self.scores {
            let found_matching = other.scores.iter().any(|other_score| {
                other_score.speaker_position == score.speaker_position
                    && other_score.speaker_id == score.speaker_id
                    && (other_score.score - score.score).max(0.0)
                        <= f32::EPSILON
            });
            if !found_matching {
                return false;
            }
        }
        true
    }

    pub fn ballot(&self) -> &Ballot {
        &self.ballot
    }

    pub fn fetch(
        ballot_id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Self {
        let ballot = tournament_ballots::table
            .filter(tournament_ballots::id.eq(ballot_id))
            .first::<Ballot>(conn)
            .unwrap();

        let scores = tournament_speaker_score_entries::table
            .filter(tournament_speaker_score_entries::ballot_id.eq(&ballot.id))
            .load::<BallotScore>(conn)
            .unwrap();

        Self { ballot, scores }
    }

    /// Count the number of teams on the ballot.
    ///
    /// This is useful as a simple check to ensure that the configuration of
    /// the tournament is not incompatible with the data currently entered (we
    /// also check to ensure that people cannot set an invalid format).
    pub fn team_count(&self) -> usize {
        self.teams().count()
    }

    pub fn teams(&self) -> impl Iterator<Item = String> {
        self.scores
            .iter()
            .unique_by(|s| s.team_id.clone())
            .map(|s| s.team_id.clone())
    }

    /// Returns the IDs of the teams, in the order in which they came in the
    /// debate.
    pub fn teams_in_rank_order(&self) -> impl Iterator<Item = String> {
        self.teams().sorted_by_key(|team| {
            let total: Decimal = self
                .scores_of_team(team)
                .iter()
                .map(|score| -> Decimal { score.score.try_into().unwrap() })
                .sum();

            // sort descending by score (not ascending)
            Reverse(total)
        })
    }

    /// Retrieves the score elements of a particular team.
    pub fn scores_of_team(&self, team_id: &str) -> Vec<BallotScore> {
        self.scores
            .iter()
            .filter(|s| s.team_id == team_id)
            .sorted_by_key(|s| s.speaker_position)
            .cloned()
            .collect()
    }
}

#[derive(Queryable)]
pub struct Ballot {
    pub id: String,
    pub tournament_id: String,
    pub debate_id: String,
    pub judge_id: String,
    pub submitted_at: NaiveDateTime,
    pub motion_id: String,

    pub version: i64,
    pub change: Option<String>,
    pub editor_id: Option<String>,
}

#[derive(Queryable, QueryableByName, Clone, Debug)]
#[diesel(check_for_backend(Sqlite))]
#[diesel(table_name = tournament_speaker_score_entries)]
pub struct BallotScore {
    pub id: String,
    pub ballot_id: String,
    pub team_id: String,
    pub speaker_id: String,
    pub speaker_position: i64,
    pub score: f32,
}
