//! Speaker standings from released, completed preliminary-round aggregates.

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use rust_decimal::{Decimal, MathematicalOps, prelude::ToPrimitive};
use uuid::Uuid;

use crate::{
    schema::{
        agg_speaker_results_of_debate as scores, debates, rounds,
        speaker_metrics, speaker_standings, speakers,
    },
    tournaments::{Tournament, config::SpeakerMetric},
};

pub struct SpeakerStandings {
    /// Ranking criteria, in the tournament's configured tie-break order.
    pub metrics: Vec<SpeakerMetric>,
    pub rows: Vec<SpeakerStandingRow>,
}

#[derive(Debug)]
pub struct SpeakerStandingRow {
    pub speaker_id: String,
    pub name: String,
    pub rank: usize,
    pub tied: bool,
    /// Values in the same order as `SpeakerStandings::metrics`.
    pub metrics: Vec<Decimal>,
}

#[derive(Default)]
struct SpeakerScores {
    name: String,
    scores: Vec<Decimal>,
    rounds: HashSet<String>,
}

impl SpeakerStandings {
    /// Only non-null substantive scores in completed, fully released
    /// preliminary rounds contribute. Speakers with no such scores are omitted.
    ///
    /// A nonnegative `exclude_from_speaker_standings_after` is interpreted as
    /// the maximum number of these rounds a speaker may miss. A round counts
    /// as attended when it has a qualifying score; multiple scores in one
    /// round do not make up for another missed round. `None` and negative
    /// values impose no limit. Standard deviation is the population value.
    pub fn recompute(
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> QueryResult<Self> {
        let tournament = crate::schema::tournaments::table
            .find(tid)
            .first::<Tournament>(conn)?;
        let metrics: Vec<SpeakerMetric> =
            serde_json::from_str(&tournament.speaker_standings_metrics)
                .map_err(|err| {
                    diesel::result::Error::DeserializationError(Box::new(err))
                })?;

        let round_count = rounds::table
            .filter(rounds::tournament_id.eq(&tournament.id))
            .filter(rounds::kind.eq("P"))
            .filter(rounds::completed.eq(true))
            .filter(rounds::draw_status.eq("released_full"))
            .count()
            .get_result::<i64>(conn)?;

        let results = scores::table
            .inner_join(speakers::table)
            .inner_join(debates::table)
            .inner_join(rounds::table.on(rounds::id.eq(debates::round_id)))
            .filter(scores::tournament_id.eq(&tournament.id))
            .filter(speakers::tournament_id.eq(&tournament.id))
            .filter(debates::tournament_id.eq(&tournament.id))
            .filter(rounds::tournament_id.eq(&tournament.id))
            .filter(rounds::kind.eq("P"))
            .filter(rounds::completed.eq(true))
            .filter(rounds::draw_status.eq("released_full"))
            .filter(scores::position.lt(tournament.substantive_speakers))
            .filter(scores::score.is_not_null())
            .select((
                speakers::id,
                speakers::name,
                rounds::id,
                scores::score.assume_not_null(),
            ))
            .load::<(String, String, String, f32)>(conn)?;

        let mut by_speaker: HashMap<String, SpeakerScores> = HashMap::new();
        for (speaker_id, name, round_id, score) in results {
            let speaker = by_speaker.entry(speaker_id).or_default();
            speaker.name = name;
            speaker.scores.push(decimal_score(score)?);
            speaker.rounds.insert(round_id);
        }

        let mut rows = Vec::with_capacity(by_speaker.len());
        for (speaker_id, mut speaker) in by_speaker {
            if let Some(limit) = tournament.exclude_from_speaker_standings_after
                && limit >= 0
                && round_count - speaker.rounds.len() as i64 > limit
            {
                continue;
            }

            speaker.scores.sort();
            let total = speaker.scores.iter().sum::<Decimal>();
            let count = Decimal::from(speaker.scores.len() as u64);
            let avg = total / count;
            let variance = speaker
                .scores
                .iter()
                .map(|score| {
                    let difference = *score - avg;
                    difference * difference
                })
                .sum::<Decimal>()
                / count;
            let std_dev = variance.sqrt().ok_or_else(|| {
                invalid_metric("Could not compute speaker standard deviation")
            })?;
            let values = metrics
                .iter()
                .map(|metric| match metric {
                    SpeakerMetric::Avg => avg,
                    SpeakerMetric::Total => total,
                    SpeakerMetric::StdDev => std_dev,
                })
                .collect();
            rows.push(SpeakerStandingRow {
                speaker_id,
                name: speaker.name,
                rank: 0,
                tied: false,
                metrics: values,
            });
        }

        let compare_metrics =
            |a: &SpeakerStandingRow, b: &SpeakerStandingRow| {
                metrics
                    .iter()
                    .zip(a.metrics.iter().zip(&b.metrics))
                    .map(|(metric, (a, b))| match metric {
                        SpeakerMetric::StdDev => a.cmp(b),
                        SpeakerMetric::Avg | SpeakerMetric::Total => b.cmp(a),
                    })
                    .find(|order| *order != Ordering::Equal)
                    .unwrap_or(Ordering::Equal)
            };
        rows.sort_by(|a, b| {
            compare_metrics(a, b)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.speaker_id.cmp(&b.speaker_id))
        });

        let mut start = 0;
        while start < rows.len() {
            let mut end = start + 1;
            while end < rows.len()
                && compare_metrics(&rows[start], &rows[end]) == Ordering::Equal
            {
                end += 1;
            }
            for row in &mut rows[start..end] {
                row.rank = start + 1;
                row.tied = end - start > 1;
            }
            start = end;
        }

        Ok(Self { metrics, rows })
    }

    /// Read saved metrics and ranks, populating the cache for older tournaments
    /// on their first visit. Mutation handlers refresh it alongside team standings.
    pub fn fetch(
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> QueryResult<Self> {
        let tournament = crate::schema::tournaments::table
            .find(tid)
            .first::<Tournament>(conn)?;
        let metrics: Vec<SpeakerMetric> =
            serde_json::from_str(&tournament.speaker_standings_metrics)
                .map_err(|err| {
                    diesel::result::Error::DeserializationError(Box::new(err))
                })?;
        let rankings = speaker_standings::table
            .inner_join(speakers::table)
            .filter(speaker_standings::tournament_id.eq(tid))
            .filter(speakers::tournament_id.eq(tid))
            .order((
                speaker_standings::rank.asc(),
                speakers::name.asc(),
                speakers::id.asc(),
            ))
            .select((speakers::id, speakers::name, speaker_standings::rank))
            .load::<(String, String, i64)>(conn)?;
        if rankings.is_empty() {
            return Self::recompute_and_save(tid, conn);
        }
        let saved_metrics = speaker_metrics::table
            .filter(speaker_metrics::tournament_id.eq(tid))
            .select((
                speaker_metrics::speaker_id,
                speaker_metrics::metric_kind,
                speaker_metrics::metric_value,
            ))
            .load::<(String, String, f32)>(conn)?;
        let mut by_speaker: HashMap<String, Vec<(SpeakerMetric, Decimal)>> =
            HashMap::new();
        for (speaker_id, kind, value) in saved_metrics {
            let kind = serde_json::from_str(&kind).map_err(|err| {
                diesel::result::Error::DeserializationError(Box::new(err))
            })?;
            by_speaker
                .entry(speaker_id)
                .or_default()
                .push((kind, decimal_score(value)?));
        }
        let mut rank_counts = HashMap::new();
        for (_, _, rank) in &rankings {
            *rank_counts.entry(*rank).or_insert(0) += 1;
        }
        let mut rows = Vec::with_capacity(rankings.len());
        for (speaker_id, name, rank) in rankings {
            let values: Option<Vec<_>> = metrics
                .iter()
                .map(|metric| {
                    by_speaker
                        .get(&speaker_id)?
                        .iter()
                        .find(|(kind, _)| kind == metric)
                        .map(|(_, value)| *value)
                })
                .collect();
            let Some(values) = values else {
                return Self::recompute_and_save(tid, conn);
            };
            rows.push(SpeakerStandingRow {
                speaker_id,
                name,
                rank: usize::try_from(rank).map_err(|_| {
                    invalid_metric("Invalid saved speaker rank")
                })?,
                tied: rank_counts[&rank] > 1,
                metrics: values,
            });
        }
        Ok(Self { metrics, rows })
    }

    pub fn save(
        &self,
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> QueryResult<()> {
        diesel::delete(
            speaker_metrics::table
                .filter(speaker_metrics::tournament_id.eq(tid)),
        )
        .execute(conn)?;
        diesel::delete(
            speaker_standings::table
                .filter(speaker_standings::tournament_id.eq(tid)),
        )
        .execute(conn)?;
        for row in &self.rows {
            for (kind, value) in self.metrics.iter().zip(&row.metrics) {
                // Match team standings' existing Float storage at the DB boundary;
                // aggregation, metric calculations, and ranking use Decimal.
                let value = value
                    .to_f32()
                    .ok_or_else(|| invalid_metric("Invalid speaker metric"))?;
                diesel::insert_into(speaker_metrics::table)
                    .values((
                        speaker_metrics::id.eq(Uuid::now_v7().to_string()),
                        speaker_metrics::tournament_id.eq(tid),
                        speaker_metrics::speaker_id.eq(&row.speaker_id),
                        speaker_metrics::metric_kind.eq(serde_json::to_string(
                            kind,
                        )
                        .map_err(|err| {
                            diesel::result::Error::SerializationError(Box::new(
                                err,
                            ))
                        })?),
                        speaker_metrics::metric_value.eq(value),
                    ))
                    .execute(conn)?;
            }
            diesel::insert_into(speaker_standings::table)
                .values((
                    speaker_standings::id.eq(Uuid::now_v7().to_string()),
                    speaker_standings::tournament_id.eq(tid),
                    speaker_standings::speaker_id.eq(&row.speaker_id),
                    speaker_standings::rank.eq(row.rank as i64),
                ))
                .execute(conn)?;
        }
        Ok(())
    }

    fn recompute_and_save(
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> QueryResult<Self> {
        let standings = Self::recompute(tid, conn)?;
        standings.save(tid, conn)?;
        Ok(standings)
    }
}

pub fn refresh_saved_speaker_standings(
    tid: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> QueryResult<()> {
    SpeakerStandings::recompute(tid, conn)?.save(tid, conn)
}

fn decimal_score(value: f32) -> QueryResult<Decimal> {
    Decimal::from_f32_retain(value)
        .ok_or_else(|| invalid_metric("Invalid speaker score"))
}

fn invalid_metric(message: &str) -> diesel::result::Error {
    diesel::result::Error::DeserializationError(
        std::io::Error::new(std::io::ErrorKind::InvalidData, message).into(),
    )
}
