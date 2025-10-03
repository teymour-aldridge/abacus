use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum PullupMetric {
    #[serde(rename = "lowest_rank")]
    LowestRank,
    #[serde(rename = "highest_rank")]
    HighestRank,
    #[serde(rename = "random")]
    Random,
    #[serde(rename = "fewer_previous_pullups")]
    FewerPreviousPullups,
    #[serde(rename = "lowest_ds_rank")]
    LowestDsRank,
    #[serde(rename = "lowest_ds_speaks")]
    LowestDsSpeaks,
}

impl std::fmt::Display for PullupMetric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("prefer teams: ")?;
        f.write_str(match self {
            PullupMetric::LowestRank => "lowest rank",
            PullupMetric::HighestRank => "highest rank",
            PullupMetric::Random => "random",
            PullupMetric::FewerPreviousPullups => "fewer previous pullups",
            PullupMetric::LowestDsRank => "lowest ds rank",
            PullupMetric::LowestDsSpeaks => "lowest ds speaks",
        })
    }
}

#[derive(Serialize, Deserialize, Copy, Clone, PartialEq, Eq)]
/// A metric upon which teams can be ranked. Note that some metrics _cannot_ be
/// used to rank teams (for example, draw strength by rank) as this turns into a
/// recursive mess.
pub enum RankableTeamMetric {
    #[serde(rename = "wins")]
    Wins,
    /// The total number of ballots in favour of this team.
    #[serde(rename = "ballots")]
    Ballots,
    /// The total number of points the teams this team has debated against have
    /// achieved.
    #[serde(rename = "draw_strength_by_wins")]
    DrawStrengthByWins,
    /// The sum of speaks of all the teams that the given team has faced.
    #[serde(rename = "draw_strength_by_speaks")]
    DrawStrengthBySpeaks,
    /// The total number of times a team has achieved this many points.
    #[serde(rename = "n_times_achieved")]
    NTimesAchieved(u8),
    /// The total speaker score of all the speakers on the team.
    #[serde(rename = "total_speaker_score")]
    TotalSpeakerScore,
    /// The average total speaker score.
    #[serde(rename = "avg_total_speaker_score")]
    AverageTotalSpeakerScore,
}

impl std::fmt::Display for RankableTeamMetric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RankableTeamMetric::Wins => "#points",
            RankableTeamMetric::Ballots => "#ballots",
            RankableTeamMetric::DrawStrengthByWins => "draw strength by wins",
            RankableTeamMetric::DrawStrengthBySpeaks => {
                "draw strength by (average) total speaker score"
            }
            RankableTeamMetric::NTimesAchieved(points) => {
                return write!(f, "#times achieved {points} points");
            }
            RankableTeamMetric::TotalSpeakerScore => "total speaker score",
            RankableTeamMetric::AverageTotalSpeakerScore => {
                "avg total speaker score"
            }
        })
    }
}

#[derive(Serialize, Deserialize, Hash, Eq, PartialEq)]
pub enum UnrankableTeamMetric {
    #[serde(rename = "draw_strength_by_rank")]
    DrawStrengthByRank,
}

#[derive(Serialize, Deserialize)]
pub enum SpeakerMetric {
    StdDev,
    /// Average
    Avg,
    Total,
}
