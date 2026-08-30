use std::collections::{HashMap, HashSet};

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use serde::{Deserialize, Serialize};

use crate::{
    schema::{
        break_categories, speaker_categories, speaker_category_implications,
        speaker_category_memberships, speakers_of_team, team_break_eligibility,
        teams,
    },
    tournaments::{categories::team::BreakCategory, teams::Team},
};

#[derive(Queryable, Clone, Debug, Serialize)]
pub struct SpeakerCategory {
    pub id: String,
    pub tournament_id: String,
    pub name: String,
    pub slug: String,
    pub seq: i64,
    pub public: bool,
    pub limit_: i64,
}

#[derive(Queryable, Clone, Debug)]
pub struct SpeakerCategoryMembership {
    pub id: String,
    pub tournament_id: String,
    pub speaker_id: String,
    pub category_id: String,
}

#[derive(Queryable, Clone, Debug)]
pub struct SpeakerCategoryImplication {
    pub id: String,
    pub tournament_id: String,
    pub child_category_id: String,
    pub parent_category_id: String,
}

#[derive(Queryable, Clone, Debug)]
pub struct TeamBreakEligibility {
    pub id: String,
    pub tournament_id: String,
    pub team_id: String,
    pub break_category_id: String,
    pub eligible: bool,
    pub source: String,
    pub explanation: String,
    pub computed_at: chrono::NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EligibilityRule {
    AllowAll,
    IfThresholdMet {
        included_categories: Vec<String>,
        threshold: SpeakerThreshold,
    },
}

impl Default for EligibilityRule {
    fn default() -> Self {
        Self::AllowAll
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SpeakerThreshold {
    All,
    /// At least `count` speakers are in the provided list of categories.
    Geq {
        count: usize,
    },
    /// At least `n-k` speakers are in the provided list of speaker categories.
    GeqNMinusK {
        k: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EligibilityEvaluation {
    pub eligible: bool,
    pub explanation: String,
}

#[derive(Debug, Clone)]
pub struct CategoryIndex {
    direct_memberships: HashMap<String, HashSet<String>>,
    implications: HashMap<String, HashSet<String>>,
    category_names: HashMap<String, String>,
}

impl CategoryIndex {
    pub fn load(
        tournament_id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Self {
        let categories = speaker_categories::table
            .filter(speaker_categories::tournament_id.eq(tournament_id))
            .load::<SpeakerCategory>(conn)
            .unwrap();

        let memberships = speaker_category_memberships::table
            .filter(
                speaker_category_memberships::tournament_id.eq(tournament_id),
            )
            .load::<SpeakerCategoryMembership>(conn)
            .unwrap();

        let implication_rows = speaker_category_implications::table
            .filter(
                speaker_category_implications::tournament_id.eq(tournament_id),
            )
            .load::<SpeakerCategoryImplication>(conn)
            .unwrap();

        let mut direct_memberships: HashMap<String, HashSet<String>> =
            HashMap::new();
        for membership in memberships {
            direct_memberships
                .entry(membership.speaker_id)
                .or_default()
                .insert(membership.category_id);
        }

        let mut implications: HashMap<String, HashSet<String>> = HashMap::new();
        for implication in implication_rows {
            implications
                .entry(implication.child_category_id)
                .or_default()
                .insert(implication.parent_category_id);
        }

        let category_names = categories
            .into_iter()
            .map(|category| (category.id, category.name))
            .collect();

        Self {
            direct_memberships,
            implications,
            category_names,
        }
    }

    pub fn category_names(&self, category_ids: &[String]) -> String {
        category_ids
            .iter()
            .map(|id| {
                self.category_names
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| id.clone())
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn effective_categories_for_speaker(
        &self,
        speaker_id: &str,
    ) -> HashSet<String> {
        let mut effective = self
            .direct_memberships
            .get(speaker_id)
            .cloned()
            .unwrap_or_default();
        let mut stack: Vec<String> = effective.iter().cloned().collect();

        while let Some(category_id) = stack.pop() {
            if let Some(parents) = self.implications.get(&category_id) {
                for parent in parents {
                    if effective.insert(parent.clone()) {
                        stack.push(parent.clone());
                    }
                }
            }
        }

        effective
    }
}

impl EligibilityRule {
    pub fn from_json_or_default(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    pub fn validate(
        &self,
        tournament_category_ids: &HashSet<String>,
    ) -> Result<(), String> {
        match self {
            EligibilityRule::AllowAll => Ok(()),
            EligibilityRule::IfThresholdMet {
                included_categories: categories,
                threshold,
            } => {
                if categories.is_empty() {
                    return Err(
                        "Choose at least one speaker category.".to_string()
                    );
                }
                for category in categories {
                    if !tournament_category_ids.contains(category) {
                        return Err(format!(
                            "Unknown speaker category in eligibility rule: {category}"
                        ));
                    }
                }
                threshold.validate()
            }
        }
    }

    pub fn evaluate(
        &self,
        speaker_ids: &[String],
        index: &CategoryIndex,
    ) -> EligibilityEvaluation {
        match self {
            EligibilityRule::AllowAll => EligibilityEvaluation {
                eligible: true,
                explanation: "Everyone is eligible.".to_string(),
            },
            EligibilityRule::IfThresholdMet {
                included_categories: categories,
                threshold,
            } => {
                let n = speaker_ids.len();
                if n == 0 {
                    return EligibilityEvaluation {
                        eligible: false,
                        explanation: "No speakers are assigned to this team."
                            .to_string(),
                    };
                }

                let accepted: HashSet<String> =
                    categories.iter().cloned().collect();
                let matching = speaker_ids
                    .iter()
                    .filter(|speaker_id| {
                        let effective =
                            index.effective_categories_for_speaker(speaker_id);
                        effective.iter().any(|cat| accepted.contains(cat))
                    })
                    .count();
                let required = threshold.required_count(n);
                let category_names = index.category_names(categories);

                EligibilityEvaluation {
                    eligible: matching >= required,
                    explanation: format!(
                        "{matching} of {n} speakers count as {category_names}; {required} required."
                    ),
                }
            }
        }
    }
}

impl SpeakerThreshold {
    pub fn required_count(&self, n: usize) -> usize {
        match self {
            SpeakerThreshold::All => n,
            SpeakerThreshold::Geq { count } => *count,
            SpeakerThreshold::GeqNMinusK { k: minus } => {
                n.saturating_sub(*minus).max(1)
            }
        }
    }

    fn validate(&self) -> Result<(), String> {
        match self {
            SpeakerThreshold::All => Ok(()),
            SpeakerThreshold::Geq { count } if *count >= 1 => Ok(()),
            SpeakerThreshold::Geq { .. } => {
                Err("The required speaker count must be at least 1."
                    .to_string())
            }
            SpeakerThreshold::GeqNMinusK { .. } => Ok(()),
        }
    }
}

pub fn implication_would_create_cycle(
    child: &str,
    parent: &str,
    existing: &[SpeakerCategoryImplication],
) -> bool {
    let mut edges: HashMap<&str, Vec<&str>> = HashMap::new();
    for implication in existing {
        edges
            .entry(&implication.child_category_id)
            .or_default()
            .push(&implication.parent_category_id);
    }
    edges.entry(child).or_default().push(parent);

    let mut stack = vec![parent];
    let mut seen = HashSet::new();
    while let Some(next) = stack.pop() {
        if next == child {
            return true;
        }
        if seen.insert(next) {
            if let Some(parents) = edges.get(next) {
                stack.extend(parents.iter().copied());
            }
        }
    }
    false
}

pub fn recompute_break_eligibility<
    L: LoadConnection<Backend = Sqlite> + diesel::Connection,
>(
    tournament_id: &str,
    conn: &mut L,
) {
    let category_index = CategoryIndex::load(tournament_id, conn);
    let teams = teams::table
        .filter(teams::tournament_id.eq(tournament_id))
        .load::<Team>(conn)
        .unwrap();
    let break_categories = break_categories::table
        .filter(break_categories::tournament_id.eq(tournament_id))
        .load::<BreakCategory>(conn)
        .unwrap();
    let team_speakers = speakers_of_team::table
        .inner_join(teams::table)
        .filter(teams::tournament_id.eq(tournament_id))
        .select((speakers_of_team::team_id, speakers_of_team::speaker_id))
        .load::<(String, String)>(conn)
        .unwrap();
    let mut speakers_by_team: HashMap<String, Vec<String>> = HashMap::new();
    for (team_id, speaker_id) in team_speakers {
        speakers_by_team
            .entry(team_id)
            .or_default()
            .push(speaker_id);
    }

    diesel::delete(
        team_break_eligibility::table
            .filter(team_break_eligibility::tournament_id.eq(tournament_id))
            .filter(team_break_eligibility::source.eq("derived")),
    )
    .execute(conn)
    .unwrap();

    let manual_keys: HashSet<(String, String)> = team_break_eligibility::table
        .filter(team_break_eligibility::tournament_id.eq(tournament_id))
        .filter(team_break_eligibility::source.ne("derived"))
        .select((
            team_break_eligibility::team_id,
            team_break_eligibility::break_category_id,
        ))
        .load::<(String, String)>(conn)
        .unwrap()
        .into_iter()
        .collect();

    for break_category in break_categories {
        let rule = EligibilityRule::from_json_or_default(
            &break_category.eligibility_rule_json,
        );
        for team in &teams {
            if manual_keys
                .contains(&(team.id.clone(), break_category.id.clone()))
            {
                continue;
            }
            let speaker_ids = speakers_by_team
                .get(&team.id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let derived = rule.evaluate(speaker_ids, &category_index);
            diesel::insert_into(team_break_eligibility::table)
                .values((
                    team_break_eligibility::id
                        .eq(uuid::Uuid::now_v7().to_string()),
                    team_break_eligibility::tournament_id.eq(tournament_id),
                    team_break_eligibility::team_id.eq(&team.id),
                    team_break_eligibility::break_category_id
                        .eq(&break_category.id),
                    team_break_eligibility::eligible.eq(derived.eligible),
                    team_break_eligibility::source.eq("derived"),
                    team_break_eligibility::explanation.eq(derived.explanation),
                ))
                .execute(conn)
                .unwrap();
        }
    }
}
