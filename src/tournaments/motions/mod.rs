use axum::extract::Path;
use diesel::prelude::*;
use hypertext::{maud, prelude::*};

use crate::{
    auth::User,
    schema::tournament_round_motions,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        rounds::{Motion, Round, TournamentRounds},
    },
    util_resp::{StandardResponse, success},
};

pub async fn public_motions_page(
    Path(tournament_id): Path<String>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let rounds_data =
        TournamentRounds::fetch(&tournament_id, &mut *conn).unwrap();

    let all_rounds: Vec<Round> = rounds_data
        .prelim
        .iter()
        .chain(rounds_data.elim.iter())
        .cloned()
        .collect();

    let motions: Vec<(Motion, String)> = tournament_round_motions::table
        .filter(tournament_round_motions::tournament_id.eq(&tournament_id))
        .load::<Motion>(&mut *conn)
        .unwrap()
        .into_iter()
        .filter_map(|motion| {
            all_rounds
                .iter()
                .find(|r| r.id == motion.round_id)
                .map(|round| (motion, round.name.clone()))
        })
        .collect();

    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .body(maud! {
                div class="container py-5 px-4" {
                    h1 { "Motions" }

                    @if motions.is_empty() {
                        p class="text-muted" { "No motions have been released yet." }
                    } @else {
                        div class="table-responsive" {
                            table class="table table-striped" {
                                thead {
                                    tr {
                                        th scope="col" { "Round" }
                                        th scope="col" { "Motion" }
                                        th scope="col" { "Infoslide" }
                                    }
                                }
                                tbody {
                                    @for (motion, round_name) in &motions {
                                        tr {
                                            td { (round_name) }
                                            td { (motion.motion) }
                                            td {
                                                @if let Some(infoslide) = &motion.infoslide {
                                                    (infoslide)
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .render(),
    )
}
