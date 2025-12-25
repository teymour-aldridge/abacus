use hypertext::prelude::*;
use hypertext::{Renderable, maud};

use crate::tournaments::{Tournament, rounds::TournamentRounds};

pub struct SidebarWrapper<'r, R: Renderable> {
    pub tournament: &'r Tournament,
    pub rounds: &'r TournamentRounds,
    pub selected_seq: Option<i64>,
    pub active_page: Option<&'r str>,
    pub children: R,
}

impl<R: Renderable> Renderable for SidebarWrapper<'_, R> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            div class="d-flex flex-column h-100" {
                div class="border-bottom bg-light" {
                    Sidebar tournament=(&self.tournament) rounds=(&self.rounds) selected_seq=(self.selected_seq) active_page=(self.active_page);
                }
                div class="flex-grow-1 py-4" {
                    (self.children)
                }
            }
        }
        .render_to(buffer);
    }
}

pub struct Sidebar<'r> {
    pub tournament: &'r Tournament,
    pub rounds: &'r TournamentRounds,
    pub selected_seq: Option<i64>,
    pub active_page: Option<&'r str>,
}

impl<'r> Renderable for Sidebar<'r> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        let grouped_rounds = self.rounds.all_grouped_by_seq();

        let active_round_seq = grouped_rounds
            .iter()
            .find(|level| level.iter().any(|r| !r.completed))
            .or_else(|| grouped_rounds.last())
            .map(|level| level[0].seq);

        let current_round_name = if let Some(seq) = self.selected_seq {
            grouped_rounds
                .iter()
                .find(|level| level.first().map(|r| r.seq) == Some(seq))
                .map(|level| {
                    level
                        .iter()
                        .map(|r| r.name.as_str())
                        .collect::<Vec<_>>()
                        .join("/")
                })
                .unwrap_or_else(|| "Select Round".to_string())
        } else {
            "Select Round".to_string()
        };

        maud! {
            div class="py-2" {
                div class="d-flex align-items-center gap-3" {
                    // Round Selector Dropdown
                    div class="dropdown" {
                        button class="btn btn-sm btn-light border dropdown-toggle fw-bold d-flex align-items-center gap-2 bg-white"
                            type="button" data-bs-toggle="dropdown" aria-expanded="false" {
                            span class="material-icons fs-6 text-secondary" { "view_agenda" }
                            (current_round_name)
                        }
                        ul class="dropdown-menu shadow-sm" {
                            @for level in &grouped_rounds {
                                @let seq = level[0].seq;
                                @let name = level.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join("/");
                                @let draw_status = level.iter().map(|r| r.draw_status.as_str()).next().unwrap_or("none");
                                @let url = if draw_status == "none" {
                                    format!("/tournaments/{}/rounds/{}/setup", self.tournament.id, seq)
                                } else if draw_status == "generated" || draw_status == "draft" {
                                    format!("/tournaments/{}/rounds/{}/draw/manage", self.tournament.id, seq)
                                } else {
                                    format!("/tournaments/{}/rounds/{}/briefing", self.tournament.id, seq)
                                };

                                li {
                                    a class="dropdown-item d-flex align-items-center gap-2"
                                        href=(url) {
                                        (name)
                                        @if active_round_seq == Some(seq) {
                                            " (active)"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div class="vr opacity-25" {}

                    // Action Navigation
                    @if let Some(seq) = self.selected_seq {
                        @let rounds_at_seq = grouped_rounds.iter().find(|level| level[0].seq == seq);
                        @let active_stage = if active_round_seq == Some(seq) {
                            if let Some(rounds) = rounds_at_seq {
                                let r = &rounds[0]; // Assuming concurrent rounds share status for simplicity, or taking first
                                if r.completed {
                                    "results"
                                } else if r.draw_status == "released_full" {
                                    "ballots"
                                } else if r.draw_status == "released_teams" {
                                    "briefing"
                                } else if r.draw_status == "generated" || r.draw_status == "draft" {
                                    "draw"
                                } else {
                                    "setup"
                                }
                            } else {
                                ""
                            }
                        } else {
                            ""
                        };

                        nav class="nav nav-pills gap-1" {
                            a class=(format!("nav-link py-1 px-3 fw-medium {}", if self.active_page == Some("setup") { "text-dark fw-bold" } else { "text-secondary" }))
                                href=(format!("/tournaments/{}/rounds/{}/setup", self.tournament.id, seq)) {
                                "Setup"
                                @if active_stage == "setup" { " (active)" }
                            }

                            a class=(format!("nav-link py-1 px-3 fw-medium {}", if self.active_page == Some("draw") { "text-dark fw-bold" } else { "text-secondary" }))
                                href=(format!("/tournaments/{}/rounds/{}/draw/manage", self.tournament.id, seq)) {
                                "Draw"
                                @if active_stage == "draw" { " (active)" }
                            }

                            a class=(format!("nav-link py-1 px-3 fw-medium {}", if self.active_page == Some("briefing") { "text-dark fw-bold" } else { "text-secondary" }))
                                href=(format!("/tournaments/{}/rounds/{}/briefing", self.tournament.id, seq)) {
                                "Briefing"
                                @if active_stage == "briefing" { " (active)" }
                            }

                            a class=(format!("nav-link py-1 px-3 fw-medium {}", if self.active_page == Some("ballots") { "text-dark fw-bold" } else { "text-secondary" }))
                                href=(format!("/tournaments/{}/rounds/{}/ballots", self.tournament.id, seq)) {
                                "Ballots"
                                @if active_stage == "ballots" { " (active)" }
                            }

                            a class=(format!("nav-link py-1 px-3 fw-medium {}", if self.active_page == Some("results") { "text-dark fw-bold" } else { "text-secondary" }))
                                href=(format!("/tournaments/{}/rounds/{}/results/manage", self.tournament.id, seq)) {
                                "Results"
                                @if active_stage == "results" { " (active)" }
                            }
                        }
                    }
                }
            }
        }.render_to(buffer);
    }
}
