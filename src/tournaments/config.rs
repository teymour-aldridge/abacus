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

#[derive(Serialize, Deserialize)]
pub enum SpeakerMetric {
    StdDev,
    /// Average
    Avg,
    Total,
}
