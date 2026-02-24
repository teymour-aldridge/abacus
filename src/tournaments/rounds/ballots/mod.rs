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
    #[tracing::instrument(skip(conn))]
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
    let ballots = debate.latest_ballots(conn);

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

use crate::tournaments::participants::TournamentParticipants;
use uuid::Uuid;

pub struct BallotTeamSpeakersBuilder<'a> {
    tournament: &'a Tournament,
    participants: &'a TournamentParticipants,
    records_scores: bool,
    records_positions: bool,
    expected_speakers: usize,

    scores: Vec<(String, Option<f32>)>,
}

impl<'a> BallotTeamSpeakersBuilder<'a> {
    pub fn new(
        tournament: &'a Tournament,
        participants: &'a TournamentParticipants,
        records_scores: bool,
        records_positions: bool,
    ) -> Self {
        let expected_speakers = tournament.substantive_speakers as usize
            + if tournament.reply_speakers { 1 } else { 0 };
        Self {
            tournament,
            participants,
            records_scores,
            records_positions,
            expected_speakers,
            scores: Vec::new(),
        }
    }

    pub fn add_speaker(
        mut self,
        speaker_id: &str,
        score: Option<f32>,
    ) -> Result<Self, String> {
        if !self.records_positions {
            return Err(
                "Error: speakers should not be submitted for this round type"
                    .into(),
            );
        }

        let position = self.scores.len();
        if position >= self.expected_speakers {
            return Err("Error: too many speakers added".into());
        }

        let score = if self.records_scores {
            if let Some(score_val) = score {
                let speaker_name = self
                    .participants
                    .speakers
                    .get(speaker_id)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();

                let dec = rust_decimal::Decimal::from_f32_retain(score_val)
                    .ok_or("Invalid score")?;

                self.tournament
                    .check_score_valid(
                        dec,
                        position
                            >= self.tournament.substantive_speakers as usize,
                        speaker_name,
                    )
                    .map_err(|e| e)?;
            }
            score
        } else {
            None
        };

        self.scores.push((speaker_id.to_string(), score));
        Ok(self)
    }

    pub fn build(self) -> Result<Vec<(String, Option<f32>)>, String> {
        if self.records_positions && self.scores.len() != self.expected_speakers
        {
            return Err("Error: missing speaker information".into());
        }
        Ok(self.scores)
    }
}

pub struct BallotBuilder<'a> {
    tournament: &'a Tournament,
    debate: &'a DebateRepr,
    participants: &'a TournamentParticipants,
    metadata: BallotMetadata,

    records_positions: bool,
    records_scores: bool,
    is_elim: bool,
    num_advancing: Option<usize>,

    scores: Vec<BallotScore>,
    advancing_team_ids: Vec<String>,
    teams_added: usize,
}

impl<'a> BallotBuilder<'a> {
    pub fn new(
        tournament: &'a Tournament,
        debate: &'a DebateRepr,
        round: &crate::tournaments::rounds::Round,
        participants: &'a TournamentParticipants,
        mut metadata: BallotMetadata,
        expected_version: i64,
        prior_version: i64,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<Self, String> {
        if expected_version != prior_version {
            return Err("Error: The ballot has been modified since you started editing. Please reload the page to see the latest version.".into());
        }
        if !debate.motions.contains_key(&metadata.motion_id) {
            return Err("Error: invalid motion".into());
        }

        if prior_version == 0 {
            assert_eq!(expected_version, 0);
        } else {
            assert_eq!(expected_version, prior_version + 1);
        }

        metadata.version = expected_version;

        let num_advancing = if round.is_elim() {
            Some(num_advancing_for_elim_round(tournament, round, conn))
        } else {
            None
        };


        Ok(Self {
            tournament,
            debate,
            participants,
            metadata,
            records_positions: tournament.round_requires_speaker_order(round),
            records_scores: tournament.round_requires_speaks(round),
            is_elim: round.is_elim(),
            num_advancing,
            scores: Vec::new(),
            advancing_team_ids: Vec::new(),
            teams_added: 0,
        })
    }

    pub fn team_speakers_builder(&self) -> BallotTeamSpeakersBuilder<'a> {
        BallotTeamSpeakersBuilder::new(
            self.tournament,
            self.participants,
            self.records_scores,
            self.records_positions,
        )
    }

    pub fn add_team(
        &mut self,
        side: usize,
        seq: usize,
        speakers: Vec<(String, Option<f32>)>,
        points: Option<usize>,
    ) -> Result<(), String> {
        tracing::debug!("Adding team for side={side} and seq={seq}");

        let dt = self.debate.team_of_side_and_seq(side as i64, seq as i64);

        if !self.records_positions && !speakers.is_empty() {
            return Err(
                "Error: speakers should not be submitted for this round type"
                    .into(),
            );
        }

        for (j, (speaker_id, score)) in speakers.into_iter().enumerate() {
            self.scores.push(BallotScore {
                id: Uuid::now_v7().to_string(),
                tournament_id: self.tournament.id.clone(),
                ballot_id: self.metadata.id.clone(),
                team_id: dt.team_id.clone(),
                speaker_id,
                speaker_position: j as i64,
                score,
            });
        }

        if self.is_elim {
            if points == Some(1) {
                self.advancing_team_ids.push(dt.team_id.clone());
            }
        }

        self.teams_added += 1;
        Ok(())
    }

    pub fn build(self) -> Result<BallotRepr, String> {
        let expected_teams = (self.tournament.teams_per_side * 2) as usize;
        if self.teams_added != expected_teams {
            tracing::debug!("rejecting ballot due to incorrect number of teams (self.teams_added={}, expected_teams={})", self.teams_added, expected_teams);
            return Err("Error: incorrect number of teams submitted".into());
        }

        let team_ranks = if self.is_elim {
            let num_advancing = self.num_advancing.expect("self.num_advancing must be computed for elimination rounds");
            if self.advancing_team_ids.len() != num_advancing {
                return Err(format!(
                    "Error: expected {} advancing team(s), but {} were selected",
                    num_advancing,
                    self.advancing_team_ids.len()
                ));
            }
            build_team_ranks_from_advancing(
                &self.advancing_team_ids,
                &self.metadata.id,
                &self.tournament.id,
                self.debate,
            )
        } else {
            build_team_ranks_from_scores(
                &self.scores,
                &self.metadata.id,
                &self.tournament.id,
                self.debate,
                self.tournament,
            )
        };

        Ok(BallotRepr::new_prelim(
            self.metadata,
            self.scores,
            team_ranks,
        ))
    }
}

fn num_advancing_for_elim_round(
    tournament: &Tournament,
    round: &crate::tournaments::rounds::Round,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> usize {
    let total_teams = (tournament.teams_per_side * 2) as usize;
    if total_teams == 2 {
        1
    } else if round.is_final_of_break_category(conn) {
        1
    } else {
        total_teams / 2
    }
}

/// Build team rank entries from speaker scores.
///
/// Teams receive one point for each team that they beat.
fn build_team_ranks_from_scores(
    scores: &[BallotScore],
    ballot_id: &str,
    tournament_id: &str,
    debate: &DebateRepr,
    tournament: &Tournament,
) -> Vec<BallotTeamRank> {
    let n_teams = (tournament.teams_per_side * 2) as usize;

    let mut team_totals: Vec<(String, f64)> = Vec::new();
    for dt in &debate.teams_of_debate {
        let total: f64 = scores
            .iter()
            .filter(|s| s.team_id == dt.team_id)
            .filter_map(|s| s.score.map(|v| v as f64))
            .sum();
        team_totals.push((dt.team_id.clone(), total));
    }

    team_totals.sort_by(|a, b| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Assign points: in a 2-team format, winner=1, loser=0.
    // In a 4-team format, 1st=3, 2nd=2, 3rd=1, 4th=0.
    team_totals
        .iter()
        .enumerate()
        .map(|(rank, (team_id, _))| BallotTeamRank {
            id: Uuid::now_v7().to_string(),
            tournament_id: tournament_id.to_string(),
            ballot_id: ballot_id.to_string(),
            team_id: team_id.clone(),
            points: (n_teams - 1 - rank) as i64,
        })
        .collect()
}

/// For advancing rounds we adopt the convention that advancing teams receive
/// one point and that eliminated teams receive zero points.
fn build_team_ranks_from_advancing(
    advancing_team_ids: &[String],
    ballot_id: &str,
    tournament_id: &str,
    debate: &DebateRepr,
) -> Vec<BallotTeamRank> {
    debate
        .teams_of_debate
        .iter()
        .map(|dt| {
            let is_advancing = advancing_team_ids.contains(&dt.team_id);
            BallotTeamRank {
                id: Uuid::now_v7().to_string(),
                tournament_id: tournament_id.to_string(),
                ballot_id: ballot_id.to_string(),
                team_id: dt.team_id.clone(),
                points: is_advancing as i64,
            }
        })
        .collect()
}
