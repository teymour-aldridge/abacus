use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum PullupMetric {
    #[serde(rename = "lowest_rank")]
    LowestRank,
    #[serde(rename = "highest_rank")]
    HighestRank,
    #[serde(rename = "randon")]
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

#[derive(Serialize, Deserialize)]
pub enum TeamMetric {
    Wins,
    /// The total number of ballots in favour of this team.
    Ballots,
    /// The total number of points the teams this team has debated against have
    /// achieved.
    DrawStrengthByWins,
    /// The total number of times a team has achieved this many points.
    NTimesAchieved(u8),
    /// The total speaker score of all the speakers on the team.
    TotalSpeakerScore,
    /// The average total speaker score.
    AverageTotalSpeakerScore,
}

impl std::fmt::Display for TeamMetric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            TeamMetric::Wins => "#points",
            TeamMetric::Ballots => "#ballots",
            TeamMetric::DrawStrengthByWins => "draw strength by wins",
            TeamMetric::NTimesAchieved(points) => {
                return write!(f, "#times achieved {points} points");
            }
            TeamMetric::TotalSpeakerScore => "total speaker score",
            TeamMetric::AverageTotalSpeakerScore => "avg total speaker score",
        })
    }
}

#[derive(Serialize, Deserialize)]
pub enum SpeakerMetric {
    StdDev,
    /// Average
    Avg,
    Total,
}
