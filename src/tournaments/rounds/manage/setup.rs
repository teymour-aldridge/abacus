use axum::extract::Path;
use diesel::{connection::LoadConnection, prelude::*, sqlite::Sqlite};
use hypertext::prelude::*;
use itertools::Itertools;

use crate::{
    auth::User,
    schema::{judge_availability, judges, team_availability, teams},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{Round, TournamentRounds},
    },
    util_resp::{StandardResponse, success},
};

fn percentage_teams_available(
    total_teams: i64,
    round_id: &str,
    conn: &mut impl LoadConnection<Backend = Sqlite>,
) -> (f32, i64, i64) {
    let available = team_availability::table
        .filter(
            team_availability::round_id
                .eq(round_id)
                .and(team_availability::available.eq(true)),
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
    let available = judge_availability::table
        .filter(
            judge_availability::round_id
                .eq(round_id)
                .and(judge_availability::available.eq(true)),
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

pub async fn setup_round_page(
    Path((tid, round_seq)): Path<(String, i64)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let all_rounds = TournamentRounds::fetch(&tid, &mut *conn).unwrap();
    let rounds_in_seq = all_rounds
        .prelim
        .iter()
        .filter(|r| r.seq == round_seq)
        .cloned()
        .collect_vec();

    let total_teams = teams::table
        .filter(teams::tournament_id.eq(&tid))
        .count()
        .get_result::<i64>(&mut *conn)
        .unwrap();
    let team_availability_prop: Vec<(Round, (f32, i64, i64))> = {
        let mut vec = Vec::with_capacity(rounds_in_seq.len());

        for round in &rounds_in_seq {
            vec.push((
                round.clone(),
                percentage_teams_available(total_teams, &round.id, &mut *conn),
            ));
        }

        vec
    };

    let total_judges = judges::table
        .filter(judges::tournament_id.eq(&tid))
        .count()
        .get_result::<i64>(&mut *conn)
        .unwrap();
    let judge_availability_prop: Vec<(Round, (f32, i64, i64))> = {
        let mut vec = Vec::with_capacity(rounds_in_seq.len());

        for round in &rounds_in_seq {
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

    let current_rounds = Round::current_rounds(&tid, &mut *conn);

    success(Page::new()
        .active_nav(crate::template::ActiveNav::Draw)
        .user(user)
        .tournament(tournament.clone())
        .current_rounds(current_rounds)
        .body(maud! {
            SidebarWrapper tournament=(&tournament) rounds=(&all_rounds) active_page=(Some(crate::tournaments::manage::sidebar::SidebarPage::Setup)) selected_seq=(Some(round_seq)) {
                div class="d-flex justify-content-between" {
                    h1 {
                        "Setup for rounds with sequence "
                        (round_seq)
                    }
                    a href=(format!("/tournaments/{}/rounds/{}/draw/manage", &tournament.id, round_seq)) class="btn btn-primary" {
                        "Manage Draw →"
                    }
                }

                div class="d-flex flex-column flex-lg-row gap-4 mt-3" {
                    div class="flex-fill" {
                        div class="d-flex justify-content-between align-items-end mb-4 pb-2 border-bottom border-2 border-dark" {
                            h5 class="mb-0 text-uppercase fw-bold" style="letter-spacing: 2px;" { "1. Teams" }
                            a href=(format!("/tournaments/{}/rounds/{}/availability/teams", &tournament.id, round_seq))
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

                    div class="flex-fill" {
                        div class="d-flex justify-content-between align-items-end mb-4 pb-2 border-bottom border-2 border-dark" {
                            h5 class="mb-0 text-uppercase fw-bold" style="letter-spacing: 2px;" { "2. Judges" }
                            a href=(format!("/tournaments/{}/rounds/{}/availability/judges", &tournament.id, round_seq))
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
        .render())
}
