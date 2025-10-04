use diesel::prelude::Queryable;

#[derive(Queryable, Clone, Debug)]
pub struct BreakCategory {
    pub id: String,
    pub tournament_id: String,
    pub name: String,
    pub priority: i64,
}
