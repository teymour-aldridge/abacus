use axum::extract::Path;
use hypertext::{Renderable, prelude::*};
use itertools::Itertools;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        participants::TournamentParticipants,
        rounds::{
            TournamentRounds,
            draws::{RoundDrawRepr, manage::ImmutableDrawForRound},
        },
    },
    util_resp::{StandardResponse, success},
};

pub async fn view_draws_page(
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
    let participants = TournamentParticipants::load(&tid, &mut *conn);

    let rounds_with_draws = rounds_in_seq
        .clone()
        .into_iter()
        .map(|round| {
            let draw = if round.draw_status != "none" {
                Some(RoundDrawRepr::of_round(round.clone(), &mut *conn))
            } else {
                None
            };
            (round, draw)
        })
        .collect_vec();

    let current_rounds =
        crate::tournaments::rounds::Round::current_rounds(&tid, &mut *conn);

    success(
        Page::new()
            .active_nav(crate::template::ActiveNav::Draw)
            .user(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .body(maud! {
                SidebarWrapper  tournament=(&tournament) rounds=(&all_rounds) selected_seq=(Some(round_seq)) active_page=(Some(crate::tournaments::manage::sidebar::SidebarPage::Draw)) {
                    div class="d-flex justify-content-between" {
                        h1 {
                            "Draws for rounds with sequence "
                            (round_seq)
                        }
                        a href=(format!("/tournaments/{}/rounds/{}/briefing", &tournament.id, round_seq)) class="btn btn-primary" {
                            "Briefing Room"
                        }
                        a href=(format!("/tournaments/{}/rounds/draws/edit?{}", &tournament.id, rounds_in_seq.iter().map(|r| format!("rounds={}", r.id)).join("&"))) class="btn btn-primary" {
                            "Edit Draw"
                        }
                        a href=(format!("/tournaments/{}/rounds/draws/rooms/edit?{}", &tournament.id, rounds_in_seq.iter().map(|r| format!("rounds={}", r.id)).join("&"))) class="btn btn-primary" {
                            "Edit Rooms"
                        }
                    }

                    @for (round, draw) in &rounds_with_draws {
                        h2 { (round.name) }
                        @if let Some(draw_repr) = draw {
                            (ImmutableDrawForRound {
                                tournament: &tournament,
                                repr: &draw_repr,
                                participants: &participants,
                            })
                        } @else {
                            a href=(format!("/tournaments/{}/rounds/{}/draws/create", &tournament.id, round.id)) class="btn btn-primary" {
                                "Create Draw"
                            }
                        }
                    }
                }
            })
            .render(),
    )
}
