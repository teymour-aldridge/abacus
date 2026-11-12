use axum::extract::Path;
use hypertext::prelude::*;

use crate::{
    auth::User,
    permission::TournamentAccess,
    state::Conn,
    template::{ActiveNav, Page},
    tournaments::{Tournament, standings::compute::speaker::SpeakerStandings},
    ui::{
        self, View,
        presentation::{Publication, publication},
    },
    util_resp::{StandardResponse, success},
};

pub async fn public_speaker_tab_page(
    Path(tid): Path<String>,
    user: Option<User<true>>,
    mut conn: Conn<true>,
) -> StandardResponse {
    let tournament = Tournament::fetch(&tid, &mut *conn)?;
    let access = TournamentAccess::resolve(
        &tid,
        user.as_ref().map(|u| u.id.as_str()),
        &mut *conn,
    )?;
    // Team-tab publication and points-only standings do not publish speakers.
    let public = tournament.speaker_tab_public;
    let visible = public || access.is_superuser();
    let standings = if visible {
        Some(SpeakerStandings::fetch(&tid, &mut *conn)?)
    } else {
        None
    };
    let standings = standings.as_ref();
    let tid = tid.as_str();
    let view = ui::html(|ctx: &TournamentAccess, out| {
        let content = ui::protected(
            |_: &TournamentAccess| visible,
            || {
                ui::html(|_: &TournamentAccess, out| {
                    maud! {
                        @if let Some(standings) = standings {
                            @if standings.rows.is_empty() {
                                p role="status" { "No speaker standings are available yet." }
                            } @else {
                                div class="table-responsive" {
                                    table class="table" {
                                        thead { tr {
                                            th scope="col" { "Rank" }
                                            th scope="col" { "Speaker name" }
                                            @for metric in &standings.metrics {
                                                th scope="col" { (metric.to_string()) }
                                            }
                                        } }
                                        tbody {
                                            @for row in &standings.rows {
                                                tr {
                                                    th scope="row" { @if row.tied { "=" } (row.rank) }
                                                    td { (row.name) }
                                                    @for value in &row.metrics {
                                                        td { (format!("{value:.2}")) }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }.render_to(out)
                })
            },
            || {
                ui::html(|_: &TournamentAccess, out| {
                    maud! {
                        p role="status" { "Speaker standings are not yet public." }
                    }.render_to(out)
                })
            },
        );
        maud! {
            div class="container py-4" {
                h1 { "Speaker standings" }
                nav class="d-flex gap-3 mb-3" aria-label="Standings navigation" {
                    a href=(format!("/tournaments/{tid}/tab/team")) { "Teams" }
                    a href=(format!("/tournaments/{tid}/tab/speaker")) aria-current="page" { "Speakers" }
                }
                @if ctx.is_superuser() {
                    (publication::<TournamentAccess>(
                        "Speaker tab",
                        if public { Publication::Public } else { Publication::Unpublished },
                    ).bind(ctx))
                }
                (content.bind(ctx))
            }
        }.render_to(out)
    });
    success(
        Page::new()
            .active_nav(ActiveNav::Standings)
            .tournament(tournament)
            .user_opt(user)
            .body(view.bind(&access))
            .render(),
    )
}
