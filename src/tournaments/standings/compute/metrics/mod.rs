use diesel::prelude::*;

use crate::schema::tournament_rounds;

pub mod atss;
pub mod draw_strength;
pub mod n_times_specific_result;
pub mod points;
pub mod tss;

#[diesel::dsl::auto_type]
pub fn completed_preliminary_rounds() -> _ {
    tournament_rounds::table.on(tournament_rounds::kind
        .eq("P")
        .and(tournament_rounds::completed.eq(true))
        .and(tournament_rounds::draw_status.eq("R")))
}
