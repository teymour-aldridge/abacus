pub mod edit;
pub mod overview;
pub mod view_ballot_set;

pub(super) fn scope_debate(
    tournament_id: &str,
    debate_id: &str,
    conn: &mut impl diesel::connection::LoadConnection<
        Backend = diesel::sqlite::Sqlite,
    >,
) -> Result<(), crate::util_resp::FailureResponse> {
    use crate::schema::{debates, rounds};
    use diesel::prelude::*;
    debates::table
        .inner_join(rounds::table)
        .filter(debates::id.eq(debate_id))
        .filter(debates::tournament_id.eq(tournament_id))
        .filter(rounds::tournament_id.eq(tournament_id))
        .select(debates::id)
        .first::<String>(conn)?;
    Ok(())
}
