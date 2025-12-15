use std::collections::HashMap;

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use itertools::Itertools;
use rust_decimal::{Decimal, prelude::FromPrimitive};
use uuid::Uuid;

use crate::{
    schema::{
        tournament_debate_speaker_results, tournament_debate_team_results,
    },
    tournaments::{
        Tournament,
        rounds::{ballots::BallotRepr, draws::DebateRepr},
    },
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

/// Callers should check that this ballot set is correct. If an invalid set of
/// ballots is provided (e.g. ballots which disagree when using consensus) this
/// function will likely panic (or write incorrect data to the database).
pub fn aggregate_ballot_set(
    ballots: &[BallotRepr],
    aggregate_how: BallotAggregationMethod,
    tournament: &Tournament,
    debate: &DebateRepr,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) {
    assert!(!ballots.is_empty());

    match aggregate_how {
        BallotAggregationMethod::Consensus => {
            for a in ballots {
                for b in ballots {
                    assert!(a.is_isomorphic(b, tournament, debate))
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
        BallotAggregationMethod::Individual => {
            // voting only makes sense for 2-team formats
            assert_eq!(ballots[0].team_count(), 2);

            let debate = DebateRepr::fetch(&ballots[0].ballot.debate_id, conn);

            let _record_winner_in_db = {
                let (votes_prop, votes_opp) = ballots.iter().fold(
                    (0, 0),
                    |(votes_prop, votes_opp), ballot| {
                        let side = get_side_judge_voted_for_in_2_team_format(
                            &debate, ballot,
                        );
                        match side {
                            0 => (votes_prop + 1, votes_opp),
                            1 => (votes_prop, votes_opp + 1),
                            _ => unreachable!(),
                        }
                    },
                );

                let chair = debate
                    .judges_of_debate
                    .iter()
                    .find(|j| j.status == "C")
                    .unwrap();
                let chair_vote = ballots
                    .iter()
                    .find_map(|ballot| {
                        if ballot.ballot().judge_id == chair.judge_id {
                            let side =
                                get_side_judge_voted_for_in_2_team_format(
                                    &debate, ballot,
                                );
                            Some(side)
                        } else {
                            None
                        }
                    })
                    .unwrap();

                let did_prop_win = if votes_opp < votes_prop {
                    true
                } else if votes_prop < votes_opp {
                    false
                } else {
                    chair_vote == 0
                };

                diesel::insert_into(tournament_debate_team_results::table)
                    .values(&[
                        (
                            tournament_debate_team_results::id
                                .eq(Uuid::now_v7().to_string()),
                            tournament_debate_team_results::debate_id
                                .eq(debate.debate.id.clone()),
                            tournament_debate_team_results::team_id.eq(debate
                                .team_of_side_and_seq(0, 0)
                                .team_id
                                .clone()),
                            tournament_debate_team_results::points
                                .eq(did_prop_win as i64),
                        ),
                        (
                            tournament_debate_team_results::id
                                .eq(Uuid::now_v7().to_string()),
                            tournament_debate_team_results::debate_id
                                .eq(debate.debate.id.clone()),
                            tournament_debate_team_results::team_id.eq(debate
                                .team_of_side_and_seq(0, 0)
                                .team_id
                                .clone()),
                            tournament_debate_team_results::points
                                .eq(!did_prop_win as i64),
                        ),
                    ])
                    .execute(conn)
                    .unwrap();
            };

            let _add_speaks = {
                let ballot_a = &ballots[0];

                let mut scores_for_db = Vec::new();
                for score in &ballot_a.scores {
                    let mut sum: Decimal = score.score.try_into().unwrap();

                    for other_ballots in ballots.iter().filter(|ballot| {
                        ballot.ballot().id != ballot_a.ballot().id
                    }) {
                        let other_score: Decimal = other_ballots
                            .scores
                            .iter()
                            .find(|o| {
                                o.speaker_id == score.speaker_id
                                    && o.speaker_position
                                        == score.speaker_position
                            })
                            .unwrap()
                            .score
                            .try_into()
                            .unwrap();
                        sum += other_score;
                    }

                    let avg =
                        sum / (Decimal::from_usize(ballots.len()).unwrap());

                    // TODO: we record iron-person speeches here, but we later
                    // need to handle them properly!
                    scores_for_db.push((
                        tournament_debate_speaker_results::id
                            .eq(Uuid::now_v7().to_string()),
                        tournament_debate_speaker_results::debate_id
                            .eq(debate.debate.id.clone()),
                        tournament_debate_speaker_results::speaker_id
                            .eq(score.speaker_id.clone()),
                        tournament_debate_speaker_results::team_id
                            .eq(score.team_id.clone()),
                        tournament_debate_speaker_results::position
                            .eq(score.speaker_position),
                        tournament_debate_speaker_results::score.eq({
                            let x: f32 = avg.round_dp(2).try_into().unwrap();
                            x
                        }),
                    ));
                }
                scores_for_db
            };
        }
    }
}

fn get_side_judge_voted_for_in_2_team_format(
    debate: &DebateRepr,
    ballot: &BallotRepr,
) -> i64 {
    let winner = ballot.teams_in_rank_order().next().unwrap();
    let side = debate
        .teams_of_debate
        .iter()
        .find(|team| team.id == winner)
        .unwrap()
        .side;
    side
}
