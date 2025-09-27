use hypertext::prelude::*;
use rocket::get;

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};

use crate::{
    auth::User,
    schema::{
        tournament_rounds, tournament_team_availability, tournament_teams,
    },
    state::Conn,
    template::Page,
    tournaments::{Tournament, rounds::Round},
    util_resp::{StandardResponse, err_not_found, success},
};

pub mod judges;
pub mod teams;

// todo: handle out-rounds (filter non-breaking teams)
fn percentage_teams_available(
    total_teams: i64,
    round_id: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> (f32, i64, i64) {
    let available = tournament_team_availability::table
        .filter(
            tournament_team_availability::round_id
                .eq(round_id)
                .and(tournament_team_availability::available.eq(true)),
        )
        .count()
        .get_result::<i64>(&mut *conn)
        .unwrap();

    (
        (available as f32) / (total_teams as f32),
        available,
        total_teams,
    )
}

#[get("/tournaments/<tournament_id>/rounds/<seq_id>/availability")]
pub async fn manage_round_availability(
    tournament_id: &str,
    seq_id: usize,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = tournament_rounds::table
        .filter(
            tournament_rounds::tournament_id
                .eq(&tournament_id)
                .and(tournament_rounds::seq.eq(seq_id as i64)),
        )
        .load::<Round>(&mut *conn)
        .unwrap();

    if rounds.is_empty() {
        // todo: needs a proper error message
        return err_not_found();
    }

    let total_teams = tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(tournament_id))
        .count()
        .get_result::<i64>(&mut *conn)
        .unwrap();
    let team_availability_prop: Vec<(Round, (f32, i64, i64))> = {
        let mut vec = Vec::with_capacity(rounds.len());

        for round in &rounds {
            vec.push((
                round.clone(),
                percentage_teams_available(total_teams, &round.id, &mut *conn),
            ));
        }

        vec
    };

    success(
        Page::new()
            .user(user)
            .body(maud! {
                h1 {
                    "Manage availability for rounds "
                    @for (i, round) in rounds.iter().enumerate() {
                        @if i > 0 {
                            ", "
                        }
                        (round.name)
                    }
                }


                div class = "row" {
                    div class = "col-md-4" {
                        div class = "card" {
                            div class="card-body" {
                                h5 class="card-title" {
                                    "Teams"
                                }
                            }
                            div class="list-group list-group-flush" {
                                @for (round, available) in &team_availability_prop {
                                    div class="list-group-item" {
                                        div class="d-flex justify-content-between" {
                                            span {
                                                (round.name)
                                            }
                                            strong {
                                                (available.1)"/"(available.2)
                                            }
                                        }
                                        div class="progress" role="progressbar"
                                            aria-label=(format!("Proportion of teams available for {}", round.name))
                                            aria_valuemin=0 aria_valuemax=100
                                            aria_valuenow=(available.0 * 100.0) {
                                                div class="progress-bar"
                                                    style=(format!("width: {};", available.0 * 100.0)) {
                                                }
                                        }
                                    }
                                }
                            }
                            div class="card-body" {
                                a href=(format!("/tournaments/{tournament_id}/rounds/{seq_id}/availability/teams"))
                                    class="btn btn-primary" {
                                    "Manage team availability"
                                }
                            }
                        }
                    }

                    div class = "col-md-4" {
                        div class = "card" {
                            div class="card-body" {
                                h5 class="card-title" {
                                    "Judges"
                                }
                                a href=(format!("/tournaments/{tournament_id}/rounds/{seq_id}/availability/judges"))
                                    class="btn btn-primary" {
                                    "Manage judge availability"
                                }
                            }
                        }
                    }
                }
            })
            .render(),
    )
}
