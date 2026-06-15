use hypertext::prelude::*;
use hypertext::{Renderable, maud};

use crate::tournaments::{Tournament, rounds::TournamentRounds};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarPage {
    Setup,
    Draw,
    Briefing,
    Ballots,
    Results,
}

pub struct SidebarWrapper<'r, R: Renderable> {
    pub tournament: &'r Tournament,
    pub rounds: &'r TournamentRounds,
    pub selected_seq: Option<i64>,
    pub active_page: Option<SidebarPage>,
    pub children: R,
}

impl<R: Renderable> Renderable for SidebarWrapper<'_, R> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {

            div class="tournament-round-rail container-fluid px-4 py-3" {
                Sidebar tournament=(&self.tournament) rounds=(&self.rounds) selected_seq=(self.selected_seq) active_page=(self.active_page);
            }
            div class="abacus-content px-4 py-4" {
                (self.children)
            }
        }
        .render_to(buffer);
    }
}

pub struct Sidebar<'r> {
    pub tournament: &'r Tournament,
    pub rounds: &'r TournamentRounds,
    pub selected_seq: Option<i64>,
    pub active_page: Option<SidebarPage>,
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
            div class="round-nav-shell d-flex align-items-center gap-3" {
                span class="admin-context-label d-inline-flex align-items-center gap-1" {
                    "Admin tools"
                }

                div class="dropdown round-selector" {
                    button class="round-selector-button btn btn-sm dropdown-toggle d-flex align-items-center gap-2"
                        type="button" data-bs-toggle="dropdown" aria-expanded="false" {
                        (current_round_name)
                    }
                    ul class="dropdown-menu round-selector-menu shadow-sm" {
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
                            @let is_selected = self.selected_seq == Some(seq);
                            @let is_active = active_round_seq == Some(seq);

                            li {
                                a class=(format!("dropdown-item round-selector-item py-2 {}", if is_selected { "round-selector-item-active" } else { "" }))
                                    href=(url) {
                                    div class="d-flex align-items-center justify-content-between gap-3" {
                                        span { (name) }
                                        @if is_active {
                                            span class="round-current-label" { "current" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div class="round-nav-divider" {}


                @if let Some(seq) = self.selected_seq {
                    nav class="round-stage-nav d-flex align-items-center gap-2" aria-label="Round stages" {
                        a class=(format!("sidebar-stage-tab text-decoration-none {}", if self.active_page == Some(SidebarPage::Setup) { "sidebar-stage-active" } else { "" }))
                            href=(format!("/tournaments/{}/rounds/{}/setup", self.tournament.id, seq)) {
                            "Setup"
                        }

                        a class=(format!("sidebar-stage-tab text-decoration-none {}", if self.active_page == Some(SidebarPage::Draw) { "sidebar-stage-active" } else { "" }))
                            href=(format!("/tournaments/{}/rounds/{}/draw/manage", self.tournament.id, seq)) {
                            "Draw"
                        }

                        a class=(format!("sidebar-stage-tab text-decoration-none {}", if self.active_page == Some(SidebarPage::Briefing) { "sidebar-stage-active" } else { "" }))
                            href=(format!("/tournaments/{}/rounds/{}/briefing", self.tournament.id, seq)) {
                            "Briefing"
                        }

                        a class=(format!("sidebar-stage-tab text-decoration-none {}", if self.active_page == Some(SidebarPage::Ballots) { "sidebar-stage-active" } else { "" }))
                            href=(format!("/tournaments/{}/rounds/{}/ballots", self.tournament.id, seq)) {
                            "Ballots"
                        }

                        a class=(format!("sidebar-stage-tab text-decoration-none {}", if self.active_page == Some(SidebarPage::Results) { "sidebar-stage-active" } else { "" }))
                            href=(format!("/tournaments/{}/rounds/{}/results/manage", self.tournament.id, seq)) {
                            "Results"
                        }
                    }
                }
            }
        }.render_to(buffer);
    }
}
