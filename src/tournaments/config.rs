use serde::{Deserialize, Serialize};

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
