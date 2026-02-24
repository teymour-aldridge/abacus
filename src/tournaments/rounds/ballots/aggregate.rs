use std::collections::HashSet;

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use rust_decimal::{Decimal, prelude::FromPrimitive};
use uuid::Uuid;

use crate::{
    schema::{agg_speaker_results_of_debate, agg_team_results_of_debate},
    tournaments::{
        Tournament,
        rounds::{Round, ballots::BallotRepr, draws::DebateRepr},
    },
};

#[derive(Queryable)]
pub struct TournamentDebateSpeakerResult {
    pub id: String,
    pub tournament_id: String,
    pub debate_id: String,
    pub speaker_id: String,
    pub team_id: String,
    pub position: i64,
    pub score: Option<f32>,
}

#[derive(Queryable)]
pub struct TournamentDebateTeamResult {
    pub id: String,
    pub tournament_id: String,
    pub debate_id: String,
    pub team_id: String,
    pub points: Option<i64>,
}

pub enum BallotAggregationMethod {
    Consensus,
    Individual,
}

/// Callers should check that this ballot set is correct. If an invalid set of
/// ballots is provided (e.g. ballots which disagree when using consensus) this
/// function will likely panic (or write incorrect data to the database).
pub fn aggregate_ballot_set(
    ballots: &[BallotRepr],
    tournament: &Tournament,
    debate: &DebateRepr,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) {
    assert!(!ballots.is_empty());
    let method = tournament.agg_method_for_current_round(conn);

    for a in ballots {
        for b in ballots {
            assert!(a.is_isomorphic(b, tournament, debate))
        }
    }

    // todo: we very often conduct numerous highly unnecessary queries like
    // this one
    let is_elim =
        Round::fetch(&debate.debate.round_id, conn).unwrap().kind == "E";

    match method {
        BallotAggregationMethod::Consensus => {
            let canonical = &ballots[0];
            if is_elim {
                aggregate_consensus_elimination(canonical, debate, conn);
            } else {
                aggregate_consensus_prelim(canonical, tournament, conn);
            }
        }
        BallotAggregationMethod::Individual => {
            assert_eq!(
                ballots[0].team_count(),
                2,
                "Individual ballot aggregation requires exactly 2 teams"
            );

            let did_prop_win = determine_winner_by_vote(ballots, debate);

            insert_two_team_results(ballots, debate, did_prop_win, conn);

            if !is_elim && tournament.current_round_requires_speaks(conn) {
                let speaker_points = compute_averaged_speaker_scores(
                    ballots,
                    &ballots[0].metadata.tournament_id,
                    &ballots[0].metadata.debate_id,
                );

                diesel::insert_into(agg_speaker_results_of_debate::table)
                    .values(speaker_points)
                    .execute(conn)
                    .unwrap();
            }
        }
    }
}

fn aggregate_consensus_prelim(
    canonical: &BallotRepr,
    tournament: &Tournament,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) {
    let team_points: Vec<_> = canonical
        .team_ranks
        .iter()
        .map(|team| {
            (
                agg_team_results_of_debate::id.eq(Uuid::now_v7().to_string()),
                agg_team_results_of_debate::tournament_id
                    .eq(canonical.metadata.tournament_id.clone()),
                agg_team_results_of_debate::debate_id
                    .eq(canonical.metadata.debate_id.clone()),
                agg_team_results_of_debate::team_id.eq(team.team_id.clone()),
                agg_team_results_of_debate::points.eq(Some(team.points)),
            )
        })
        .collect();

    diesel::insert_into(agg_team_results_of_debate::table)
        .values(team_points)
        .execute(conn)
        .unwrap();

    if tournament.current_round_requires_speaks(conn) {
        let speaker_points: Vec<_> = canonical
            .scores
            .iter()
            .map(|score| {
                (
                    agg_speaker_results_of_debate::id
                        .eq(Uuid::now_v7().to_string()),
                    agg_speaker_results_of_debate::tournament_id
                        .eq(canonical.metadata.tournament_id.clone()),
                    agg_speaker_results_of_debate::debate_id
                        .eq(canonical.metadata.debate_id.clone()),
                    agg_speaker_results_of_debate::speaker_id
                        .eq(score.speaker_id.clone()),
                    agg_speaker_results_of_debate::team_id
                        .eq(score.team_id.clone()),
                    agg_speaker_results_of_debate::position
                        .eq(score.speaker_position),
                    agg_speaker_results_of_debate::score.eq(score.score),
                )
            })
            .collect();

        if !speaker_points.is_empty() {
            diesel::insert_into(agg_speaker_results_of_debate::table)
                .values(speaker_points)
                .execute(conn)
                .unwrap();
        }
    }
}

fn aggregate_consensus_elimination(
    canonical: &BallotRepr,
    debate: &DebateRepr,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) {
    let advancing: HashSet<String> = canonical
        .team_ranks
        .iter()
        .filter(|s| {
            assert!(s.points == 0 || s.points == 1, "{}", s.points);
            s.points == 1
        })
        .map(|s| s.team_id.clone())
        .collect();

    let team_points: Vec<_> = debate
        .teams_of_debate
        .iter()
        .map(|team| {
            let is_advancing = advancing.contains(team.team_id.as_str());
            (
                agg_team_results_of_debate::id.eq(Uuid::now_v7().to_string()),
                agg_team_results_of_debate::tournament_id
                    .eq(canonical.metadata.tournament_id.clone()),
                agg_team_results_of_debate::debate_id
                    .eq(canonical.metadata.debate_id.clone()),
                agg_team_results_of_debate::team_id.eq(team.team_id.clone()),
                agg_team_results_of_debate::points
                    .eq(Some(is_advancing as i64)),
            )
        })
        .collect();

    diesel::insert_into(agg_team_results_of_debate::table)
        .values(team_points)
        .execute(conn)
        .unwrap();
}

/// Determine the winner in a 2-team format by counting votes.
/// Returns `true` if the proposition (side 0) won.
fn determine_winner_by_vote(
    ballots: &[BallotRepr],
    debate: &DebateRepr,
) -> bool {
    let (votes_prop, votes_opp) =
        ballots
            .iter()
            .fold((0, 0), |(votes_prop, votes_opp), ballot| {
                let side =
                    side_judge_voted_for_in_2_team_format(debate, ballot);
                match side {
                    0 => (votes_prop + 1, votes_opp),
                    1 => (votes_prop, votes_opp + 1),
                    _ => unreachable!(),
                }
            });

    if votes_prop > votes_opp {
        return true;
    }
    if votes_opp > votes_prop {
        return false;
    }

    let chair = debate
        .judges_of_debate
        .iter()
        .find(|j| j.status == "C")
        .expect("No chair judge found");

    let chair_vote = ballots
        .iter()
        .find_map(|ballot| {
            if ballot.ballot().judge_id == chair.judge_id {
                Some(side_judge_voted_for_in_2_team_format(debate, ballot))
            } else {
                None
            }
        })
        .expect("Chair ballot not found");

    chair_vote == 0
}

fn insert_two_team_results(
    ballots: &[BallotRepr],
    debate: &DebateRepr,
    did_prop_win: bool,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) {
    let prop_team = debate.team_of_side_and_seq(0, 0);
    let opp_team = debate.team_of_side_and_seq(1, 0);

    let team_points = vec![
        (
            agg_team_results_of_debate::id.eq(Uuid::now_v7().to_string()),
            agg_team_results_of_debate::tournament_id
                .eq(ballots[0].metadata.tournament_id.clone()),
            agg_team_results_of_debate::debate_id
                .eq(ballots[0].metadata.debate_id.clone()),
            agg_team_results_of_debate::team_id.eq(prop_team.team_id.clone()),
            agg_team_results_of_debate::points.eq(Some(did_prop_win as i64)),
        ),
        (
            agg_team_results_of_debate::id.eq(Uuid::now_v7().to_string()),
            agg_team_results_of_debate::tournament_id
                .eq(ballots[0].metadata.tournament_id.clone()),
            agg_team_results_of_debate::debate_id
                .eq(ballots[0].metadata.debate_id.clone()),
            agg_team_results_of_debate::team_id.eq(opp_team.team_id.clone()),
            agg_team_results_of_debate::points.eq(Some(!did_prop_win as i64)),
        ),
    ];

    diesel::insert_into(agg_team_results_of_debate::table)
        .values(team_points)
        .execute(conn)
        .unwrap();
}

fn compute_averaged_speaker_scores(
    ballots: &[BallotRepr],
    tournament_id: &str,
    debate_id: &str,
) -> Vec<(
    diesel::dsl::Eq<agg_speaker_results_of_debate::id, String>,
    diesel::dsl::Eq<agg_speaker_results_of_debate::tournament_id, String>,
    diesel::dsl::Eq<agg_speaker_results_of_debate::debate_id, String>,
    diesel::dsl::Eq<agg_speaker_results_of_debate::speaker_id, String>,
    diesel::dsl::Eq<agg_speaker_results_of_debate::team_id, String>,
    diesel::dsl::Eq<agg_speaker_results_of_debate::position, i64>,
    diesel::dsl::Eq<agg_speaker_results_of_debate::score, Option<f32>>,
)> {
    let mut speaker_points = Vec::new();

    for score in &ballots[0].scores {
        let mut sum: Decimal = score
            .score
            .expect("Scores must be provided when speaks are enabled")
            .try_into()
            .unwrap();

        for other_ballot in ballots.iter().skip(1) {
            let other_score: Decimal = other_ballot
                .scores
                .iter()
                .find(|o| {
                    o.speaker_id == score.speaker_id
                        && o.speaker_position == score.speaker_position
                })
                .expect("Speaker score not found in ballot")
                .score
                .expect("Scores must be provided when speaks are enabled")
                .try_into()
                .unwrap();

            sum += other_score;
        }

        let avg = sum / Decimal::from_usize(ballots.len()).unwrap();
        let avg_f32: f32 = avg.round_dp(2).try_into().unwrap();

        speaker_points.push((
            agg_speaker_results_of_debate::id.eq(Uuid::now_v7().to_string()),
            agg_speaker_results_of_debate::tournament_id
                .eq(tournament_id.to_string()),
            agg_speaker_results_of_debate::debate_id.eq(debate_id.to_string()),
            agg_speaker_results_of_debate::speaker_id
                .eq(score.speaker_id.clone()),
            agg_speaker_results_of_debate::team_id.eq(score.team_id.clone()),
            agg_speaker_results_of_debate::position.eq(score.speaker_position),
            agg_speaker_results_of_debate::score.eq(Some(avg_f32)),
        ));
    }

    speaker_points
}

fn side_judge_voted_for_in_2_team_format(
    debate: &DebateRepr,
    ballot: &BallotRepr,
) -> i64 {
    let winner = ballot
        .team_ranks
        .iter()
        .find(|team| team.points == 1)
        .expect("No winning team found on ballot")
        .team_id
        .clone();

    debate
        .teams_of_debate
        .iter()
        .find(|team| team.team_id == winner)
        .expect("Winning team not found in debate")
        .side
}
