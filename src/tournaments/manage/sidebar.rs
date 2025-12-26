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

            div class="border-bottom px-4 pt-2 pb-0" style="background-color: #f8f9fa;" {
                Sidebar tournament=(&self.tournament) rounds=(&self.rounds) selected_seq=(self.selected_seq) active_page=(self.active_page);
            }
            div class="px-4 py-4" {
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
            div class="d-flex align-items-center gap-4" {
                div class="dropdown" {
                    button class="btn btn-sm dropdown-toggle d-flex align-items-center gap-2"
                        type="button" data-bs-toggle="dropdown" aria-expanded="false"
                        style="font-weight: 600; color: #212529; background: transparent; border: none; padding-left: 0;" {
                        span class="material-icons" style="font-size: 1rem; color: #6c757d;" { "view_agenda" }
                        (current_round_name)
                    }
                    ul class="dropdown-menu shadow-sm" style="min-width: 200px;" {
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
                                a class="dropdown-item py-2"
                                    href=(url)
                                    style=(if is_selected { "font-weight: 600; color: #212529;" } else { "color: #495057;" }) {
                                    div class="d-flex align-items-center justify-content-between gap-3" {
                                        span { (name) }
                                        @if is_active {
                                            span class="text-muted" style="font-size: 0.6875rem; font-weight: 400; white-space: nowrap;" { "current" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div class="vr opacity-25" {}


                @if let Some(seq) = self.selected_seq {
                    nav class="d-flex align-items-center gap-3" style="margin-bottom: -1px;" {
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
