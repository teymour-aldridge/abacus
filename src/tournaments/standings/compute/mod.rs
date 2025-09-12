use std::collections::HashMap;

use diesel::prelude::*;
use diesel::{Connection, connection::LoadConnection, sqlite::Sqlite};

use crate::schema::{tournament_teams, tournaments};
use crate::tournaments::Tournament;
use crate::tournaments::config::TeamMetric;
use crate::tournaments::standings::compute::metrics::Metric;
use crate::tournaments::standings::compute::metrics::MetricValue;
use crate::tournaments::standings::compute::metrics::n_times_specific_result::NTimesSpecificResultComputer;
use crate::tournaments::standings::compute::metrics::points::TeamPointsComputer;
use crate::tournaments::standings::compute::metrics::tss::TotalTeamSpeakerScoreComputer;
use crate::tournaments::teams::Team;

pub mod history;
pub mod metrics;

pub struct TournamentTeamStandings {
    pub metrics: Vec<TeamMetric>,
    pub metrics_of_team: HashMap<String, Vec<MetricValue>>,
    pub sorted: Vec<Team>,
}

impl TournamentTeamStandings {
    pub fn fetch(
        tid: &str,
        conn: &mut (impl Connection<Backend = Sqlite> + LoadConnection),
    ) -> Self {
        let tournament = tournaments::table
            .filter(tournaments::id.eq(tid))
            .first::<Tournament>(conn)
            .unwrap();

        let metrics: Vec<TeamMetric> =
            serde_json::from_str(&tournament.team_standings_metrics).unwrap();

        let mut metrics_of_team = HashMap::new();

        for metric in &metrics {
            let val2merge = match metric {
                TeamMetric::Wins => {
                    TeamPointsComputer::compute(&TeamPointsComputer, tid, conn)
                }
                TeamMetric::NTimesAchieved(t) => {
                    NTimesSpecificResultComputer(*t).compute(tid, conn)
                }
                TeamMetric::TotalSpeakerScore => {
                    TotalTeamSpeakerScoreComputer::compute(
                        &TotalTeamSpeakerScoreComputer,
                        tid,
                        conn,
                    )
                }
                TeamMetric::DrawStrengthByWins => todo!(),
                TeamMetric::AverageTotalSpeakerScore => todo!(),
                TeamMetric::Ballots => todo!(),
            };

            for (k, v) in val2merge {
                metrics_of_team
                    .entry(k)
                    .and_modify(|vals: &mut Vec<MetricValue>| vals.push(v))
                    .or_insert(vec![v]);
            }
        }

        let mut teams = tournament_teams::table
            .filter(tournament_teams::tournament_id.eq(tid))
            .load::<Team>(conn)
            .unwrap();

        teams.sort_by_cached_key(|team| metrics_of_team.get(&team.id));

        Self {
            metrics,
            metrics_of_team,
            sorted: teams,
        }
    }

    pub fn points_of_team(&self, team: &String) -> Option<i64> {
        self.metrics_of_team.get(team).and_then(|t| t.iter().find_map(|metric| {
            match metric {
                crate::tournaments::standings::compute::metrics::MetricValue::Points(p) => Some(*p),
                _ => None,
            }
        }))
    }
}
