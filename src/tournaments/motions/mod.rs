use axum::extract::Path;
use diesel::prelude::*;
use hypertext::{maud, prelude::*};

use crate::{
    auth::User,
    schema::motions_of_round,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament,
        manage::sidebar::SidebarWrapper,
        rounds::{Motion, Round, TournamentRounds},
    },
    util_resp::{StandardResponse, success},
    widgets::non_public::NonPublic,
};

#[derive(Clone)]
struct MotionsContent<'a> {
    motions: &'a Vec<(Motion, String)>,
}

impl Renderable for MotionsContent<'_> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            h1 { "Motions" }

            @if self.motions.is_empty() {
                p class="text-muted" { "No motions have been released yet." }
            } @else {
                div class="table-responsive" {
                    table class="table table-striped" {
                        thead {
                            tr {
                                th scope="col" { "Round" }
                                th scope="col" { "Motion" }
                                th scope="col" { "Infoslide" }
                            }
                        }
                        tbody {
                            @for (motion, round_name) in self.motions {
                                tr {
                                    td { (round_name) }
                                    td { (motion.motion) }
                                    td {
                                        @if let Some(infoslide) = &motion.infoslide {
                                            (infoslide)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }.render_to(buffer);
    }
}

pub async fn public_motions_page(
    Path(tournament_id): Path<String>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let rounds_data =
        TournamentRounds::fetch(&tournament_id, &mut *conn).unwrap();

    let all_rounds: Vec<Round> = rounds_data
        .prelim
        .iter()
        .chain(rounds_data.elim.iter())
        .cloned()
        .collect();

    let is_admin = if let Some(ref u) = user {
        tournament
            .check_user_is_superuser(&u.id, &mut *conn)
            .is_ok()
    } else {
        false
    };

    let mut query = motions_of_round::table
        .filter(motions_of_round::tournament_id.eq(&tournament_id))
        .into_boxed();

    if !is_admin {
        query = query.filter(motions_of_round::published_at.is_not_null());
    }

    let motions: Vec<(Motion, String)> = query
        .load::<Motion>(&mut *conn)
        .unwrap()
        .into_iter()
        .filter_map(|motion| {
            all_rounds
                .iter()
                .find(|r| r.id == motion.round_id)
                .map(|round| (motion, round.name.clone()))
        })
        .collect();

    let has_unpublished_motions =
        motions.iter().any(|(m, _)| m.published_at.is_none());

    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);

    let motions_content = MotionsContent { motions: &motions };

    if is_admin {
        success(
            Page::new()
                .active_nav(crate::template::ActiveNav::Motions)
                .user_opt(user)
                .tournament(tournament.clone())
                .current_rounds(current_rounds.clone())
                .body(maud! {
                    SidebarWrapper tournament=(&tournament) rounds=(&rounds_data) selected_seq=(current_rounds.first().map(|r| r.seq)) active_page=(None) {
                        @if has_unpublished_motions {
                            (NonPublic {
                                title: "Unpublished Motions",
                                child: motions_content.clone()
                            })
                        } @else {
                            (motions_content)
                        }
                    }
                })
                .render(),
        )
    } else {
        success(
            Page::new()
                .active_nav(crate::template::ActiveNav::Motions)
                .user_opt(user)
                .tournament(tournament.clone())
                .current_rounds(current_rounds)
                .body(maud! {
                    div class="container py-5 px-4" {
                        (motions_content)
                    }
                })
                .render(),
        )
    }
}
