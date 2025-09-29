use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use rand::{Rng, distr::Alphanumeric};

use crate::schema::{tournament_judges, tournament_speakers};

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
            tournament_speakers::table
                .filter(
                    tournament_speakers::private_url
                        .eq(&random_string)
                        .and(tournament_speakers::tournament_id.eq(tournament)),
                )
                .select(tournament_speakers::id)
                .union(
                    tournament_judges::table
                        .filter(
                            tournament_judges::private_url
                                .eq(&random_string)
                                .and(
                                    tournament_judges::tournament_id
                                        .eq(&tournament),
                                ),
                        )
                        .select(tournament_judges::id),
                ),
        ))
        .get_result::<bool>(conn)
        .unwrap();

        if !is_duplicate {
            return random_string;
        }
    }
}
