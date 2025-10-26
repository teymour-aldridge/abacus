use hypertext::prelude::*;
use rocket::get;

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament, manage::sidebar::SidebarWrapper,
        participants::TournamentParticipants, rounds::TournamentRounds,
    },
    util_resp::{StandardResponse, success},
};

#[get("/tournaments/<tournament_id>/participants/privateurls")]
pub async fn view_private_urls(
    tournament_id: &str,
    user: User<true>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    tournament.check_user_is_superuser(&user.id, &mut *conn)?;
    let rounds = TournamentRounds::fetch(&tournament.id, &mut *conn).unwrap();
    let participants = TournamentParticipants::load(&tournament_id, &mut *conn);

    success(
        Page::new()
            .tournament(tournament.clone())
            .user(user)
            .body(maud! {
                SidebarWrapper tournament=(&tournament) rounds=(&rounds) {
                    table class="table" {
                        thead {
                            tr {
                                th scope="col" {
                                    "Name"
                                }
                                th scope="col" {
                                    "Email"
                                }
                                th scope="col" {
                                    "Role"
                                }
                                th scope="col" {
                                    "Private URL"
                                }
                            }
                        }
                        @if !participants.judges.is_empty() {
                            tbody class="table-group-divider" {
                                @for judge in participants.judges.values() {
                                    tr {
                                        th scope="row" {
                                            (judge.name)
                                        }
                                        td {
                                            (judge.email)
                                        }
                                        td {
                                            "Judge"
                                        }
                                        td {
                                            (judge.private_url)
                                        }
                                    }
                                }
                            }
                        }
                        @if !participants.speakers.is_empty() {
                            tbody class="table-group-divider" {
                                @for speaker in participants.speakers.values() {
                                    tr {
                                        th scope="row" {
                                            (speaker.name)
                                        }
                                        td {
                                            (speaker.email)
                                        }
                                        td {
                                            "Speaker"
                                        }
                                        td {
                                            (speaker.private_url)
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
