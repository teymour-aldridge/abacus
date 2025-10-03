use std::fmt;

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};

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

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
/// A metric upon which teams can be ranked. Note that some metrics _cannot_ be
/// used to rank teams (for example, draw strength by rank) as this turns into a
/// recursive mess.
pub enum RankableTeamMetric {
    Wins,
    /// The total number of ballots in favour of this team.
    Ballots,
    /// The total number of points the teams this team has debated against have
    /// achieved.
    DrawStrengthByWins,
    /// The sum of speaks of all the teams that the given team has faced.
    DrawStrengthBySpeaks,
    /// The total number of times a team has achieved this many points.
    NTimesAchieved(u8),
    /// The total speaker score of all the speakers on the team.
    TotalSpeakerScore,
    /// The average total speaker score.
    AverageTotalSpeakerScore,
}

impl Serialize for RankableTeamMetric {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            RankableTeamMetric::Wins => serializer.serialize_str("wins"),
            RankableTeamMetric::Ballots => serializer.serialize_str("ballots"),
            RankableTeamMetric::DrawStrengthByWins => {
                serializer.serialize_str("draw_strength_by_wins")
            }
            RankableTeamMetric::DrawStrengthBySpeaks => {
                serializer.serialize_str("draw_strength_by_speaks")
            }
            RankableTeamMetric::NTimesAchieved(n) => {
                // Dynamically create the string here
                let s = format!("n_times_achieved_{}", n);
                serializer.serialize_str(&s)
            }
            RankableTeamMetric::TotalSpeakerScore => {
                serializer.serialize_str("total_speaker_score")
            }
            RankableTeamMetric::AverageTotalSpeakerScore => {
                serializer.serialize_str("avg_total_speaker_score")
            }
        }
    }
}

impl<'de> Deserialize<'de> for RankableTeamMetric {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RankableTeamMetricVisitor;

        impl<'de> Visitor<'de> for RankableTeamMetricVisitor {
            type Value = RankableTeamMetric;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter
                    .write_str("a string representing a RankableTeamMetric")
            }

            fn visit_str<E>(self, value: &str) -> Result<RankableTeamMetric, E>
            where
                E: de::Error,
            {
                match value {
                    "wins" => Ok(RankableTeamMetric::Wins),
                    "ballots" => Ok(RankableTeamMetric::Ballots),
                    "draw_strength_by_wins" => {
                        Ok(RankableTeamMetric::DrawStrengthByWins)
                    }
                    "draw_strength_by_speaks" => {
                        Ok(RankableTeamMetric::DrawStrengthBySpeaks)
                    }
                    "total_speaker_score" => {
                        Ok(RankableTeamMetric::TotalSpeakerScore)
                    }
                    "avg_total_speaker_score" => {
                        Ok(RankableTeamMetric::AverageTotalSpeakerScore)
                    }
                    s if s.starts_with("n_times_achieved_") => {
                        // Parse the number from the end of the string
                        let num_str = s.trim_start_matches("n_times_achieved_");
                        match num_str.parse::<u8>() {
                            Ok(n) => Ok(RankableTeamMetric::NTimesAchieved(n)),
                            Err(_) => Err(E::custom(format!(
                                "invalid number in metric: {}",
                                s
                            ))),
                        }
                    }
                    _ => Err(E::unknown_variant(
                        value,
                        &["wins", "ballots", "n_times_achieved_..."],
                    )),
                }
            }
        }
        deserializer.deserialize_str(RankableTeamMetricVisitor)
    }
}

#[cfg(test)]
#[test]
fn test_all_variants_roundtrip() {
    let metrics_to_test = vec![
        RankableTeamMetric::Wins,
        RankableTeamMetric::Ballots,
        RankableTeamMetric::DrawStrengthByWins,
        RankableTeamMetric::DrawStrengthBySpeaks,
        RankableTeamMetric::NTimesAchieved(0),
        RankableTeamMetric::NTimesAchieved(3),
        RankableTeamMetric::NTimesAchieved(255), // Max u8 value
        RankableTeamMetric::TotalSpeakerScore,
        RankableTeamMetric::AverageTotalSpeakerScore,
    ];

    for original_metric in metrics_to_test {
        let serialized_metric = serde_json::to_string(&original_metric)
            .expect("Failed to serialize metric");

        let deserialized_metric: RankableTeamMetric =
            serde_json::from_str(&serialized_metric)
                .expect("Failed to deserialize metric");

        assert_eq!(
            original_metric, deserialized_metric,
            "Round trip failed for {:?}",
            original_metric
        );
    }
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
