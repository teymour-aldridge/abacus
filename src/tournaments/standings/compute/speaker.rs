//! Speaker standings from released, completed preliminary-round aggregates.

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

use diesel::{
    connection::LoadConnection, prelude::*, sql_types::Double, sqlite::Sqlite,
};

use crate::{
    schema::{
        agg_speaker_results_of_debate as scores, debates, rounds, speakers,
    },
    tournaments::{Tournament, config::SpeakerMetric},
    util_resp::FailureResponse,
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
    pub metrics: Vec<f64>,
}

#[derive(Default)]
struct SpeakerScores {
    name: String,
    scores: Vec<f64>,
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
    pub fn compute(
        tournament: &Tournament,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<Self, FailureResponse> {
        let metrics: Vec<SpeakerMetric> =
            serde_json::from_str(&tournament.speaker_standings_metrics)
                .map_err(|err| {
                    tracing::error!(
                        tournament_id = %tournament.id,
                        "Invalid speaker standings metrics: {err}"
                    );
                    FailureResponse::ServerError(())
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
                // SQLite stores REAL as doubles. Avoid narrowing them to the
                // schema's f32 before computing metrics and comparing ties.
                diesel::dsl::sql::<Double>(
                    "agg_speaker_results_of_debate.score",
                ),
            ))
            .load::<(String, String, String, f64)>(conn)?;

        let mut by_speaker: HashMap<String, SpeakerScores> = HashMap::new();
        for (speaker_id, name, round_id, score) in results {
            let speaker = by_speaker.entry(speaker_id).or_default();
            speaker.name = name;
            speaker.scores.push(score);
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

            // Give identical sets of scores identical floating-point sums,
            // regardless of database row order or which round came first.
            speaker.scores.sort_by(f64::total_cmp);
            let total = speaker.scores.iter().sum::<f64>();
            let avg = total / speaker.scores.len() as f64;
            let std_dev = (speaker
                .scores
                .iter()
                .map(|score| (score - avg).powi(2))
                .sum::<f64>()
                / speaker.scores.len() as f64)
                .sqrt();
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
                        SpeakerMetric::StdDev => a.total_cmp(b),
                        SpeakerMetric::Avg | SpeakerMetric::Total => {
                            b.total_cmp(a)
                        }
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
}
