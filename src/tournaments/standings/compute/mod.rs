use std::collections::HashMap;

use diesel::prelude::*;
use diesel::{connection::LoadConnection, sqlite::Sqlite};
use itertools::Itertools;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::{
    tournament_team_metrics, tournament_team_standings, tournament_teams,
    tournaments,
};
use crate::tournaments::Tournament;
use crate::tournaments::config::{
    PullupMetric, RankableTeamMetric, UnrankableTeamMetric,
};
use crate::tournaments::standings::compute::metrics::atss;
use crate::tournaments::standings::compute::metrics::draw_strength::draw_strength_of_teams;
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

    pub fn recompute(
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Self {
        let tournament = tournaments::table
            .filter(tournaments::id.eq(tid))
            .first::<Tournament>(conn)
            .unwrap();

        let metrics: Vec<RankableTeamMetric> = tournament
            .metrics()
            .into_iter()
            .sorted_by_key(RankableTeamMetric::sort_order_for_comp)
            .collect();
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
                    // these always needs to be manually computed
                    PullupMetric::FewerPreviousPullups
                    | PullupMetric::LowestDsRank => true,
                    PullupMetric::LowestDsSpeaks => metrics.iter().any(|m| {
                        matches!(m, RankableTeamMetric::DrawStrengthBySpeaks)
                    }),
                })
        };

        let mut ranked_metrics_of_team: HashMap<
            String,
            Vec<(RankableTeamMetric, Decimal)>,
        > = HashMap::new();

        for metric in &metrics {
            let val2merge: HashMap<String, rust_decimal::Decimal> = match metric
            {
                RankableTeamMetric::Wins => points_of_team((tid,), conn)
                    .into_iter()
                    .map(|(k, v)| (k, rust_decimal::Decimal::from(v)))
                    .collect(),
                RankableTeamMetric::NTimesAchieved(t) => {
                    times_team_achieved_p_points((*t, tid), conn)
                        .into_iter()
                        .map(|(k, v)| (k, rust_decimal::Decimal::from(v)))
                        .collect()
                }
                RankableTeamMetric::TotalSpeakerScore => {
                    total_speaker_score_of_team((tid,), conn)
                }
                RankableTeamMetric::DrawStrengthByWins => {
                    // todo: store metrics separately
                    let team_points = ranked_metrics_of_team
                        .iter()
                        .map(|(team, metrics)| {
                            (
                                team.clone(),
                                metrics
                                    .iter()
                                    .find_map(|(metric, value)| match metric {
                                        RankableTeamMetric::Wins => {
                                            Some(value.clone())
                                        }
                                        _ => None,
                                    })
                                    // todo: unwrap_or(Zero) (?)
                                    .unwrap(),
                            )
                        })
                        .collect();
                    draw_strength_of_teams((tid, team_points), conn)
                        .into_iter()
                        .map(|(k, v)| (k, rust_decimal::Decimal::from(v)))
                        .collect()
                }
                RankableTeamMetric::AverageTotalSpeakerScore => {
                    let tss = ranked_metrics_of_team
                        .iter()
                        .map(|(team, metrics)| {
                            (
                                team.clone(),
                                metrics
                                    .iter()
                                    .find_map(|(metric, value)| match metric {
                                        RankableTeamMetric::TotalSpeakerScore => {
                                            Some(value.clone())
                                        }
                                        _ => None,
                                    })
                                    // todo: unwrap_or(Zero) (?)
                                    .unwrap(),
                            )
                        })
                        .collect();
                    atss::atss((tid, tss), conn)
                }
                RankableTeamMetric::Ballots
                | RankableTeamMetric::DrawStrengthBySpeaks => todo!(),
            };

            for (k, v) in val2merge {
                ranked_metrics_of_team
                    .entry(k)
                    .and_modify(
                        |vals: &mut Vec<(RankableTeamMetric, Decimal)>| {
                            vals.push((*metric, v))
                        },
                    )
                    .or_insert(vec![(*metric, v)]);
            }
        }

        let mut teams = tournament_teams::table
            .filter(tournament_teams::tournament_id.eq(tid))
            .load::<Team>(conn)
            .unwrap();

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

            dbg!(&team.name, &map);

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

        let teams: HashMap<_, _> = tournament_teams::table
            .filter(tournament_teams::tournament_id.eq(tid))
            .load::<Team>(conn)
            .unwrap()
            .into_iter()
            .map(|team| (team.id.clone(), team))
            .collect();

        let metrics = tournament.metrics();

        let rankings = tournament_team_standings::table
            .filter(tournament_team_standings::tournament_id.eq(tid))
            .select((
                tournament_team_standings::team_id,
                tournament_team_standings::rank,
            ))
            .order_by(tournament_team_standings::rank.asc())
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

        let team_metrics = tournament_team_metrics::table
            .filter(tournament_team_metrics::tournament_id.eq(tid))
            .select((
                tournament_team_metrics::team_id,
                tournament_team_metrics::metric_kind,
                tournament_team_metrics::metric_value,
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
                    let pos =
                        metrics.iter().position(|t| *t == rankable_team_metric);

                    match pos {
                        Some(pos) => {
                            metrics_of_team
                                .entry(team)
                                .and_modify(|metrics: &mut Vec<_>| {
                                    if metrics.len() - 1 <= pos {
                                        metrics
                                            .push((rankable_team_metric, value))
                                    } else {
                                        metrics.insert(
                                            pos,
                                            (rankable_team_metric, value),
                                        )
                                    }
                                })
                                .or_insert(vec![(rankable_team_metric, value)]);
                        }
                        None => {
                            continue;
                        }
                    }
                }
                SerializableMetric::NonRankable(unrankable_team_metric) => {
                    non_ranking_metrics
                        .insert((team, unrankable_team_metric), value);
                }
            }
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
                tournament_team_metrics::table
                    .filter(tournament_team_metrics::tournament_id.eq(tid)),
            )
            .execute(conn)
            .unwrap();

            diesel::delete(
                tournament_team_standings::table
                    .filter(tournament_team_standings::tournament_id.eq(tid)),
            )
            .execute(conn)
            .unwrap();
        };

        let _save_ranked_metrics = {
            let mut records = Vec::new();

            for (team, metric) in &self.ranked_metrics_of_team {
                for (kind, value) in metric {
                    records.push((
                        tournament_team_metrics::id
                            .eq(Uuid::now_v7().to_string()),
                        tournament_team_metrics::tournament_id.eq(tid),
                        tournament_team_metrics::team_id.eq(team),
                        tournament_team_metrics::metric_kind
                            .eq(serde_json::to_string(kind).unwrap()),
                        tournament_team_metrics::metric_value
                            .eq(value.to_f32().unwrap()),
                    ))
                }
            }

            diesel::insert_into(tournament_team_metrics::table)
                .values(records)
                .execute(conn)
                .unwrap();
        };

        let _save_unranked_metrics = {
            let mut records = Vec::new();

            for ((team, metric_kind), value) in &self.pullup_metrics {
                records.push((
                    tournament_team_metrics::id.eq(Uuid::now_v7().to_string()),
                    tournament_team_metrics::tournament_id.eq(tid),
                    tournament_team_metrics::team_id.eq(team),
                    tournament_team_metrics::metric_kind
                        .eq(serde_json::to_string(metric_kind).unwrap()),
                    tournament_team_metrics::metric_value
                        .eq(value.to_f32().unwrap()),
                ));
            }

            diesel::insert_into(tournament_team_metrics::table)
                .values(records)
                .execute(conn)
                .unwrap();
        };

        let _save_rankings = {
            let mut records = Vec::new();

            for (team, rank) in &self.rank_of_team {
                records.push((
                    tournament_team_standings::id
                        .eq(Uuid::now_v7().to_string()),
                    tournament_team_standings::tournament_id.eq(tid),
                    tournament_team_standings::team_id.eq(team.clone()),
                    tournament_team_standings::rank.eq(rank),
                ));
            }

            diesel::insert_into(tournament_team_standings::table)
                .values(records)
                .execute(conn)
                .unwrap();
        };

        Ok(())
    }
}
