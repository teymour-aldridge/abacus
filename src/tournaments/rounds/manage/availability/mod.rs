use axum::extract::Path;
use hypertext::prelude::*;

use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};

use crate::{
    auth::User,
    schema::{
        tournament_judge_availability, tournament_judges,
        tournament_team_availability, tournament_teams,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{Round, TournamentRounds},
    },
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

fn percentage_judges_available(
    total_judges: i64,
    round_id: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> (f32, i64, i64) {
    let available = tournament_judge_availability::table
        .filter(
            tournament_judge_availability::round_id
                .eq(round_id)
                .and(tournament_judge_availability::available.eq(true)),
        )
        .count()
        .get_result::<i64>(&mut *conn)
        .unwrap();

    (
        (available as f32) / (total_judges as f32),
        available,
        total_judges,
    )
}

pub async fn manage_round_availability(
    Path((tournament_id, seq_id)): Path<(String, usize)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let current_rounds = Round::current_rounds(&tournament.id, &mut *conn);

    if current_rounds.is_empty() {
        // todo: needs a proper error message
        return err_not_found();
    }

    let total_teams = tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(&tournament_id))
        .count()
        .get_result::<i64>(&mut *conn)
        .unwrap();
    let team_availability_prop: Vec<(Round, (f32, i64, i64))> = {
        let mut vec = Vec::with_capacity(current_rounds.len());

        for round in &current_rounds {
            vec.push((
                round.clone(),
                percentage_teams_available(total_teams, &round.id, &mut *conn),
            ));
        }

        vec
    };

    let total_judges = tournament_judges::table
        .filter(tournament_judges::tournament_id.eq(&tournament_id))
        .count()
        .get_result::<i64>(&mut *conn)
        .unwrap();
    let judge_availability_prop: Vec<(Round, (f32, i64, i64))> = {
        let mut vec = Vec::with_capacity(current_rounds.len());

        for round in &current_rounds {
            vec.push((
                round.clone(),
                percentage_judges_available(
                    total_judges,
                    &round.id,
                    &mut *conn,
                ),
            ));
        }

        vec
    };

    success(
        Page::new()
            .user(user)
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) {
                    h1 {
                        "Manage availability for rounds "
                        @for (i, round) in current_rounds.iter().enumerate() {
                            @if i > 0 {
                                ", "
                            }
                            (round.name)
                        }
                    }

                    div class="d-flex flex-column flex-lg-row gap-4 mt-3" {
                        // Teams Section
                        div class="flex-fill" {
                            div class="d-flex justify-content-between align-items-end mb-4 pb-2 border-bottom border-2 border-dark" {
                                h5 class="mb-0 text-uppercase fw-bold" style="letter-spacing: 2px;" { "1. Teams" }
                                a href=(format!("/tournaments/{tournament_id}/rounds/{seq_id}/availability/teams"))
                                    class="btn btn-primary btn-sm" {
                                    "Manage"
                                }
                            }

                            div class="table-responsive" {
                                table class="table table-hover table-borderless align-middle" {
                                    thead class="border-bottom border-dark" {
                                        tr {
                                            th scope="col" class="text-uppercase small fw-bold text-muted py-3" { "Round" }
                                            th scope="col" class="text-uppercase small fw-bold text-muted py-3 text-end" { "Available" }
                                            th scope="col" class="text-uppercase small fw-bold text-muted py-3" { "Availability" }
                                        }
                                    }
                                    tbody {
                                        @for (round, available) in &team_availability_prop {
                                            tr class="border-bottom" {
                                                td class="py-3" {
                                                    span class="fw-bold" { (round.name) }
                                                }
                                                td class="py-3 text-end" {
                                                    span class="font-monospace" {
                                                        (available.1)"/"(available.2)
                                                    }
                                                }
                                                td class="py-3" style="width: 40%;" {
                                                    div class="progress" role="progressbar"
                                                        aria-label=(format!("Proportion of teams available for {}", round.name))
                                                        aria_valuemin=0 aria_valuemax=100
                                                        aria_valuenow=(available.0 * 100.0)
                                                        style="height: 8px;" {
                                                            div class="progress-bar bg-dark"
                                                                style=(format!("width: {}%;", available.0 * 100.0)) {
                                                            }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Judges Section
                        div class="flex-fill" {
                            div class="d-flex justify-content-between align-items-end mb-4 pb-2 border-bottom border-2 border-dark" {
                                h5 class="mb-0 text-uppercase fw-bold" style="letter-spacing: 2px;" { "2. Judges" }
                                a href=(format!("/tournaments/{tournament_id}/rounds/{seq_id}/availability/judges"))
                                    class="btn btn-primary btn-sm" {
                                    "Manage"
                                }
                            }

                            div class="table-responsive" {
                                table class="table table-hover table-borderless align-middle" {
                                    thead class="border-bottom border-dark" {
                                        tr {
                                            th scope="col" class="text-uppercase small fw-bold text-muted py-3" { "Round" }
                                            th scope="col" class="text-uppercase small fw-bold text-muted py-3 text-end" { "Available" }
                                            th scope="col" class="text-uppercase small fw-bold text-muted py-3" { "Availability" }
                                        }
                                    }
                                    tbody {
                                        @for (round, available) in &judge_availability_prop {
                                            tr class="border-bottom" {
                                                td class="py-3" {
                                                    span class="fw-bold" { (round.name) }
                                                }
                                                td class="py-3 text-end" {
                                                    span class="font-monospace" {
                                                        (available.1)"/"(available.2)
                                                    }
                                                }
                                                td class="py-3" style="width: 40%;" {
                                                    div class="progress" role="progressbar"
                                                        aria-label=(format!("Proportion of judges available for {}", round.name))
                                                        aria_valuemin=0 aria_valuemax=100
                                                        aria_valuenow=(available.0 * 100.0)
                                                        style="height: 8px;" {
                                                            div class="progress-bar bg-dark"
                                                                style=(format!("width: {}%;", available.0 * 100.0)) {
                                                            }
                                                    }
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
