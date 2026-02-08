use axum::extract::Path;
use diesel::prelude::*;
use hypertext::prelude::*;
use itertools::Itertools;

use crate::{
    auth::User,
    schema::{tournament_debates, tournament_rounds},
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{
            Round, TournamentRounds,
            ballots::BallotRepr,
            draws::{Debate, DebateRepr},
        },
    },
    util_resp::{StandardResponse, err_not_found, success},
};

pub async fn admin_ballot_of_seq_overview(
    Path((tournament_id, round_seq)): Path<(String, i64)>,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let all_rounds =
        TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    let rounds = tournament_rounds::table
        .filter(tournament_rounds::seq.eq(round_seq))
        .load::<Round>(&mut *conn)
        .unwrap();

    if rounds.is_empty() {
        return err_not_found();
    }

    let debates = tournament_debates::table
        .inner_join(
            tournament_rounds::table.on(tournament_rounds::seq
                .eq(round_seq)
                .and(tournament_debates::round_id.eq(tournament_rounds::id))),
        )
        .order_by((
            tournament_rounds::id.asc(),
            tournament_debates::number.asc(),
        ))
        .select(tournament_debates::all_columns)
        .load::<Debate>(&mut *conn)
        .unwrap();

    let ballot_sets: Vec<(DebateRepr, Vec<BallotRepr>)> = debates
        .iter()
        .map(|debate| {
            let debate_repr = DebateRepr::fetch(&debate.id, &mut *conn);
            let ballots = debate_repr.ballots(&mut *conn);
            (debate_repr, ballots)
        })
        .collect_vec();

    let html = {
        let tournament = tournament.clone();
        maud! {
            SidebarWrapper rounds=(&all_rounds) tournament=(&tournament)
                active_page=(Some(crate::tournaments::manage::sidebar::SidebarPage::Ballots))
                selected_seq=(Some(round_seq)) {

                div class="mb-4 pb-3 border-bottom border-2 border-dark" {
                    h1 class="mb-2" { "Ballot Status" }
                    p class="text-muted mb-0" { (rounds[0].name) }
                }

                @let total_ballots = ballot_sets.iter()
                    .map(|(_, ballots)| ballots.len()).sum::<usize>();
                @let total_expected = ballot_sets.iter()
                    .map(|(debate, _)| debate.judges_of_debate.len())
                    .sum::<usize>();
                @let completion_rate = if total_expected > 0 {
                    (total_ballots as f64 / total_expected as f64 * 100.0) as i32
                } else {
                    0
                };

                div class="row g-3 mb-4" {
                    div class="col-md-4" {
                        div class="p-3 border border-dark rounded-3" {
                            div class="text-uppercase small fw-bold text-muted mb-1"
                                style="letter-spacing: 1px;" {
                                "Total Debates"
                            }
                            div class="fs-3 fw-bold" { (ballot_sets.len()) }
                        }
                    }
                    div class="col-md-4" {
                        div class="p-3 border border-dark rounded-3" {
                            div class="text-uppercase small fw-bold text-muted mb-1"
                                style="letter-spacing: 1px;" {
                                "Ballots Submitted"
                            }
                            div class="fs-3 fw-bold" {
                                (total_ballots) " / " (total_expected)
                            }
                        }
                    }
                    div class="col-md-4" {
                        div class="p-3 border border-dark rounded-3" {
                            div class="text-uppercase small fw-bold text-muted mb-1"
                                style="letter-spacing: 1px;" {
                                "Completion"
                            }
                            div class="fs-3 fw-bold" { (completion_rate) "%" }
                        }
                    }
                }

                div class="table-responsive" {
                    table class="table table-hover table-borderless align-middle" {
                        thead class="border-bottom border-dark" {
                            tr {
                                th scope="col"
                                    class="text-uppercase small fw-bold text-muted py-3" {
                                    "Round"
                                }
                                th scope="col"
                                    class="text-uppercase small fw-bold text-muted py-3"
                                    style="width: 80px;" {
                                    "Debate"
                                }
                                th scope="col"
                                    class="text-uppercase small fw-bold text-muted py-3" {
                                    "Teams"
                                }
                                th scope="col"
                                    class="text-uppercase small fw-bold text-muted py-3" {
                                    "Judge"
                                }
                                th scope="col"
                                    class="text-uppercase small fw-bold text-muted py-3"
                                    style="width: 120px;" {
                                    "Status"
                                }
                                th scope="col"
                                    class="text-uppercase small fw-bold text-muted py-3 text-end"
                                    style="width: 100px;" {
                                    "Actions"
                                }
                            }
                        }
                        tbody {
                            @for (debate, ballots) in ballot_sets.iter() {
                                @let num_judges = debate.judges_of_debate.len();
                                @let ballot_problems = BallotRepr::problems_of_set(
                                    ballots, &tournament, debate);

                                @if num_judges == 0 {
                                    tr class="border-bottom" {
                                        td class="text-center py-4 fw-bold fs-5" {
                                            (rounds.iter()
                                                .find(|r| r.id == debate.debate.round_id)
                                                .unwrap().name)
                                        }
                                        td class="text-center py-4 fw-bold fs-5" {
                                            (debate.debate.number)
                                        }
                                        td class="py-4" {
                                            div class="d-flex flex-column gap-1" {
                                                @for team in &debate.teams_of_debate {
                                                    div class="small" {
                                                        span class="fw-bold" {
                                                            (debate.teams[&team.team_id].name)
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        td class="py-3" style="color: #6c757d;" {
                                            div class="fw-bold fst-italic" {
                                                "No judges assigned"
                                            }
                                        }
                                        td class="py-3" {
                                            span class="badge text-uppercase small"
                                                style="background-color: #6c757d;
                                                   color: white;
                                                   letter-spacing: 0.5px;" {
                                                "No Judge"
                                            }
                                        }
                                        td class="text-end py-3" {
                                        }
                                    }
                                } @else {
                                    @for (idx, judge_in_debate) in
                                        debate.judges_of_debate.iter().enumerate() {
                                        tr class="border-bottom" {
                                            @let judge = &debate.judges[
                                                &judge_in_debate.judge_id];
                                            @let ballot_of_judge = ballots.iter()
                                                .find(|b| b.metadata.judge_id == judge.id);

                                            @if idx == 0 {
                                                td class="text-center py-4 fw-bold fs-5" {
                                                    (rounds.iter()
                                                        .find(|r| r.id == debate.debate.round_id)
                                                        .unwrap().name)
                                                }
                                                td rowspan=(num_judges)
                                                    class="text-center py-4 fw-bold fs-5" {
                                                    (debate.debate.number)
                                                }
                                                td rowspan=(num_judges) class="py-4" {
                                                    div class="d-flex flex-column gap-1" {
                                                        @for team in &debate.teams_of_debate {
                                                            div class="small" {
                                                                span class="fw-bold" {
                                                                    (debate.teams[&team.team_id].name)
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            td class="py-3" {
                                                div class="fw-bold" { (judge.name) }
                                            }
                                            td class="py-3" {
                                                @match ballot_of_judge {
                                                    Some(_) => {
                                                        @if ballot_problems.is_empty() {
                                                            span class="badge text-uppercase small"
                                                                style="background-color: #198754;
                                                                   color: white;
                                                                   letter-spacing: 0.5px;" {
                                                                "Submitted"
                                                            }
                                                        } @else {
                                                            div class="d-flex flex-column gap-2" {
                                                                span class="badge text-uppercase small"
                                                                    style="background-color: #ffc107;
                                                                       color: #212529;
                                                                       letter-spacing: 0.5px;" {
                                                                    "Problems"
                                                                }
                                                                div class="small text-muted"
                                                                    style="font-size: 0.75rem;
                                                                           line-height: 1.4;" {
                                                                    @for problem in &ballot_problems {
                                                                        div { (problem) }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    },
                                                    None => {
                                                        span class="badge text-uppercase small"
                                                            style="background-color: #d9534f;
                                                               color: white;
                                                               letter-spacing: 0.5px;" {
                                                            "Missing"
                                                        }
                                                    },
                                                }
                                            }
                                            td class="text-end py-3" {
                                                @if ballot_of_judge.is_some() {
                                                    a href=(format!(
                                                        "/tournaments/{}/debates/{}/ballots",
                                                        tournament.id,
                                                        debate.debate.id
                                                      ))
                                                      class="btn btn-sm"
                                                      style
                                                        ="border: 1px solid #212529;
                                                          color: #212529;"
                                                    {
                                                        "View"
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
        }
    };

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(html)
            .render(),
    )
}
