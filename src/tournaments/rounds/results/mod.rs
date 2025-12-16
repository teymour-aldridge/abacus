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

    let round = match Round::of_seq(round_seq, &tid, &mut *conn).first() {
        Some(r) => r.clone(),
        None => return err_not_found(),
    };

    let all_rounds = TournamentRounds::fetch(&tid, &mut *conn).unwrap();

    if !tournament.show_round_results {
        return err_not_found();
    }

    if round.results_published_at.is_none() {
        if round.completed {
            return success(
                Page::new()
                    .user_opt(user)
                    .tournament(tournament.clone())
                    .body(maud! {
                        SidebarWrapper tournament=(&tournament) rounds=(&all_rounds) {
                            div class="alert alert-info" {
                                "This round has been completed, but the results "
                                "have not yet been published. You can still see "
                                "the "
                                a href=(format!("/tournaments/{}/rounds/{}/draw", tid, round_seq)) {
                                    "draw for this round"
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

    let draw_repr = RoundDrawRepr::of_round(round.clone(), &mut *conn);

    let debate_ids: Vec<String> = draw_repr
        .debates
        .iter()
        .map(|d| d.debate.id.clone())
        .collect();

    let team_results = tournament_debate_team_results::table
        .filter(tournament_debate_team_results::debate_id.eq_any(&debate_ids))
        .load::<TeamResult>(&mut *conn)
        .unwrap();

    let team_results_map: HashMap<(String, String), i64> = team_results
        .into_iter()
        .map(|r| ((r.debate_id, r.team_id), r.points))
        .collect();

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&all_rounds) {
                    div class="container-fluid" {
                        h1 { (round.name) " Results" }

                        table class="table table-bordered" {
                            thead class="table-light" {
                                tr {
                                    th { "Room" }
                                    th { "Position" }
                                    th { "Team" }
                                    th class="text-end" { "Points" }
                                    th { "Adjudicators" }
                                }
                            }
                            tbody {
                                @for debate in &draw_repr.debates {
                                    @let team_count = debate.teams_of_debate.len();
                                    @for (idx, dt) in debate.teams_of_debate.iter().enumerate() {
                                        @let team = debate.teams.get(&dt.team_id).unwrap();
                                        @let points = team_results_map.get(&(debate.debate.id.clone(), team.id.clone()));
                                        tr {
                                            @if idx == 0 {
                                                td rowspan=(team_count) class="align-middle" {
                                                    @if let Some(room) = &debate.room {
                                                        (room.name)
                                                    } @else {
                                                        span class="text-muted" { "TBA" }
                                                    }
                                                }
                                            }
                                            td {
                                                (side_names::name_of_side(&tournament, dt.side, dt.seq, false))
                                            }
                                            td { (team.name) }
                                            td class="text-end" {
                                                @if let Some(pts) = points {
                                                    @if *pts > 0 {
                                                        span class="badge bg-success" { (pts) }
                                                    } @else {
                                                        span class="badge bg-danger" { (pts) }
                                                    }
                                                } @else {
                                                    span class="text-muted" { "-" }
                                                }
                                            }
                                            @if idx == 0 {
                                                td rowspan=(team_count) class="align-middle" {
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
                }
            })
            .render(),
    )
}
