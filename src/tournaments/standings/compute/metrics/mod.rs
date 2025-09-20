use std::collections::HashMap;

use diesel::prelude::*;
use diesel::{connection::LoadConnection, sqlite::Sqlite};

use crate::schema::tournament_rounds;

pub mod atss;
pub mod draw_strength;
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
    Integer(i64),
    Float(rust_decimal::Decimal),
}

impl MetricValue {
    pub fn as_integer(&self) -> Option<&i64> {
        if let Self::Integer(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_float(&self) -> Option<&rust_decimal::Decimal> {
        if let Self::Float(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

impl std::fmt::Display for MetricValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricValue::Integer(integer) => write!(f, "{integer}"),
            MetricValue::Float(decimal) => write!(f, "{decimal}"),
        }
    }
}
