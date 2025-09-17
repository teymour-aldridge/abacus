use diesel::prelude::Queryable;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Queryable, Clone, Debug)]
pub struct Team {
    pub id: String,
    pub tournament_id: String,
    pub name: String,
    pub institution_id: Option<String>,
    pub number: i64,
}
