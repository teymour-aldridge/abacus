use std::collections::HashMap;

use axum::extract::Path;
use diesel::prelude::*;
use hypertext::prelude::*;

use crate::{
    auth::User,
    schema::tournament_teams,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        rounds::{Round, draws::RoundDrawRepr},
        teams::Team,
    },
    util_resp::{StandardResponse, success},
};

pub async fn view_active_draw_page(
    Path((tournament_id, round_seq)): Path<(String, i64)>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;

    let rounds = Round::of_seq(round_seq, &tournament_id, &mut *conn);
    if rounds.is_empty() {
        return crate::util_resp::err_not_found();
    }

    if rounds.iter().all(|r| r.results_published_at.is_some()) {
        return crate::util_resp::see_other_ok(axum::response::Redirect::to(
            &format!("/tournaments/{tournament_id}/rounds/{round_seq}/results"),
        ));
    }

    let draws = rounds
        .iter()
        .filter(|r| r.draw_status == "R")
        .map(|round| RoundDrawRepr::of_round(round.clone(), &mut *conn))
        .collect::<Vec<_>>();

    let teams = tournament_teams::table
        .filter(tournament_teams::tournament_id.eq(&tournament_id))
        .load::<Team>(&mut *conn)
        .unwrap()
        .into_iter()
        .map(|t| (t.id.clone(), t))
        .collect::<HashMap<_, _>>();

    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .body(maud! {
                div class="p-3" {
                    h1 {
                        "Draw for "
                        @if rounds.len() == 1 {
                            (rounds[0].name)
                        } @else {
                            "Concurrent Rounds"
                        }
                    }

                    @if draws.is_empty() {
                        div class="alert alert-info" {
                            "The draw for these rounds is not yet public."
                        }
                    } @else {
                        @if draws.len() < rounds.len() {
                            @let released_ids: std::collections::HashSet<_> = draws.iter().map(|d| &d.round.id).collect();
                            @for round in &rounds {
                                @if !released_ids.contains(&round.id) {
                                    div class="alert alert-info" {
                                        "The draw for " (round.name) " is not yet available."
                                    }
                                }
                            }
                        }

                        @for draw in &draws {
                            @if draw.round.draw_released_at.is_some() {
                                h3 {
                                    (draw.round.name)
                                }
                                table class="table" {
                                    thead {
                                        tr {
                                            th scope="col" { "#" }
                                            @for i in 0..tournament.teams_per_side {
                                                th scope="col" {
                                                    "Prop " (i + 1)
                                                }
                                                th scope="col" {
                                                    "Opp " (i + 1)
                                                }
                                            }
                                            th scope="col" { "Panel" }
                                        }
                                    }
                                    tbody {
                                        @for (i, debate) in draw.debates.iter().enumerate() {
                                            tr {
                                                th scope="row" { (i + 1) }
                                                @for debate_team in &debate.teams_of_debate {
                                                    td {
                                                        a href = (format!("/tournaments/{tournament_id}/teams/{}", &debate_team.team_id)) {
                                                            (teams.get(&debate_team.team_id).map(|t| t.name.as_str()).unwrap_or("Unknown Team"))
                                                        }
                                                    }
                                                }
                                                td {
                                                    @for judge in &debate.judges_of_debate {
                                                        span class="badge bg-secondary me-1" {
                                                            (debate.judges.get(&judge.judge_id).map(|j| j.name.as_str()).unwrap_or("Unknown Judge"))
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
