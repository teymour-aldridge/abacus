//! Advisory allocation warnings, loaded once rather than queried per candidate.

use std::collections::{BTreeSet, HashMap, HashSet};

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};

use crate::{
    schema::{
        debates, judge_clashes_of_judge, judges, judges_of_debate, rounds,
        team_clashes_of_judge, teams, teams_of_debate,
    },
    tournaments::{
        participants::{Judge, TournamentParticipants},
        rounds::draws::DebateRepr,
        teams::Team,
    },
};

type Pair = (String, String);
// Sequence keeps history chronological; debate ID distinguishes encounters
// within one round while duplicate allocation rows still count only once.
type History = BTreeSet<(i64, String, String, String)>;

#[derive(Clone, Copy)]
pub(super) enum WarningKind {
    Personal,
    Institutional,
    History,
}

impl WarningKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Personal => "Personal",
            Self::Institutional => "Institutional",
            Self::History => "History",
        }
    }
}

pub(super) struct AllocationWarning {
    pub kind: WarningKind,
    pub message: String,
}

#[derive(Default, Clone, Copy)]
pub(super) struct Relation {
    pub personal: bool,
    pub institutional: bool,
    pub encounters: usize,
}

impl Relation {
    pub(super) fn is_empty(self) -> bool {
        !self.personal && !self.institutional && self.encounters == 0
    }
}

pub(super) struct AllocationClashes {
    tournament_id: String,
    team_personal: HashSet<Pair>,
    judge_personal: HashSet<Pair>,
    team_history: HashMap<Pair, History>,
    judge_history: HashMap<Pair, History>,
}

fn judge_pair(a: &str, b: &str) -> Pair {
    if a < b {
        (a.to_owned(), b.to_owned())
    } else {
        (b.to_owned(), a.to_owned())
    }
}

impl AllocationClashes {
    pub(super) fn load(
        tournament_id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Self {
        let judge_ids: HashSet<String> = judges::table
            .filter(judges::tournament_id.eq(tournament_id))
            .select(judges::id)
            .load(conn)
            .unwrap()
            .into_iter()
            .collect();
        let team_ids: HashSet<String> = teams::table
            .filter(teams::tournament_id.eq(tournament_id))
            .select(teams::id)
            .load(conn)
            .unwrap()
            .into_iter()
            .collect();

        let team_personal = team_clashes_of_judge::table
            .filter(team_clashes_of_judge::tournament_id.eq(tournament_id))
            .select((
                team_clashes_of_judge::judge_id,
                team_clashes_of_judge::team_id,
            ))
            .load::<Pair>(conn)
            .unwrap()
            .into_iter()
            .filter(|(j, t)| judge_ids.contains(j) && team_ids.contains(t))
            .collect();
        let judge_personal = judge_clashes_of_judge::table
            .filter(judge_clashes_of_judge::tournament_id.eq(tournament_id))
            .select((
                judge_clashes_of_judge::judge1_id,
                judge_clashes_of_judge::judge2_id,
            ))
            .load::<Pair>(conn)
            .unwrap()
            .into_iter()
            .filter(|(a, b)| judge_ids.contains(a) && judge_ids.contains(b))
            .map(|(a, b)| judge_pair(&a, &b))
            .collect();

        // Both sides of the join must belong to this tournament, even if a
        // malformed imported debate references a different tournament's round.
        let debate_rounds: HashMap<String, (i64, String, String)> =
            debates::table
                .inner_join(rounds::table)
                .filter(debates::tournament_id.eq(tournament_id))
                .filter(rounds::tournament_id.eq(tournament_id))
                .select((debates::id, (rounds::seq, rounds::id, rounds::name)))
                .load(conn)
                .unwrap()
                .into_iter()
                .collect();

        // Validate endpoints in memory. This avoids an unbounded SQL IN list
        // while keeping all queries bulk and scoped by tournament.
        let mut debate_judges: HashMap<String, HashSet<String>> =
            HashMap::new();
        for (debate, judge) in judges_of_debate::table
            .filter(judges_of_debate::tournament_id.eq(tournament_id))
            .select((judges_of_debate::debate_id, judges_of_debate::judge_id))
            .load::<Pair>(conn)
            .unwrap()
        {
            if debate_rounds.contains_key(&debate) && judge_ids.contains(&judge)
            {
                debate_judges.entry(debate).or_default().insert(judge);
            }
        }
        let mut debate_teams: HashMap<String, HashSet<String>> = HashMap::new();
        for (debate, team) in teams_of_debate::table
            .filter(teams_of_debate::tournament_id.eq(tournament_id))
            .select((teams_of_debate::debate_id, teams_of_debate::team_id))
            .load::<Pair>(conn)
            .unwrap()
        {
            if debate_rounds.contains_key(&debate) && team_ids.contains(&team) {
                debate_teams.entry(debate).or_default().insert(team);
            }
        }

        let mut team_history: HashMap<Pair, History> = HashMap::new();
        let mut judge_history: HashMap<Pair, History> = HashMap::new();
        for (debate, panel) in debate_judges {
            let round = &debate_rounds[&debate];
            let encounter =
                (round.0, round.1.clone(), round.2.clone(), debate.clone());
            for judge in &panel {
                if let Some(teams) = debate_teams.get(&debate) {
                    for team in teams {
                        team_history
                            .entry((judge.clone(), team.clone()))
                            .or_default()
                            .insert(encounter.clone());
                    }
                }
                for colleague in &panel {
                    if judge < colleague {
                        judge_history
                            .entry(judge_pair(judge, colleague))
                            .or_default()
                            .insert(encounter.clone());
                    }
                }
            }
        }

        Self {
            tournament_id: tournament_id.to_owned(),
            team_personal,
            judge_personal,
            team_history,
            judge_history,
        }
    }

    pub(super) fn team_relation(
        &self,
        participants: &TournamentParticipants,
        judge: &Judge,
        team: &Team,
        round_seq: i64,
    ) -> Relation {
        if judge.tournament_id != self.tournament_id
            || team.tournament_id != self.tournament_id
        {
            return Relation::default();
        }
        let institution = judge
            .institution_id
            .as_deref()
            .filter(|id| participants.institutions.contains_key(*id));
        let key = (judge.id.clone(), team.id.clone());
        Relation {
            personal: self.team_personal.contains(&key),
            institutional: institution.is_some()
                && institution == team.institution_id.as_deref(),
            encounters: earlier_encounters(
                self.team_history.get(&key),
                round_seq,
            ),
        }
    }

    pub(super) fn judge_relation(
        &self,
        participants: &TournamentParticipants,
        judge: &Judge,
        colleague: &Judge,
        round_seq: i64,
    ) -> Relation {
        if judge.tournament_id != self.tournament_id
            || colleague.tournament_id != self.tournament_id
            || judge.id == colleague.id
        {
            return Relation::default();
        }
        let institution = judge
            .institution_id
            .as_deref()
            .filter(|id| participants.institutions.contains_key(*id));
        let key = judge_pair(&judge.id, &colleague.id);
        Relation {
            personal: self.judge_personal.contains(&key),
            institutional: institution.is_some()
                && institution == colleague.institution_id.as_deref(),
            encounters: earlier_encounters(
                self.judge_history.get(&key),
                round_seq,
            ),
        }
    }

    pub(super) fn warnings(
        &self,
        participants: &TournamentParticipants,
        debate: &DebateRepr,
        judge: &Judge,
        round_seq: i64,
    ) -> Vec<AllocationWarning> {
        if judge.tournament_id != self.tournament_id
            || debate.debate.tournament_id != self.tournament_id
        {
            return Vec::new();
        }
        let mut warnings = Vec::new();
        let mut seen_teams = HashSet::new();
        for allocation in &debate.teams_of_debate {
            if allocation.tournament_id != self.tournament_id
                || !seen_teams.insert(&allocation.team_id)
            {
                continue;
            }
            let Some(team) = participants.teams.get(&allocation.team_id) else {
                continue;
            };
            if team.tournament_id != self.tournament_id {
                continue;
            }
            let name =
                if team.institution_id.as_ref().is_some_and(|id| {
                    !participants.institutions.contains_key(id)
                }) {
                    team.name.clone()
                } else {
                    participants.canonical_name_of_team(team)
                };
            let relation =
                self.team_relation(participants, judge, team, round_seq);
            if relation.institutional {
                warnings.push(AllocationWarning {
                    kind: WarningKind::Institutional,
                    message: format!(
                        "{} and team {name} share an institution.",
                        judge.name
                    ),
                });
            }
            if relation.personal {
                warnings.push(AllocationWarning {
                    kind: WarningKind::Personal,
                    message: format!(
                        "{} has a declared clash with team {name}.",
                        judge.name
                    ),
                });
            }
            if relation.encounters > 0 {
                let key = (judge.id.clone(), team.id.clone());
                if let Some(rounds) =
                    earlier_rounds(self.team_history.get(&key), round_seq)
                {
                    warnings.push(AllocationWarning {
                        kind: WarningKind::History,
                        message: format!(
                            "{} previously judged team {name} in {rounds}.",
                            judge.name
                        ),
                    });
                }
            }
        }

        let mut seen_judges = HashSet::new();
        for allocation in &debate.judges_of_debate {
            let id = allocation.judge_id.as_str();
            if allocation.tournament_id != self.tournament_id
                || id == judge.id
                || !seen_judges.insert(id)
            {
                continue;
            }
            let Some(colleague) = participants.judges.get(id) else {
                continue;
            };
            if colleague.tournament_id != self.tournament_id {
                continue;
            }
            let relation =
                self.judge_relation(participants, judge, colleague, round_seq);
            if relation.institutional {
                warnings.push(AllocationWarning {
                    kind: WarningKind::Institutional,
                    message: format!(
                        "{} and {} share an institution.",
                        judge.name, colleague.name
                    ),
                });
            }
            if relation.personal {
                warnings.push(AllocationWarning {
                    kind: WarningKind::Personal,
                    message: format!(
                        "{} has a declared clash with {}.",
                        judge.name, colleague.name
                    ),
                });
            }
            if relation.encounters > 0 {
                let key = judge_pair(&judge.id, id);
                if let Some(rounds) =
                    earlier_rounds(self.judge_history.get(&key), round_seq)
                {
                    warnings.push(AllocationWarning {
                        kind: WarningKind::History,
                        message: format!(
                            "{} previously shared a panel with {} in {rounds}.",
                            judge.name, colleague.name
                        ),
                    });
                }
            }
        }
        warnings
    }
}

fn earlier_rounds(history: Option<&History>, round_seq: i64) -> Option<String> {
    let mut seen_rounds = HashSet::new();
    let names: Vec<&str> = history?
        .iter()
        .take_while(|(seq, _, _, _)| *seq < round_seq)
        .filter(|(_, id, _, _)| seen_rounds.insert(id))
        .map(|(_, _, name, _)| name.as_str())
        .collect();
    (!names.is_empty()).then(|| names.join(", "))
}

fn earlier_encounters(history: Option<&History>, round_seq: i64) -> usize {
    history.map_or(0, |history| {
        history
            .iter()
            .take_while(|(seq, _, _, _)| *seq < round_seq)
            .count()
    })
}
