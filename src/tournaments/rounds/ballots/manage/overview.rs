use diesel::prelude::*;
use hypertext::prelude::*;
use itertools::Itertools;
use rocket::get;

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

#[get("/tournaments/<tournament_id>/rounds/<seq>/ballots")]
pub async fn admin_ballot_of_seq_overview(
    tournament_id: &str,
    seq: i64,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;

    let all_rounds =
        TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();

    let rounds = tournament_rounds::table
        .filter(tournament_rounds::seq.eq(seq))
        .load::<Round>(&mut *conn)
        .unwrap();

    if rounds.is_empty() {
        return err_not_found();
    }

    let debates = tournament_debates::table
        .inner_join(
            tournament_rounds::table.on(tournament_rounds::seq
                .eq(seq)
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

    success(
        Page::new()
            .user(user)
            .tournament(tournament.clone())
            .body(maud! {
                SidebarWrapper rounds=(&all_rounds) tournament=(&tournament) {
                    table class="table" {
                        thead {
                            tr {
                                th scope="col" {
                                    "Round"
                                }
                                th scope="col" {
                                    "Debate #"
                                }
                                th scope="col" {
                                    "Ballot statuses"
                                }
                            }
                        }
                        tbody {
                            @for (debate, ballots) in ballot_sets.iter() {
                                tr {
                                    th scope="col" {
                                        (rounds.iter().find(|round| round.id == debate.debate.round_id).unwrap().name)
                                    }
                                    th scope="col" {
                                        (debate.debate.number)
                                    }
                                    td {
                                        @if ballots.is_empty() {
                                            div class="badge rounded-pill bg-warning text-dark" {
                                                "No ballots yet"
                                            }
                                        } @else {
                                            // todo: warnings

                                            @for judge in &debate.judges_of_debate {
                                                @match ballots.iter().find(|ballot| ballot.ballot.judge_id == judge.judge_id) {
                                                    Some(ballot) => {
                                                        div class="badge rounded-pill bg-success text-white" {
                                                            (debate.judges[&judge.judge_id].name) ": submitted @ " (ballot.ballot.submitted_at.format("%Y-%m-%d %H:%M:%S").to_string())
                                                        }
                                                    }
                                                    None => {
                                                        div class="badge rounded-pill bg-secondary text-white" {
                                                            (debate.judges[&judge.judge_id].name) ": no ballot"
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
