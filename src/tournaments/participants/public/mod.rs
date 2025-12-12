use axum::extract::Path;
use hypertext::{maud, prelude::*};

use crate::{
    auth::User,
    state::Conn,
    template::Page,
    tournaments::{Tournament, participants::TournamentParticipants},
    util_resp::{StandardResponse, success},
};

pub async fn public_participants_page(
    Path(tournament_id): Path<String>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tournament_id, &mut *conn)?;
    let participants = TournamentParticipants::load(&tournament_id, &mut *conn);

    success(
        Page::new()
            .user_opt(user)
            .tournament(tournament.clone())
            .body(maud! {
                div class="container-fluid p-3" {
                    h1 { "Participants" }
                    
                    @if !participants.teams.is_empty() {
                        h2 class="mt-4" { "Teams" }
                        div class="table-responsive" {
                            table class="table table-striped" {
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
                                            td { (participants.canonical_name_of_team(team)) }
                                            td {
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

                    @if !participants.judges.is_empty() {
                        h2 class="mt-4" { "Judges" }
                        div class="table-responsive" {
                            table class="table table-striped" {
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
                                            td { (judge.name) }
                                            td {
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
            })
            .render(),
    )
}
