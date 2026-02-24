use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use rand::{Rng, distr::Alphanumeric};

use crate::schema::{judges, speakers};

pub fn get_unique_private_url(
    tournament: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> String {
    loop {
        let random_string: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(12)
            .map(char::from)
            .collect();

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
