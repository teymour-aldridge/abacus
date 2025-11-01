use std::collections::HashMap;

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use itertools::Itertools;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::{
    schema::{
        tournament_debate_speaker_results, tournament_debate_team_results,
    },
    tournaments::rounds::ballots::BallotRepr,
};

#[derive(Queryable)]
pub struct TournamentDebateSpeakerResult {
    pub id: String,
    pub debate_id: String,
    pub speaker_id: String,
    pub team_id: String,
    pub position: i64,
    pub score: f32,
}

#[derive(Queryable)]
pub struct TournamentDebateTeamResult {
    pub id: String,
    pub debate_id: String,
    pub team_id: String,
    pub points: i64,
}

pub enum BallotAggregationMethod {
    Consensus,
    // TODO: different formats will aggregate individual ballots differently.
    // This currently implements (what I believe is the correct) WSDC behaviour
    // - the winner is selected based on the number of ballots in favour and the
    // speaks are the average of all the submitted ballots.
    Individual,
}

pub fn aggregate_ballot_set(
    ballots: &[BallotRepr],
    aggregate_how: BallotAggregationMethod,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) {
    assert!(!ballots.is_empty());

    match aggregate_how {
        BallotAggregationMethod::Consensus => {
            for a in ballots {
                for b in ballots {
                    assert!(a.is_isomorphic(b))
                }
            }

            let canonical = &ballots[0];

            let mut speaker_points = Vec::new();

            for score in &canonical.scores {
                speaker_points.push((
                    tournament_debate_speaker_results::id
                        .eq(Uuid::now_v7().to_string()),
                    tournament_debate_speaker_results::debate_id
                        .eq(canonical.ballot.debate_id.clone()),
                    tournament_debate_speaker_results::speaker_id
                        .eq(score.speaker_id.clone()),
                    tournament_debate_speaker_results::team_id
                        .eq(score.team_id.clone()),
                    tournament_debate_speaker_results::position
                        .eq(score.speaker_position),
                    tournament_debate_speaker_results::score.eq(score.score),
                ));
            }

            let mut team_scores = HashMap::new();
            for team in canonical.teams() {
                let scores = canonical.scores_of_team(&team);
                let team_score: Decimal = scores
                    .iter()
                    .map(|score| {
                        rust_decimal::Decimal::from_f32_retain(score.score)
                            .unwrap()
                    })
                    .sum();
                team_scores.insert(team, team_score);
            }

            let team_points =
                team_scores
                    .iter()
                    .sorted_by_key(|score| -score.1)
                    .enumerate()
                    .map(|(idx, (team, _))| {
                        (
                            tournament_debate_team_results::id
                                .eq(Uuid::now_v7().to_string()),
                            tournament_debate_team_results::debate_id
                                .eq(canonical.ballot.debate_id.clone()),
                            tournament_debate_team_results::team_id
                                .eq(team.clone()),
                            tournament_debate_team_results::points
                                .eq((canonical.team_count() - 1 - idx) as i64),
                        )
                    })
                    .collect_vec();

            diesel::insert_into(tournament_debate_team_results::table)
                .values(team_points)
                .execute(conn)
                .unwrap();
            diesel::insert_into(tournament_debate_speaker_results::table)
                .values(speaker_points)
                .execute(conn)
                .unwrap();
        }
        BallotAggregationMethod::Individual => todo!(),
    }
}
