use chrono::NaiveDateTime;
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use serde::{Deserialize, Serialize};

#[derive(Queryable, Serialize, Deserialize)]
pub struct Snapshot {
    id: String,
    created_at: NaiveDateTime,
    contents: Option<String>,
    tournament_id: String,
    prev: Option<String>,
    schema_id: String,
}

pub struct SnapshotData {}

#[tracing::instrument(skip(_conn))]
pub fn take_snapshot(
    _tid: &str,
    _conn: &mut impl LoadConnection<Backend = Sqlite>,
) {
    // todo: implement
}
