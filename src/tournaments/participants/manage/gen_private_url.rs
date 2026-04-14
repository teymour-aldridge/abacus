use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};

use crate::{
    non_det::NonDet,
    schema::{judges, speakers},
};

pub fn get_unique_private_url(
    tournament: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
    non_det: &NonDet,
) -> String {
    loop {
        let random_string = non_det.alphanumeric_string(12);

        let is_duplicate = diesel::dsl::select(diesel::dsl::exists(
            speakers::table
                .filter(
                    speakers::private_url
                        .eq(&random_string)
                        .and(speakers::tournament_id.eq(tournament)),
                )
                .select(speakers::id)
                .union(
                    judges::table
                        .filter(
                            judges::private_url
                                .eq(&random_string)
                                .and(judges::tournament_id.eq(&tournament)),
                        )
                        .select(judges::id),
                ),
        ))
        .get_result::<bool>(conn)
        .unwrap();

        if !is_duplicate {
            return random_string;
        }
    }
}
