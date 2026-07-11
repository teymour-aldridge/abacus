use diesel::prelude::Queryable;

#[derive(Queryable, Clone, Debug)]
pub struct BreakCategory {
    pub id: String,
    pub tournament_id: String,
    pub name: String,
    pub priority: i64,
    pub slug: String,
    pub seq: i64,
    pub break_size: i64,
    pub reserve_size: i64,
    pub public: bool,
    pub limit_: i64,
    pub eligibility_rule_json: String,
}
