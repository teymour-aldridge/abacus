use std::collections::HashMap;

use diesel::prelude::*;
use hypertext::{Renderable, maud, prelude::*};
use rocket::get;

use crate::{
    auth::User,
    schema::tournament_rounds,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        participants::TournamentParticipants,
        rounds::{
            Round, TournamentRounds,
            draws::{DebateRepr, RoundDrawRepr, manage::DrawForRound},
        },
    },
    util_resp::{StandardResponse, success},
};

#[get("/tournaments/<tid>/rounds/<rid>", rank = 2)]
pub async fn view_tournament_rounds_page(
    tid: &str,
    rid: i64,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tid, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let rounds = tournament_rounds::table
        .filter(tournament_rounds::tournament_id.eq(&tid))
        .filter(tournament_rounds::seq.eq(rid))
        .load::<Round>(&mut *conn)
        .unwrap();

    let reprs = {
        let mut map = HashMap::new();
        for round in &rounds {
            if round.draw_status != "N" {
                let repr = RoundDrawRepr::of_round(round.clone(), &mut *conn);

                map.insert(round.id.clone(), repr);
            }
        }
        map
    };

    let all_rounds =
        TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let participants = TournamentParticipants::load(&tournament.id, &mut *conn);

    success(
        Page::new()
            .tournament(tournament.clone())
            .user(user)
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&all_rounds) {
                    h1 {
                        "Rounds "
                            @for (i, round) in rounds.iter().enumerate() {
                                @if i > 0 {
                                    ", "
                                }
                                (round.name)
                            }
                    }
                    @for round in &rounds {
                        h3 {
                            (round.name)
                        }

                        @let repr = if round.draw_status != "N" {
                            Some(reprs.get(&round.id).unwrap())
                        } else {
                            None
                        };

                        ul class="list-group list-group-horizontal" {
                            li class="list-group-item" {
                                a href=(format!("/tournaments/{}/rounds/{}/edit",
                                        tournament.id,
                                        round.id))
                                {
                                    "Edit round details"
                                }
                            }
                            @if round.draw_status == "D" {
                                li class="list-group-item" {
                                    a href=(format!("/tournaments/{}/rounds/{}/draw/edit",
                                            tournament.id,
                                            round.id))
                                    {
                                        "Edit draw"
                                    }
                                }
                            }
                        }

                        @if round.draw_status != "N" {
                            @let renderer = DrawForRound {
                                tournament: &tournament,
                                repr: &repr.as_ref().unwrap(),
                                actions: |_: &DebateRepr| maud! {
                                    "TODO"
                                },
                                participants: &participants
                            };
                            (renderer)
                        } @else {
                            a href=(format!("/tournaments/{}/rounds/{}/draws/create",
                                    tournament.id,
                                    round.id)) class="btn btn-primary" {
                                "Generate Draw"
                            }
                        }
                    }
                }
            })
            .render(),
    )
}
