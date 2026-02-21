//! Ballot management is probably the most painful part of this application.
//! We handle a diverse array of potential formats. The cartesian product of
//! configuration options quickly leads to combinatorial explosion, so we try
//! to handle each format individually.

use std::cmp::Reverse;

use chrono::NaiveDateTime;
use diesel::{
    Queryable, connection::LoadConnection, prelude::*, sqlite::Sqlite,
};
use itertools::Itertools;
use rust_decimal::Decimal;

use crate::{
    schema::{
        tournament_ballots, tournament_speaker_score_entries,
        tournament_team_rank_entries,
    },
    tournaments::{
        Tournament,
        rounds::{draws::DebateRepr, side_names},
    },
};

pub mod aggregate;
pub mod form;
pub mod manage;
pub mod public;

#[derive(Debug)]
pub struct BallotRepr {
    pub metadata: BallotMetadata,
    pub scores: Vec<BallotScore>,
    pub team_ranks: Vec<BallotTeamRank>,
}

impl BallotRepr {
    pub fn new_prelim(
        metadata: BallotMetadata,
        scores: Vec<BallotScore>,
        team_ranks: Vec<BallotTeamRank>,
    ) -> Self {
        Self {
            metadata,
            scores,
            team_ranks,
        }
    }

    pub fn new_elim(
        metadata: BallotMetadata,
        advancing: Vec<BallotTeamRank>,
    ) -> Self {
        Self {
            metadata,
            scores: Vec::new(),
            team_ranks: advancing,
        }
    }

    /// Insert the provided ballot into the database. This cannot be used to
    /// update existing ballots, only to create new ones!
    pub fn insert(&self, conn: &mut impl LoadConnection<Backend = Sqlite>) {
        let n = diesel::insert_into(tournament_ballots::table)
            .values((
                tournament_ballots::id.eq(&self.metadata.id),
                tournament_ballots::tournament_id
                    .eq(&self.metadata.tournament_id),
                tournament_ballots::debate_id.eq(&self.metadata.debate_id),
                tournament_ballots::judge_id.eq(&self.metadata.judge_id),
                tournament_ballots::submitted_at
                    .eq(&self.metadata.submitted_at),
                tournament_ballots::motion_id.eq(&self.metadata.motion_id),
                tournament_ballots::version.eq(&self.metadata.version),
                tournament_ballots::change.eq(&self.metadata.change),
                tournament_ballots::editor_id.eq(&self.metadata.editor_id),
            ))
            .execute(conn)
            .unwrap();
        assert_eq!(n, 1);

        for team_rank in &self.team_ranks {
            let n = diesel::insert_into(tournament_team_rank_entries::table)
                .values((
                    tournament_team_rank_entries::id.eq(&team_rank.id),
                    tournament_team_rank_entries::tournament_id
                        .eq(&team_rank.tournament_id),
                    tournament_team_rank_entries::ballot_id
                        .eq(&team_rank.ballot_id),
                    tournament_team_rank_entries::team_id
                        .eq(&team_rank.team_id),
                    tournament_team_rank_entries::points.eq(&team_rank.points),
                ))
                .execute(conn)
                .unwrap();
            assert_eq!(n, 1);
        }

        for score in &self.scores {
            let n =
                diesel::insert_into(tournament_speaker_score_entries::table)
                    .values((
                        tournament_speaker_score_entries::id.eq(&score.id),
                        tournament_speaker_score_entries::tournament_id
                            .eq(&score.tournament_id),
                        tournament_speaker_score_entries::ballot_id
                            .eq(&score.ballot_id),
                        tournament_speaker_score_entries::team_id
                            .eq(&score.team_id),
                        tournament_speaker_score_entries::speaker_id
                            .eq(&score.speaker_id),
                        tournament_speaker_score_entries::speaker_position
                            .eq(&score.speaker_position),
                        tournament_speaker_score_entries::score
                            .eq(&score.score),
                    ))
                    .execute(conn)
                    .unwrap();
            assert_eq!(n, 1);
        }
    }

    pub fn get_human_readable_description_for_problems(
        &self,
        other: &BallotRepr,
        tournament: &Tournament,
        debate: &DebateRepr,
    ) -> Vec<String> {
        let mut problems = Vec::new();

        let self_judge_name =
            &debate.judges.get(&self.metadata.judge_id).unwrap().name;
        let other_judge_name =
            &debate.judges.get(&other.metadata.judge_id).unwrap().name;

        // Compare advancing team selections (relevant for elim rounds, and
        // always checked since the team_ranks are present on every ballot).
        for team_rank_a in &self.team_ranks {
            if let Some(team_rank_b) = other
                .team_ranks
                .iter()
                .find(|tr| tr.team_id == team_rank_a.team_id)
            {
                if team_rank_a.points != team_rank_b.points {
                    let team_name = debate
                        .teams
                        .get(&team_rank_a.team_id)
                        .map(|t| t.name.as_str())
                        .unwrap_or("unknown team");
                    problems.push(ammonia::clean(&format!(
                        "Error: the ballot from {} gives {} {} point(s), \
                         whereas the ballot from {} gives them {} point(s).",
                        self_judge_name,
                        team_name,
                        team_rank_a.points,
                        other_judge_name,
                        team_rank_b.points,
                    )));
                }
            }
        }

        // Compare speaker positions and scores (only when both ballots have
        // speaker data).
        if !self.scores.is_empty() && !other.scores.is_empty() {
            for side in 0..=1 {
                for team in 0..tournament.teams_per_side {
                    for speaker in 0..tournament.substantive_speakers {
                        let team_in_this_pos =
                            debate.team_of_side_and_seq(side, team);
                        let score_of_ballot_a = self
                            .scores_of_team(&team_in_this_pos.team_id)
                            .iter()
                            .find(|score| score.speaker_position == speaker)
                            .cloned()
                            .unwrap();
                        let score_of_ballot_b = other
                            .scores_of_team(&team_in_this_pos.team_id)
                            .iter()
                            .find(|score| score.speaker_position == speaker)
                            .cloned()
                            .unwrap();

                        if score_of_ballot_a.speaker_id
                            != score_of_ballot_b.speaker_id
                        {
                            problems.push(ammonia::clean(&format!(
                                "Error: the ballot from {} has {} as {}, whereas \
                                the ballot from {} has {} as {}.",
                                self_judge_name,
                                debate
                                    .speakers_of_team
                                    .get(&team_in_this_pos.team_id)
                                    .unwrap()
                                    .iter()
                                    .find(|s| s.id == score_of_ballot_a.speaker_id)
                                    .unwrap()
                                    .name,
                                side_names::name_of_side(
                                    tournament, side, speaker, true,
                                ),
                                other_judge_name,
                                debate
                                    .speakers_of_team
                                    .get(&team_in_this_pos.team_id)
                                    .unwrap()
                                    .iter()
                                    .find(|s| s.id == score_of_ballot_b.speaker_id)
                                    .unwrap()
                                    .name,
                                side_names::name_of_side(
                                    tournament, side, speaker, true,
                                ),
                            )))
                        }

                        // Compare scores when both ballots have them
                        if let (Some(a), Some(b)) =
                            (score_of_ballot_a.score, score_of_ballot_b.score)
                        {
                            if (a - b).abs() > f32::EPSILON {
                                problems.push(ammonia::clean(&format!(
                                    "Error: the ballot from {} has a score of {} for {} as {}, whereas \
                                    the ballot from {} has a score of {} for {} as {}.",
                                    self_judge_name,
                                    a,
                                    debate.speakers_of_team.get(&team_in_this_pos.team_id).unwrap()
                                        .iter().find(|s| s.id == score_of_ballot_a.speaker_id).unwrap().name,
                                    side_names::name_of_side(tournament, side, speaker, true),
                                    other_judge_name,
                                    b,
                                    debate.speakers_of_team.get(&team_in_this_pos.team_id).unwrap()
                                        .iter().find(|s| s.id == score_of_ballot_b.speaker_id).unwrap().name,
                                    side_names::name_of_side(tournament, side, speaker, true),
                                )));
                            }
                        }
                    }
                }
            }
        }

        problems
    }

    pub fn is_isomorphic(
        &self,
        other: &BallotRepr,
        tournament: &Tournament,
        debate: &DebateRepr,
    ) -> bool {
        self.get_human_readable_description_for_problems(
            other, tournament, debate,
        )
        .is_empty()
    }

    pub fn problems_of_set(
        ballots: &[BallotRepr],
        tournament: &Tournament,
        debate: &DebateRepr,
    ) -> Vec<String> {
        let debate_id = &debate.debate.id;
        for ballot in ballots {
            assert_eq!(
                &ballot.metadata.debate_id, debate_id,
                "All ballots must be from the same debate"
            );
        }

        let mut problems = Vec::new();
        for ballot in ballots {
            for other_ballot in ballots {
                if ballot.metadata.id == other_ballot.metadata.id {
                    continue;
                }
                problems.extend(
                    ballot.get_human_readable_description_for_problems(
                        other_ballot,
                        tournament,
                        debate,
                    ),
                );
            }
        }
        problems
    }

    pub fn ballot(&self) -> &BallotMetadata {
        &self.metadata
    }

    pub fn fetch(
        ballot_id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Self {
        let ballot = tournament_ballots::table
            .filter(tournament_ballots::id.eq(ballot_id))
            .first::<BallotMetadata>(conn)
            .unwrap();

        let team_ranks = tournament_team_rank_entries::table
            .filter(tournament_team_rank_entries::ballot_id.eq(&ballot.id))
            .load::<BallotTeamRank>(conn)
            .unwrap();

        let scores = tournament_speaker_score_entries::table
            .filter(tournament_speaker_score_entries::ballot_id.eq(&ballot.id))
            .load::<BallotScore>(conn)
            .unwrap();

        Self {
            metadata: ballot,
            scores,
            team_ranks,
        }
    }

    pub fn team_count(&self) -> usize {
        self.team_ids().count()
    }

    pub fn team_ids(&self) -> impl Iterator<Item = &str> {
        self.team_ranks.iter().map(|r| r.team_id.as_str()).unique()
    }

    pub fn teams(&self) -> impl Iterator<Item = String> + '_ {
        self.team_ids().map(|s| s.to_string())
    }

    /// Returns the IDs of the teams sorted descending by total score.
    pub fn teams_in_rank_order(&self) -> impl Iterator<Item = String> + '_ {
        self.team_ids()
            .map(|id| id.to_string())
            .sorted_by_key(|team| {
                let total: Decimal = self
                    .scores_of_team(team)
                    .iter()
                    .filter_map(|score| {
                        score.score.map(|s| Decimal::try_from(s).unwrap())
                    })
                    .sum();
                Reverse(total)
            })
    }

    pub fn scores_of_team(&self, team_id: &str) -> Vec<BallotScore> {
        self.scores
            .iter()
            .filter(|s| s.team_id == team_id)
            .sorted_by_key(|s| s.speaker_position)
            .cloned()
            .collect()
    }

    pub fn points_of_team(&self, team_id: &str) -> i64 {
        self.team_ranks
            .iter()
            .find(|t| t.team_id == team_id)
            .unwrap()
            .points
    }
}

/// Recompute the status of a debate based on its current ballots.
///
/// - If all non-trainee judges have submitted and the ballots are consistent,
///   the status is set to `confirmed`.
///   TODO: create a mode where ballots can be manually reviewed
/// - If all non-trainee judges have submitted but there are conflicts, the
///   status is set to `conflict`.
/// - Otherwise the status is set to `draft`.
pub fn update_debate_status(
    debate: &DebateRepr,
    tournament: &Tournament,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) {
    let ballots = debate.ballots(conn);

    let non_trainee_judges: Vec<_> = debate
        .judges_of_debate
        .iter()
        .filter(|j| j.status != "T")
        .collect();

    let all_non_trainees_submitted = non_trainee_judges.iter().all(|judge| {
        ballots
            .iter()
            .any(|b| b.metadata.judge_id == judge.judge_id)
    });

    let status = if all_non_trainees_submitted && !non_trainee_judges.is_empty()
    {
        let problems =
            BallotRepr::problems_of_set(&ballots, tournament, debate);
        if problems.is_empty() {
            "confirmed"
        } else {
            "conflict"
        }
    } else {
        "draft"
    };

    diesel::update(
        crate::schema::tournament_debates::table.filter(
            crate::schema::tournament_debates::id.eq(&debate.debate.id),
        ),
    )
    .set(crate::schema::tournament_debates::status.eq(status))
    .execute(conn)
    .unwrap();
}

#[derive(Queryable, Debug, Clone)]
pub struct BallotMetadata {
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
#[diesel(table_name = tournament_team_rank_entries)]
pub struct BallotTeamRank {
    pub id: String,
    pub tournament_id: String,
    pub ballot_id: String,
    pub team_id: String,
    pub points: i64,
}

#[derive(Queryable, QueryableByName, Clone, Debug)]
#[diesel(check_for_backend(Sqlite))]
#[diesel(table_name = tournament_speaker_score_entries)]
pub struct BallotScore {
    pub id: String,
    pub tournament_id: String,
    pub ballot_id: String,
    pub team_id: String,
    pub speaker_id: String,
    pub speaker_position: i64,
    pub score: Option<f32>,
}
