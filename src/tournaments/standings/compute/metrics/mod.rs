use std::collections::HashMap;

use diesel::prelude::*;
use diesel::{connection::LoadConnection, sqlite::Sqlite};

use crate::schema::tournament_rounds;

pub mod ds_wins;
pub mod n_times_specific_result;
pub mod points;
pub mod tss;

pub trait Metric<V> {
    fn compute(
        &self,
        tid: &str,
        conn: &mut impl LoadConnection<Backend = Sqlite>,
    ) -> HashMap<String, V>;
}

#[diesel::dsl::auto_type]
pub fn completed_preliminary_rounds() -> _ {
    tournament_rounds::table.on(tournament_rounds::kind
        .eq("P")
        .and(tournament_rounds::completed.eq(true)))
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
pub enum MetricValue {
    Points(i64),
    NTimesResult(u8, i64),
    Tss(rust_decimal::Decimal),
    DsWins(i64),
}

impl std::fmt::Display for MetricValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricValue::Points(n) => write!(f, "{n}"),
            MetricValue::NTimesResult(_, times) => write!(f, "{times}"),
            MetricValue::Tss(decimal) => write!(f, "{decimal}"),
            MetricValue::DsWins(wins) => write!(f, "{wins}"),
        }
    }
}
