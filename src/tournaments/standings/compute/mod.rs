use std::{cell::OnceCell, collections::HashMap};

use diesel::prelude::*;
use diesel::{connection::LoadConnection, sqlite::Sqlite};
use itertools::Itertools;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::{team_metrics, team_standings, teams, tournaments};
use crate::tournaments::Tournament;
use crate::tournaments::config::{
    PullupMetric, RankableTeamMetric, UnrankableTeamMetric,
};
use crate::tournaments::standings::compute::metrics::atss;
use crate::tournaments::standings::compute::metrics::ballots::ballot_points_of_team;
use crate::tournaments::standings::compute::metrics::draw_strength::{
    draw_strength_of_teams, teams_of_completed_debates,
};
use crate::tournaments::standings::compute::metrics::n_times_specific_result::times_team_achieved_p_points;
use crate::tournaments::standings::compute::metrics::points::points_of_team;
use crate::tournaments::standings::compute::metrics::tss::total_speaker_score_of_team;
use crate::tournaments::teams::Team;
use rust_decimal::Decimal;

pub mod history;
pub mod metrics;

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum SerializableMetric {
    Rankable(RankableTeamMetric),
    NonRankable(UnrankableTeamMetric),
}

/// This struct groups together related items for computing the base metrics
/// which apply to each team.
pub struct TeamStandings {
    pub metrics: Vec<RankableTeamMetric>,
    pub ranked_metrics_of_team:
        HashMap<String, Vec<(RankableTeamMetric, Decimal)>>,
    /// Metrics that are exclusively used for pullups.
    pub pullup_metrics: HashMap<(String, UnrankableTeamMetric), Decimal>,
    /// Stores the teams, in ranked. Note that teams which are tied will occupy
    /// the same list. Teams which are not tied occupy a single list each.
    pub teams_in_rank_order: Vec<Vec<Team>>,
    /// Stores the actual rank of each team (i.e. the position this team is at:
    /// the number of teams which outperformed this team, plus one).
    pub rank_of_team: HashMap<String, i64>,
}

impl TeamStandings {
    pub fn get_ranked_metric_of_team(
        &self,
        team_id: &str,
        metric: RankableTeamMetric,
    ) -> Decimal {
        *self
            .ranked_metrics_of_team
            .get(team_id)
            .unwrap()
            .iter()
            .find_map(
                |(kind, value)| {
                    if *kind == metric { Some(value) } else { None }
                },
            )
            .unwrap()
    }

    pub fn recompute<C: LoadConnection<Backend = Sqlite>>(
        tid: &str,
        conn: &mut C,
    ) -> Self {
        let tournament = tournaments::table
            .filter(tournaments::id.eq(tid))
            .first::<Tournament>(conn)
            .unwrap();

        let metrics = tournament.metrics();
        let pullup_metrics = tournament.pullup_metrics();

        // Some pullup metrics need to be computed ahead of time (for example,
        // draw strength by rank). Others don't.
        let pullup_metrics_to_compute_and_save = {
            pullup_metrics
                .iter()
                .filter(|pullup_metric| match pullup_metric {
                    PullupMetric::LowestRank
                    | PullupMetric::HighestRank
                    | PullupMetric::Random => false,
                    // these always need to be manually computed
                    PullupMetric::FewerPreviousPullups
                    | PullupMetric::LowestDsRank => true,
                    PullupMetric::LowestDsSpeaks => metrics.iter().any(|m| {
                        matches!(m, RankableTeamMetric::DrawStrengthBySpeaks)
                    }),
                })
        };

        let mut teams = teams::table
            .filter(teams::tournament_id.eq(tid))
            .load::<Team>(conn)
            .unwrap();

        // These cells live for this computation only. Dependencies can be used
        // without adding them to the tournament's ranking criteria.
        let points = OnceCell::new();
        let totals = OnceCell::new();
        let averages = OnceCell::new();
        let ballots = OnceCell::new();
        let opponents = OnceCell::new();
        let ds_wins = OnceCell::new();
        let ds_speaks = OnceCell::new();
        let mut position_counts = HashMap::<u8, OnceCell<_>>::new();

        let points = |conn: &mut C| {
            points.get_or_init(|| {
                points_of_team((tid,), conn)
                    .into_iter()
                    .map(|(team, points)| (team, Decimal::from(points)))
                    .collect::<HashMap<_, _>>()
            })
        };
        let totals = |conn: &mut C| {
            totals.get_or_init(|| {
                let mut values = total_speaker_score_of_team((tid,), conn);
                for team in &teams {
                    values.entry(team.id.clone()).or_insert(Decimal::ZERO);
                }
                values
            })
        };

        let mut ranked_metrics_of_team: HashMap<
            String,
            Vec<(RankableTeamMetric, Decimal)>,
        > = HashMap::new();

        // Compute on demand, but append values in configured tie-break order.
        for metric in &metrics {
            let values = match metric {
                RankableTeamMetric::Wins => points(conn),
                RankableTeamMetric::NTimesAchieved(n) => {
                    position_counts.entry(*n).or_default().get_or_init(|| {
                        times_team_achieved_p_points((*n, tid), conn)
                            .into_iter()
                            .map(|(team, count)| (team, Decimal::from(count)))
                            .collect()
                    })
                }
                RankableTeamMetric::TotalSpeakerScore => totals(conn),
                RankableTeamMetric::AverageTotalSpeakerScore => averages
                    .get_or_init(|| {
                        let totals = totals(conn);
                        atss::atss((tid, totals), conn)
                    }),
                RankableTeamMetric::DrawStrengthByWins => {
                    ds_wins.get_or_init(|| {
                        let points = points(conn);
                        let opponents = opponents.get_or_init(|| {
                            teams_of_completed_debates(tid, conn)
                        });
                        draw_strength_of_teams(opponents, points)
                    })
                }
                RankableTeamMetric::DrawStrengthBySpeaks => ds_speaks
                    .get_or_init(|| {
                        let totals = totals(conn);
                        let opponents = opponents.get_or_init(|| {
                            teams_of_completed_debates(tid, conn)
                        });
                        draw_strength_of_teams(opponents, totals)
                    }),
                RankableTeamMetric::Ballots => ballots.get_or_init(|| {
                    ballot_points_of_team((tid,), conn)
                        .into_iter()
                        .map(|(team, points)| (team, Decimal::from(points)))
                        .collect()
                }),
            };

            for (team, value) in values {
                ranked_metrics_of_team
                    .entry(team.clone())
                    .or_default()
                    .push((*metric, *value));
            }
        }

        #[cfg(debug_assertions)]
        {
            for team in &teams {
                debug_assert_ne!(ranked_metrics_of_team.get(&team.id), None);
            }
        }

        let f = |team: &Team| -> _ {
            let map = ranked_metrics_of_team
                .get(&team.id)
                .map(|t| t.iter().map(|(_k, v)| v).collect::<Vec<_>>());

            std::cmp::Reverse(map)
        };

        teams.sort_by_cached_key(f);

        let binding = teams.clone().into_iter().chunk_by(f);
        let teams_grouped_by_rank = binding
            .into_iter()
            .map(|(_key, chunk)| chunk.into_iter().collect::<Vec<_>>())
            .collect::<Vec<_>>();

        let pullup_metrics = {
            let map = HashMap::new();

            for metric in pullup_metrics_to_compute_and_save {
                match metric {
                    PullupMetric::FewerPreviousPullups => todo!(),
                    PullupMetric::LowestDsRank => todo!(),
                    // todo: need to first implement ATSS metric
                    PullupMetric::LowestDsSpeaks => todo!(),
                    _ => unreachable!(),
                };
            }

            map
        };

        let rank_of_team = {
            let mut map = HashMap::new();

            let mut n = 1;

            for teams_of_rank in &teams_grouped_by_rank {
                for team in teams_of_rank {
                    map.insert(team.id.clone(), n as i64);
                }

                // we increase in line with the number of teams we just handled,
                // i.e. if the brackets are
                //
                // [t1, t2]
                // [t3, t4]
                // [t5, t6, t7]
                //
                // then the ranks are
                // =1 : t1, t2
                // =3 : t3, t4
                // =5 : t5, t6, t7
                n += teams_of_rank.len();
            }

            map
        };

        Self {
            metrics,
            ranked_metrics_of_team,
            teams_in_rank_order: teams_grouped_by_rank,
            pullup_metrics,
            rank_of_team,
        }
    }

    /// Load the metrics from the database.
    ///
    /// Note: where these are stale (e.g. because a new round has just been
    /// confirmed) these should instead be computed from scratch using
    /// [`TeamStandings::recompute`].
    pub fn fetch(
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Self {
        // todo: take this as an argument
        let tournament = tournaments::table
            .find(tid)
            .first::<Tournament>(conn)
            .unwrap();

        let teams: HashMap<_, _> = teams::table
            .filter(teams::tournament_id.eq(tid))
            .load::<Team>(conn)
            .unwrap()
            .into_iter()
            .map(|team| (team.id.clone(), team))
            .collect();

        let metrics = tournament.metrics();

        let rankings = team_standings::table
            .filter(team_standings::tournament_id.eq(tid))
            .select((team_standings::team_id, team_standings::rank))
            .order_by(team_standings::rank.asc())
            .load::<(String, i64)>(conn)
            .unwrap();

        let grouped =
            rankings.clone().into_iter().chunk_by(|(_team, rank)| *rank);
        let ranked = grouped
            .into_iter()
            .map(|(_rank, team)| {
                team.into_iter()
                    .map(|(team, _rank)| teams.get(&team).unwrap().clone())
                    .collect::<Vec<_>>()
            })
            .collect();

        let team_metrics = team_metrics::table
            .filter(team_metrics::tournament_id.eq(tid))
            .select((
                team_metrics::team_id,
                team_metrics::metric_kind,
                team_metrics::metric_value,
            ))
            .load::<(String, String, f32)>(conn)
            .unwrap();

        let mut metrics_of_team = HashMap::new();
        let mut non_ranking_metrics = HashMap::new();

        for (team, kind, value) in team_metrics {
            let kind: SerializableMetric = serde_json::from_str(&kind).unwrap();

            let value = Decimal::from_f32_retain(value).unwrap();

            match kind {
                SerializableMetric::Rankable(rankable_team_metric) => {
                    if metrics.contains(&rankable_team_metric) {
                        metrics_of_team
                            .entry(team)
                            .or_insert_with(Vec::new)
                            .push((rankable_team_metric, value));
                    }
                }
                SerializableMetric::NonRankable(unrankable_team_metric) => {
                    non_ranking_metrics
                        .insert((team, unrankable_team_metric), value);
                }
            }
        }

        for metrics_of_team in metrics_of_team.values_mut() {
            metrics_of_team.sort_by_key(|(kind, _)| {
                metrics.iter().position(|needle| needle == kind).unwrap()
            });
        }

        // assert that all team metrics are correctly sorted
        debug_assert!({
            metrics_of_team.iter().all(|(_team, metrics_of_team)| {
                metrics_of_team.iter().is_sorted_by_key(|(kind, _)| {
                    metrics.iter().position(|needle| needle == kind).unwrap()
                })
            })
        });

        Self {
            metrics,
            ranked_metrics_of_team: metrics_of_team,
            teams_in_rank_order: ranked,
            pullup_metrics: non_ranking_metrics,
            rank_of_team: rankings.into_iter().collect::<HashMap<_, _>>(),
        }
    }

    pub fn points_of_team(&self, team: &String) -> Option<i64> {
        self.ranked_metrics_of_team.get(team).and_then(|t| {
            t.iter().find_map(|(kind, value)| match (kind, value) {
                (RankableTeamMetric::Wins, decimal) => {
                    assert!(
                        decimal.is_integer(),
                        "expected decimal {} to be an integer",
                        decimal
                    );
                    Some(decimal.to_i64().unwrap())
                }
                _ => None,
            })
        })
    }

    pub fn save(
        &self,
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<(), diesel::result::Error> {
        let _flush = {
            diesel::delete(
                team_metrics::table.filter(team_metrics::tournament_id.eq(tid)),
            )
            .execute(conn)
            .unwrap();

            diesel::delete(
                team_standings::table
                    .filter(team_standings::tournament_id.eq(tid)),
            )
            .execute(conn)
            .unwrap();
        };

        let _save_ranked_metrics = {
            let mut records = Vec::new();

            for (team, metric) in &self.ranked_metrics_of_team {
                for (kind, value) in metric {
                    records.push((
                        team_metrics::id.eq(Uuid::now_v7().to_string()),
                        team_metrics::tournament_id.eq(tid),
                        team_metrics::team_id.eq(team),
                        team_metrics::metric_kind
                            .eq(serde_json::to_string(kind).unwrap()),
                        team_metrics::metric_value.eq(value.to_f32().unwrap()),
                    ))
                }
            }

            if !records.is_empty() {
                diesel::insert_into(team_metrics::table)
                    .values(records)
                    .execute(conn)
                    .unwrap();
            }
        };

        let _save_unranked_metrics = {
            let mut records = Vec::new();

            for ((team, metric_kind), value) in &self.pullup_metrics {
                records.push((
                    team_metrics::id.eq(Uuid::now_v7().to_string()),
                    team_metrics::tournament_id.eq(tid),
                    team_metrics::team_id.eq(team),
                    team_metrics::metric_kind
                        .eq(serde_json::to_string(metric_kind).unwrap()),
                    team_metrics::metric_value.eq(value.to_f32().unwrap()),
                ));
            }

            if !records.is_empty() {
                diesel::insert_into(team_metrics::table)
                    .values(records)
                    .execute(conn)
                    .unwrap();
            }
        };

        let _save_rankings = {
            let mut records = Vec::new();

            for (team, rank) in &self.rank_of_team {
                records.push((
                    team_standings::id.eq(Uuid::now_v7().to_string()),
                    team_standings::tournament_id.eq(tid),
                    team_standings::team_id.eq(team.clone()),
                    team_standings::rank.eq(rank),
                ));
            }

            if !records.is_empty() {
                diesel::insert_into(team_standings::table)
                    .values(records)
                    .execute(conn)
                    .unwrap();
            }
        };

        Ok(())
    }
}

pub fn refresh_saved_team_standings(
    tid: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> Result<(), diesel::result::Error> {
    TeamStandings::recompute(tid, conn).save(tid, conn)
}
