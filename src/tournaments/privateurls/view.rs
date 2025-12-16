use axum::extract::Path;
use diesel::{connection::LoadConnection, prelude::*};
use hypertext::{Renderable, maud, prelude::*};
use itertools::Itertools;

use crate::{
    auth::User,
    schema::{
        tournament_debate_judges, tournament_debate_teams, tournament_debates,
        tournament_institutions, tournament_team_speakers,
    },
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        participants::{Judge, Participant, Speaker},
        rounds::{
            Round, TournamentRounds,
            draws::{DebateRepr, DebateTeam},
            side_names::name_of_side,
        },
    },
    util_resp::{StandardResponse, success},
};

pub async fn private_url_page(
    Path((tournament_id, private_url)): Path<(String, String)>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let participant = Participant::of_private_url_and_tournament(
        &tournament_id,
        &private_url,
        &mut *conn,
    )?;

    match participant {
        Participant::Speaker(speaker) => {
            private_url_page_of_speaker(tournament, speaker, user, &mut *conn)
        }
        Participant::Judge(judge) => {
            private_url_page_of_judge(tournament, judge, user, &mut *conn)
        }
    }
}

fn private_url_page_of_speaker(
    tournament: Tournament,
    speaker: Speaker,
    user: Option<User<true>>,
    conn: &mut impl LoadConnection<Backend = diesel::sqlite::Sqlite>,
) -> StandardResponse {
    let current_rounds = Round::current_rounds(&tournament.id, conn);

    let current_debate_info = if !current_rounds.is_empty() {
        // todo: should make sure that teams can only ever be allocated to ONE
        // round per sequence number

        // todo: make sure that when speakers are on multiple teams, only one
        // of those teams can be marked available (and placed on the draw)
        // across all rounds (!)

        let team_debate = tournament_debate_teams::table
            .filter(
                tournament_debate_teams::team_id.eq_any(
                    tournament_team_speakers::table
                        .filter(
                            tournament_team_speakers::speaker_id
                                .eq(&speaker.id),
                        )
                        .select(tournament_team_speakers::team_id),
                ),
            )
            .filter(tournament_debate_teams::debate_id.eq_any({
                tournament_debates::table
                    .filter(
                        tournament_debates::round_id.eq_any(
                            current_rounds
                                .iter()
                                .map(|round| round.id.clone())
                                .collect::<Vec<_>>(),
                        ),
                    )
                    .select(tournament_debates::id)
            }))
            .first::<DebateTeam>(conn)
            .optional()
            .unwrap();

        if let Some(team_debate) = team_debate {
            let tournament = tournament.clone();
            let debate = DebateRepr::fetch(&team_debate.debate_id, conn);

            Some(maud! {
                div class="card mb-4" {
                    div class="card-body" {
                        h2 class="card-title h5 text-uppercase fw-bold text-muted mb-3" {
                            "In this round"
                        }
                        p class="card-text" {
                            "You are debating as the "
                            (name_of_side(&tournament, team_debate.side, team_debate.seq, false))
                            " team"
                            @match &debate.room {
                                Some(room) => {
                                    " in "
                                    @if let Some(url) = &room.url {
                                        a href=(url) class="text-dark text-decoration-underline fw-bold" {
                                            (room.name)
                                        }
                                    } @else {
                                        span {
                                            (room.name)
                                        }
                                    }
                                }
                                None => {
                                    " (no room assigned)"
                                }
                            }
                        }
                    }
                }
            })
        } else {
            None
        }
    } else {
        None
    };

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .body(maud! {
                div class="container py-5" style="max-width: 800px;" {
                    header class="mb-5" {
                        h1 class="display-4 fw-bold mb-3" {
                            (speaker.name)
                        }
                        span class="badge bg-light text-dark" {
                            "Speaker"
                        }
                    }

                    @if let Some(ref debate_info) = current_debate_info {
                        section class="mb-5" {
                            (debate_info)
                        }
                    }
                }
            })
            .render(),
    )
}

fn private_url_page_of_judge(
    tournament: Tournament,
    judge: Judge,
    user: Option<User<true>>,
    conn: &mut impl LoadConnection<Backend = diesel::sqlite::Sqlite>,
) -> StandardResponse {
    let rounds = TournamentRounds::fetch(&tournament.id, conn).unwrap();
    let current_rounds = Round::current_rounds(&tournament.id, conn)
        .into_iter()
        .filter(|round| round.draw_released_at.is_some())
        .collect_vec();

    let institution_name = if let Some(ref inst_id) = judge.institution_id {
        tournament_institutions::table
            .filter(tournament_institutions::id.eq(inst_id))
            .select(tournament_institutions::name)
            .first::<String>(conn)
            .ok()
    } else {
        None
    };

    let current_debate_info = if !current_rounds.is_empty() {
        let judge_debate = tournament_debate_judges::table
            .filter(tournament_debate_judges::judge_id.eq(&judge.id))
            .filter(tournament_debate_judges::debate_id.eq_any({
                tournament_debates::table
                    .filter(
                        tournament_debates::round_id.eq_any(
                            current_rounds
                                .iter()
                                .map(|round| round.id.clone())
                                .collect::<Vec<_>>(),
                        ),
                    )
                    .select(tournament_debates::id)
            }))
            .select((
                tournament_debate_judges::debate_id,
                tournament_debate_judges::status,
            ))
            .first::<(String, String)>(conn)
            .optional()
            .unwrap();

        if let Some((debate_id, status)) = judge_debate {
            let debate = DebateRepr::fetch(&debate_id, conn);
            let round_id = debate.debate.round_id.clone();
            let tournament_id = tournament.id.clone();
            let judge_private_url = judge.private_url.clone();

            Some(maud! {
                div class="card mb-4" {
                    div class="card-body" {
                        h2 class="card-title mb-3" {
                            "In this round"
                        }
                        p class="card-text" {
                            "You are judging "
                            @if status == "C" {
                                "as the Chair"
                            } @else if status == "P" {
                                "as a Panelist"
                            } @else if status == "T" {
                                "as a Trainee"
                            }
                            @match &debate.room {
                                Some(room) => {
                                    " in "
                                    @if let Some(url) = &room.url {
                                        a href=(url) class="text-dark text-decoration-underline fw-bold" {
                                            (room.name)
                                        }
                                    } @else {
                                        span {
                                            (room.name)
                                        }
                                    }
                                }
                                None => {
                                    " (no room assigned)"
                                }
                            }
                        }
                        a href=(format!("/tournaments/{}/privateurls/{}/rounds/{}/submit", tournament_id, judge_private_url, round_id))
                          class="btn btn-dark btn-lg mt-3" {
                            "Submit Ballot"
                        }
                    }
                }
            })
        } else {
            None
        }
    } else {
        None
    };

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .body(maud! {
                div class="container py-5" style="max-width: 800px;" {
                    header class="mb-5" {
                        h1 class="display-4 fw-bold mb-3" {
                            (judge.name)
                        }
                        span class="badge bg-light text-dark fs-6 me-2" { "Judge" }
                        @if let Some(inst) = &institution_name {
                            span class="badge bg-light text-dark fs-6" { "Institution: " (inst) }
                        } @else {
                            span class="badge bg-light text-dark fs-6" { "Independent Adjudicator" }
                        }
                    }

                    @if let Some(ref debate_info) = current_debate_info {
                        section class="mb-5" {
                            (debate_info)
                        }
                    }

                    @if rounds.prelim.iter().any(|r| r.completed) {
                        section class="mb-5" {
                            h2 class="h4 text-uppercase fw-bold text-secondary mb-4" {
                                "Feedback Submissions"
                            }
                            ul class="list-unstyled" {
                                @for round in rounds.prelim.iter().filter(|r| r.completed) {
                                    li class="mb-3" {
                                        a href=(
                                            format!("/tournaments/{}/privateurls/{}/rounds/{}/feedback/submit", tournament.id, judge.private_url, round.id)
                                        ) class="btn btn-outline-dark" {
                                            "Submit feedback for " (round.name)
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
