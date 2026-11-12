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

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::connection::SimpleConnection;
    use diesel_migrations::MigrationHarness;

    fn fixture() -> (SqliteConnection, Tournament) {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.run_pending_migrations(crate::MIGRATIONS).unwrap();
        conn.batch_execute(r#"
            INSERT INTO tournaments (
                id, name, abbrv, slug, created_at, teams_per_side,
                substantive_speakers, pool_ballot_setup, elim_ballot_setup,
                institution_penalty, history_penalty, pullup_metrics,
                repeat_pullup_penalty, team_standings_metrics, speaker_standings_metrics
            ) VALUES
                ('t', 'Test', 'T', 'test', CURRENT_TIMESTAMP, 1, 3,
                 'consensus', 'consensus', 0, 0, '[]', 0, '[]', '["Avg","StdDev","Total"]'),
                ('other', 'Other', 'O', 'other', CURRENT_TIMESTAMP, 1, 3,
                 'consensus', 'consensus', 0, 0, '[]', 0, '[]', '[]');
            INSERT INTO speakers (id, tournament_id, name, email, private_url) VALUES
                ('a', 't', 'Alice', '', 'a'),
                ('b', 't', 'Bob', '', 'b'),
                ('c', 't', 'Cora', '', 'c'),
                ('d', 't', 'Dan', '', 'd'),
                ('none', 't', 'No scores', '', 'none'),
                ('foreign', 'other', 'Foreign', '', 'foreign');
            INSERT INTO teams (id, tournament_id, name, number) VALUES
                ('team', 't', 'Team', 1), ('foreign-team', 'other', 'Other', 1);
            INSERT INTO rounds (id, tournament_id, seq, name, kind, completed, draw_status) VALUES
                ('r1', 't', 1, 'R1', 'P', 1, 'released_full'),
                ('r2', 't', 2, 'R2', 'P', 1, 'released_full'),
                ('future', 't', 3, 'Future', 'P', 0, 'released_full'),
                ('hidden', 't', 4, 'Hidden', 'P', 1, 'draft'),
                ('elim', 't', 5, 'Final', 'E', 1, 'released_full'),
                ('foreign-round', 'other', 1, 'Other', 'P', 1, 'released_full');
            INSERT INTO debates (id, tournament_id, round_id, number, status) VALUES
                ('d1', 't', 'r1', 1, 'confirmed'),
                ('d2', 't', 'r2', 1, 'confirmed'),
                ('df', 't', 'future', 1, 'confirmed'),
                ('dh', 't', 'hidden', 1, 'confirmed'),
                ('de', 't', 'elim', 1, 'confirmed'),
                ('do', 'other', 'foreign-round', 1, 'confirmed'),
                ('wrong-round', 't', 'foreign-round', 2, 'confirmed'),
                ('wrong-debate', 'other', 'r1', 2, 'confirmed');
            INSERT INTO agg_speaker_results_of_debate
                (id, tournament_id, debate_id, speaker_id, team_id, position, score) VALUES
                ('a1', 't', 'd1', 'a', 'team', 0, 70),
                ('a2', 't', 'd2', 'a', 'team', 0, 90),
                ('b1', 't', 'd1', 'b', 'team', 1, 80),
                ('b2', 't', 'd2', 'b', 'team', 1, 80),
                ('c1', 't', 'd1', 'c', 'team', 2, 80),
                ('c2', 't', 'd2', 'c', 'team', 2, 80),
                ('d-score', 't', 'd1', 'd', 'team', 0, 75);
        "#).unwrap();
        let tournament = crate::schema::tournaments::table
            .find("t")
            .first(&mut conn)
            .unwrap();
        (conn, tournament)
    }

    fn summary(standings: &SpeakerStandings) -> Vec<(&str, usize, bool)> {
        standings
            .rows
            .iter()
            .map(|row| (row.speaker_id.as_str(), row.rank, row.tied))
            .collect()
    }

    #[test]
    fn configured_metrics_population_deviation_and_competition_ties() {
        let (mut conn, mut tournament) = fixture();
        let standings =
            SpeakerStandings::compute(&tournament, &mut conn).unwrap();
        assert_eq!(
            summary(&standings),
            vec![
                ("b", 1, true),
                ("c", 1, true),
                ("a", 3, false),
                ("d", 4, false)
            ]
        );
        assert_eq!(standings.rows[0].metrics, vec![80.0, 0.0, 160.0]);
        assert_eq!(standings.rows[2].metrics, vec![80.0, 10.0, 160.0]);
        assert_eq!(standings.rows[3].metrics, vec![75.0, 0.0, 75.0]);

        // A lower total beats a higher one when average is configured first.
        conn.batch_execute("UPDATE agg_speaker_results_of_debate SET score = 100 WHERE id = 'd-score';").unwrap();
        let standings =
            SpeakerStandings::compute(&tournament, &mut conn).unwrap();
        assert_eq!(standings.rows[0].speaker_id, "d");
        tournament.speaker_standings_metrics = r#"["Total","Avg"]"#.into();
        let standings =
            SpeakerStandings::compute(&tournament, &mut conn).unwrap();
        assert_eq!(
            summary(&standings),
            vec![
                ("a", 1, true),
                ("b", 1, true),
                ("c", 1, true),
                ("d", 4, false)
            ]
        );
        assert_eq!(standings.rows[0].metrics, vec![160.0, 80.0]);

        tournament.speaker_standings_metrics = r#"["StdDev"]"#.into();
        let standings =
            SpeakerStandings::compute(&tournament, &mut conn).unwrap();
        assert_eq!(standings.rows.last().unwrap().speaker_id, "a");
        assert_eq!(standings.rows.last().unwrap().rank, 4);
    }

    #[test]
    fn excludes_unreleased_incomplete_elimination_reply_null_and_foreign_scores()
     {
        let (mut conn, tournament) = fixture();
        conn.batch_execute(r#"
            INSERT INTO agg_speaker_results_of_debate
                (id, tournament_id, debate_id, speaker_id, team_id, position, score) VALUES
                ('reply', 't', 'd1', 'a', 'team', 3, 1000),
                ('later-reply', 't', 'd1', 'a', 'team', 4, 1000),
                ('future-score', 't', 'df', 'a', 'team', 0, 1000),
                ('hidden-score', 't', 'dh', 'a', 'team', 0, 1000),
                ('elim-score', 't', 'de', 'a', 'team', 0, 1000),
                ('null', 't', 'd1', 'a', 'team', 0, NULL),
                ('none-null', 't', 'd1', 'none', 'team', 0, NULL),
                ('none-reply', 't', 'd1', 'none', 'team', 3, 1000),
                ('foreign-score', 'other', 'do', 'foreign', 'foreign-team', 0, 1000),
                ('foreign-aggregate', 'other', 'd1', 'a', 'team', 0, 1000),
                ('foreign-speaker', 't', 'd1', 'foreign', 'team', 0, 1000),
                ('foreign-round-score', 't', 'wrong-round', 'a', 'team', 0, 1000),
                ('foreign-debate-score', 't', 'wrong-debate', 'a', 'team', 0, 1000);
        "#).unwrap();
        let standings =
            SpeakerStandings::compute(&tournament, &mut conn).unwrap();
        assert_eq!(
            summary(&standings),
            vec![
                ("b", 1, true),
                ("c", 1, true),
                ("a", 3, false),
                ("d", 4, false)
            ]
        );
        assert_eq!(standings.rows[2].metrics, vec![80.0, 10.0, 160.0]);
    }

    #[test]
    fn missed_round_limit_uses_distinct_rounds_and_can_be_disabled() {
        let (mut conn, mut tournament) = fixture();
        // A second score in the same round must not hide a missed round.
        conn.batch_execute(r#"
            INSERT INTO agg_speaker_results_of_debate
                (id, tournament_id, debate_id, speaker_id, team_id, position, score)
            VALUES ('d-duplicate', 't', 'd1', 'd', 'team', 1, 75),
                   ('d-null', 't', 'd2', 'd', 'team', 0, NULL);
        "#).unwrap();
        for limit in [None, Some(-1), Some(1)] {
            tournament.exclude_from_speaker_standings_after = limit;
            let standings =
                SpeakerStandings::compute(&tournament, &mut conn).unwrap();
            assert_eq!(standings.rows.len(), 4);
        }
        tournament.exclude_from_speaker_standings_after = Some(0);
        let standings =
            SpeakerStandings::compute(&tournament, &mut conn).unwrap();
        assert_eq!(standings.rows.len(), 3);
        assert!(standings.rows.iter().all(|row| row.speaker_id != "d"));
    }

    #[test]
    fn empty_tournament_and_invalid_metric_configuration() {
        let (mut conn, mut tournament) = fixture();
        conn.batch_execute("DELETE FROM agg_speaker_results_of_debate;")
            .unwrap();
        let standings =
            SpeakerStandings::compute(&tournament, &mut conn).unwrap();
        assert!(standings.rows.is_empty());
        assert_eq!(standings.metrics.len(), 3);
        tournament.speaker_standings_metrics = r#"["invalid"]"#.into();
        assert!(matches!(
            SpeakerStandings::compute(&tournament, &mut conn),
            Err(FailureResponse::ServerError(()))
        ));
    }
}
