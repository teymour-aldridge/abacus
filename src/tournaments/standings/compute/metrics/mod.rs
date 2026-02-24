use diesel::prelude::*;

use crate::schema::rounds;

pub mod atss;
pub mod draw_strength;
pub mod n_times_specific_result;
pub mod points;
pub mod tss;

#[diesel::dsl::auto_type]
pub fn completed_preliminary_rounds() -> _ {
    rounds::table.on(rounds::kind
        .eq("P")
        .and(rounds::completed.eq(true))
        .and(rounds::draw_status.eq("released_full")))
}
