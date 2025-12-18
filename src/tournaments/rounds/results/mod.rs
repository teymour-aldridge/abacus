use std::collections::HashMap;

use axum::{extract::Path, response::Redirect};
use diesel::prelude::*;
use hypertext::{Renderable, prelude::*};

use crate::{
    auth::User,
    schema::tournament_debate_team_results,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{Round, TournamentRounds, draws::RoundDrawRepr, side_names},
    },
    util_resp::{StandardResponse, err_not_found, see_other_ok, success},
};

#[derive(Queryable, Clone, Debug)]
#[allow(dead_code)]
struct TeamResult {
    id: String,
    debate_id: String,
    team_id: String,
    points: i64,
}

pub async fn view_results_page(
    Path((tid, round_seq)): Path<(String, i64)>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;

    let rounds_in_seq = Round::of_seq(round_seq, &tid, &mut *conn);
    if rounds_in_seq.is_empty() {
        return err_not_found();
    }

    let all_rounds = TournamentRounds::fetch(&tid, &mut *conn).unwrap();

    let current_rounds = Round::current_rounds(&tid, &mut *conn);

    if !tournament.show_round_results {
        return success(
            Page::new()
                .user_opt(user)
                .tournament(tournament.clone())
                .current_rounds(current_rounds)
                .body(maud! {
                    SidebarWrapper tournament=(&tournament) rounds=(&all_rounds) {
                        div class="alert alert-warning" {
                            "This tournament does not make round results publicly available."
                        }
                    }
                })
                .render(),
        );
    }

    let (published_rounds, unpublished_rounds): (Vec<Round>, Vec<Round>) =
        rounds_in_seq
            .iter()
            .cloned()
            .partition(|r| r.results_published_at.is_some());

    if published_rounds.is_empty() {
        if rounds_in_seq.iter().any(|r| r.completed) {
            return success(
                Page::new()
                    .user_opt(user)
                    .tournament(tournament.clone())
                    .current_rounds(current_rounds.clone())
                    .body(maud! {
                        SidebarWrapper tournament=(&tournament) rounds=(&all_rounds) {
                            div class="alert alert-info" {
                                "The results for "
                                @if rounds_in_seq.len() > 1 {
                                    "these rounds have "
                                } @else {
                                    "this round has "
                                }
                                "been completed, but the results "
                                "have not yet been published. You can still see "
                                "the "
                                a href=(format!("/tournaments/{}/rounds/{}/draw", tid, round_seq)) {
                                    "draw for "
                                    @if rounds_in_seq.len() > 1 {
                                        "these rounds"
                                    } @else {
                                        "this round"
                                    }
                                }
                            }
                        }
                    })
                    .render(),
            );
        } else {
            return see_other_ok(Redirect::to(&format!(
                "/tournaments/{}/rounds/{}/draw",
                tid, round_seq
            )));
        }
    }

    struct RoundDisplayData {
        round: Round,
        draw_repr: RoundDrawRepr,
        results_map: HashMap<(String, String), i64>,
    }

    let mut display_data_list = Vec::new();

    for round in published_rounds {
        let draw_repr = RoundDrawRepr::of_round(round.clone(), &mut *conn);

        let debate_ids: Vec<String> = draw_repr
            .debates
            .iter()
            .map(|d| d.debate.id.clone())
            .collect();

        let team_results = tournament_debate_team_results::table
            .filter(
                tournament_debate_team_results::debate_id.eq_any(&debate_ids),
            )
            .load::<TeamResult>(&mut *conn)
            .unwrap();

        let team_results_map: HashMap<(String, String), i64> = team_results
            .into_iter()
            .map(|r| ((r.debate_id, r.team_id), r.points))
            .collect();

        display_data_list.push(RoundDisplayData {
            round,
            draw_repr,
            results_map: team_results_map,
        });
    }

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&all_rounds) {
                    div class="container-fluid" {
                        @if !unpublished_rounds.is_empty() {
                            div class="alert alert-info" {
                                "The results for "
                                @if unpublished_rounds.len() > 1 {
                                    "some rounds "
                                } @else {
                                    "one of the rounds "
                                }
                                "occuring at this time have not yet been published. You can still see the "
                                a href=(format!("/tournaments/{}/rounds/{}/draw", tid, round_seq)) {
                                    "draw for "
                                    @if unpublished_rounds.len() > 1 {
                                        "these rounds"
                                    } @else {
                                        "this round"
                                    }
                                }
                                "."
                            }
                        }

                        @for data in &display_data_list {
                            h1 class="mb-4" { (data.round.name) " Results" }

                            table class="table table-hover align-middle mb-5" {
                                thead class="border-bottom" {
                                    tr {
                                        th scope="col" class="text-uppercase small fw-bold text-muted py-3" { "Room" }
                                        @if let Some(first_debate) = data.draw_repr.debates.first() {
                                            @for dt in &first_debate.teams_of_debate {
                                                th scope="col" class="text-uppercase small fw-bold text-muted py-3" { (side_names::name_of_side(&tournament, dt.side, dt.seq, false)) }
                                            }
                                        }
                                        th scope="col" class="text-uppercase small fw-bold text-muted py-3" { "Adjudicators" }
                                    }
                                }
                                tbody {
                                    @for debate in &data.draw_repr.debates {
                                        tr {
                                            td class="align-middle" {
                                                @if let Some(room) = &debate.room {
                                                    (room.name)
                                                } @else {
                                                    span class="text-muted" { "TBA" }
                                                }
                                            }
                                            @for dt in &debate.teams_of_debate {
                                                @let team = debate.teams.get(&dt.team_id).unwrap();
                                                @let points = data.results_map.get(&(debate.debate.id.clone(), team.id.clone()));
                                                td class="align-middle" {
                                                    span class="fw-bold" { (team.name) }
                                                    @if let Some(pts) = points {
                                                        @let team_count = debate.teams_of_debate.len();

                                                        // todo: might want to
                                                        // put this in a
                                                        // function for re-use
                                                        @let icon = match team_count {
                                                            4 => match pts {
                                                                3 => Some("keyboard_double_arrow_up"),
                                                                2 => Some("keyboard_arrow_up"),
                                                                1 => Some("keyboard_arrow_down"),
                                                                0 => Some("keyboard_double_arrow_down"),
                                                                _ => None,
                                                            },
                                                            2 => if *pts > 0 { Some("keyboard_arrow_up") } else { Some("keyboard_arrow_down") },
                                                            _ => None,
                                                        };

                                                        @if let Some(i) = icon {
                                                            span class="material-icons ms-2 align-middle text-muted" { (i) }
                                                        } @else {
                                                            @if *pts > 0 {
                                                                span class="badge bg-success ms-2" { (pts) }
                                                            } @else {
                                                                span class="badge bg-danger ms-2" { (pts) }
                                                            }
                                                        }
                                                    } @else {
                                                        span class="text-muted ms-2" { "-" }
                                                    }
                                                }
                                            }
                                            td class="align-middle" {
                                                @for judge in debate.judges_of_debate.iter() {
                                                    span class="badge bg-secondary me-1" {
                                                        (debate.judges.get(&judge.judge_id).unwrap().name)
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
