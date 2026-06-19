use axum::extract::Path;
use hypertext::{maud, prelude::*};

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{
        Tournament, participants::TournamentParticipants, rounds::Round,
    },
    util_resp::{StandardResponse, success},
};

pub async fn public_participants_page(
    Path(tournament_id): Path<String>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let participants = TournamentParticipants::load(&tournament_id, &mut *conn);
    let current_rounds = Round::current_rounds(&tournament_id, &mut *conn);

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .current_rounds(current_rounds)
            .active_nav(crate::template::ActiveNav::Participants)
            .body(maud! {
                div class="container py-5 px-4 public-participants-page" {
                    div class="public-participants-heading" {
                        h1 { "Participants" }
                    }

                    @if !participants.teams.is_empty() {
                        section class="participants-section" {
                            h2 { "Teams" }
                            div class="table-responsive participants-table-wrap" {
                                table class="table participants-table" {
                                    colgroup {
                                        col class="participants-table-index";
                                        col class="participants-table-name";
                                        col class="participants-table-detail";
                                    }
                                    thead {
                                        tr {
                                            th scope="col" { "#" }
                                            th scope="col" { "Team Name" }
                                            th scope="col" { "Speakers" }
                                        }
                                    }
                                    tbody {
                                        @for (idx, team) in participants.teams.values().enumerate() {
                                            tr {
                                                th scope="row" { (idx + 1) }
                                                td class="participant-primary" { (participants.canonical_name_of_team(team)) }
                                                td class="participant-detail" {
                                                    @if let Some(speaker_ids) = participants.team_speakers.get(&team.id) {
                                                        @for (i, speaker_id) in speaker_ids.iter().enumerate() {
                                                            @if let Some(speaker) = participants.speakers.get(speaker_id) {
                                                                @if i > 0 { ", " }
                                                                (speaker.name)
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

                    @if !participants.judges.is_empty() {
                        section class="participants-section" {
                            h2 { "Judges" }
                            div class="table-responsive participants-table-wrap" {
                                table class="table participants-table" {
                                    colgroup {
                                        col class="participants-table-index";
                                        col class="participants-table-name";
                                        col class="participants-table-detail";
                                    }
                                    thead {
                                        tr {
                                            th scope="col" { "#" }
                                            th scope="col" { "Name" }
                                            th scope="col" { "Institution" }
                                        }
                                    }
                                    tbody {
                                        @for (idx, judge) in participants.judges.values().enumerate() {
                                            tr {
                                                th scope="row" { (idx + 1) }
                                                td class="participant-primary" { (judge.name) }
                                                td class="participant-detail" {
                                                    @if let Some(inst_id) = &judge.institution_id {
                                                        @if let Some(inst) = participants.institutions.get(inst_id) {
                                                            (inst.code)
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
