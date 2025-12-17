use std::collections::HashMap;

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    schema::{tournament_round_motions, tournament_rounds},
    util_resp::{FailureResponse, err_not_found},
};

pub mod ballots;
pub mod draws;
pub mod manage;
pub mod results;
pub mod side_names;

#[derive(Serialize, Deserialize, Queryable, Clone, Debug)]
pub struct Round {
    pub id: String,
    pub tournament_id: String,
    pub seq: i64,
    pub name: String,
    kind: String,
    break_cat: Option<String>,
    pub completed: bool,
    pub draw_status: String,
    pub draw_released_at: Option<chrono::NaiveDateTime>,
    pub motions_released_at: Option<chrono::NaiveDateTime>,
    pub results_published_at: Option<chrono::NaiveDateTime>,
}

#[derive(Debug, Copy, Clone)]
pub enum RoundStatus {
    NotStarted,
    InProgress,
    Completed,
    Draft,
}

impl Round {
    pub fn of_seq(
        seq: i64,
        tournament_id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Vec<Round> {
        tournament_rounds::table
            .filter(
                tournament_rounds::tournament_id
                    .eq(tournament_id)
                    .and(tournament_rounds::seq.eq(seq)),
            )
            .load::<Round>(conn)
            .unwrap()
    }

    pub fn fetch(
        round_id: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<Self, FailureResponse> {
        tournament_rounds::table
            .filter(tournament_rounds::id.eq(round_id))
            .first::<Round>(conn)
            .optional()
            .unwrap()
            .map(Ok)
            .unwrap_or(err_not_found().map(|_| {
                unreachable!("err_not_found always returns an `Err` variant")
            }))
    }

    /// Retrieves the current rounds.
    pub fn current_rounds(
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Vec<Self> {
        let ret = tournament_rounds::table
            .filter(
                tournament_rounds::tournament_id
                    .eq(tid)
                    .and(tournament_rounds::completed.eq(false))
                    .and({
                        let sq = diesel::alias!(tournament_rounds as sq);
                        let min_seq = sq
                            .filter(
                                sq.field(tournament_rounds::tournament_id)
                                    .eq(tid)
                                    .and(
                                        tournament_rounds::completed.eq(false),
                                    ),
                            )
                            .select(diesel::dsl::min(
                                sq.field(tournament_rounds::seq),
                            ))
                            .single_value();

                        tournament_rounds::seq.eq(diesel::dsl::case_when(
                            min_seq.is_not_null(),
                            min_seq.assume_not_null(),
                        )
                        .otherwise(1))
                    }),
            )
            .order_by(tournament_rounds::seq.asc())
            .load::<Round>(conn)
            .unwrap();

        // TODO: is this desirable?
        debug_assert!(if ret.iter().any(|r| r.kind == "P") {
            ret.iter().all(|r| r.kind != "E")
        } else {
            true
        });

        ret
    }

    pub fn find_first_preceding_incomplete_round(
        &self,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Option<Round> {
        tournament_rounds::table
            .filter(tournament_rounds::seq.lt(self.seq))
            .filter(tournament_rounds::tournament_id.eq(&self.tournament_id))
            .first::<Round>(conn)
            .optional()
            .unwrap()
    }
}

#[derive(Clone)]
pub struct TournamentRounds {
    pub prelim: Vec<Round>,
    pub elim: Vec<Round>,
    pub statuses: HashMap<String, RoundStatus>,
}

impl TournamentRounds {
    pub fn fetch(
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> Result<TournamentRounds, diesel::result::Error> {
        let rounds = tournament_rounds::table
            .filter(tournament_rounds::tournament_id.eq(tid))
            .order_by((tournament_rounds::seq.asc(),))
            .load::<Round>(conn)
            .unwrap();

        // check ordering of the rounds, and that elimination and preliminary
        // rounds are well separated
        enum State {
            P,
            E,
        }

        rounds.iter().fold(State::P, |state, next| {
            match (state, next.kind.as_str()) {
                (State::P, "P") => {
                    State::P
                }
                (State::P, "E") => {
                    State::E
                }
                (State::E, "E") => {
                    State::E
                },
                (State::E, "P") => {
                    panic!("preliminary rounds must come before elimination rounds {next:?}");
                }
                _ => unreachable!()
            }
        });

        let is_prelim_round = |round: &Round| round.kind == "P";

        let round_status = rounds
            .iter()
            .map(|round| {
                if round.completed {
                    (round.id.clone(), RoundStatus::Completed)
                } else if round.draw_status == "R" {
                    (round.id.clone(), RoundStatus::InProgress)
                } else if round.draw_status == "D" || round.draw_status == "C" {
                    (round.id.clone(), RoundStatus::Draft)
                } else {
                    (round.id.clone(), RoundStatus::NotStarted)
                }
            })
            .collect::<HashMap<_, _>>();
        Ok(TournamentRounds {
            prelim: rounds
                .clone()
                .into_iter()
                .take_while(is_prelim_round)
                .collect(),
            elim: rounds.into_iter().skip_while(is_prelim_round).collect(),
            statuses: { round_status },
        })
    }

    pub fn prelims_grouped_by_seq(&self) -> Vec<Vec<Round>> {
        let grouped_iterator = self
            .prelim
            .iter()
            .sorted_by_key(|round| round.seq)
            .chunk_by(|round| round.seq);

        grouped_iterator
            .into_iter()
            .map(|(_, rounds)| {
                rounds.into_iter().map(Clone::clone).collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn all_grouped_by_seq(&self) -> Vec<Vec<Round>> {
        use itertools::Itertools;

        let grouped_iterator = self
            .prelim
            .iter()
            .chain(self.elim.iter())
            .sorted_by_key(|round| round.seq)
            .chunk_by(|round| round.seq);

        grouped_iterator
            .into_iter()
            .map(|(_, rounds)| {
                rounds.into_iter().map(Clone::clone).collect::<Vec<_>>()
            })
            .collect()
    }

    pub fn categories(&self) -> HashMap<String, Vec<Round>> {
        let mut ret = HashMap::new();
        for round in &self.elim {
            ret.entry(
                round
                    .break_cat
                    .as_ref()
                    .expect(
                        "all elimination rounds should have a break category",
                    )
                    .clone(),
            )
            .and_modify(|list: &mut Vec<Round>| {
                list.push(round.clone());
            })
            .or_insert(vec![round.clone()]);
        }
        ret
    }
}

#[derive(Queryable, QueryableByName)]
#[diesel(table_name = tournament_round_motions)]
pub struct Motion {
    pub id: String,
    pub tournament_id: String,
    pub round_id: String,
    pub infoslide: Option<String>,
    pub motion: String,
}
