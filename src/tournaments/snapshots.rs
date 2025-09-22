use chrono::NaiveDateTime;
use diesel::{
    connection::LoadConnection, prelude::*, sql_types::Text, sqlite::Sqlite,
};
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

/// This struct can be deserialized
pub struct SnapshotData {}

pub fn take_snapshot(
    _tid: &str,
    _conn: &mut impl LoadConnection<Backend = Sqlite>,
) {
    // todo: implement
}

#[derive(QueryableByName)]
struct JsonResult {
    #[diesel(sql_type = Text)]
    _json: String,
}
