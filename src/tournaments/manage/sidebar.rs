use hypertext::prelude::*;
use hypertext::{Renderable, maud};
use itertools::Itertools;

use crate::tournaments::{Tournament, rounds::TournamentRounds};

pub struct SidebarWrapper<'r, R: Renderable> {
    pub tournament: &'r Tournament,
    pub rounds: &'r TournamentRounds,
    pub children: R,
}

impl<R: Renderable> Renderable for SidebarWrapper<'_, R> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            div class="container-fluid h-100" {
                div class="row h-100" {
                    Sidebar tournament=(&self.tournament) rounds=(&self.rounds);
                    div class="col-12 col-md-9 col-lg-10" {
                        div class="p-3" {
                            (self.children)
                        }
                    }
                }
            }
        }
        .render_to(buffer);
    }
}

pub struct Sidebar<'r> {
    pub tournament: &'r Tournament,
    pub rounds: &'r TournamentRounds,
}

impl<'r> Renderable for Sidebar<'r> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        let grouped_rounds = self.rounds.all_grouped_by_seq();

        maud! {
            div class="col-12 col-md-3 col-lg-2 order-last order-md-first p-0" {
                div class="p-3 text-white flex-shrink-0 h-100" style="background-color: #3b224c;" {
                    a href="/" class="d-flex align-items-center pb-3 mb-3 link-dark text-decoration-none border-bottom" {
                        span class="fs-5 fw-semibold text-white" {
                            "Navigation"
                        }
                    }
                    ul class="list-unstyled ps-0" {
                        @for level in &grouped_rounds {
                            li class="list-unstyled mb-1" {
                                strong class="mb-1" {
                                    @for (i, round) in level.iter().enumerate() {
                                        @if i > 0 {
                                            ", "
                                        }
                                        (round.name)
                                    }
                                }

                                ul class="list-unstyled fw-normal pb-1 small" {
                                    li {
                                        a class="link-light" href=(format!("/tournaments/{}/rounds/{}", self.tournament.id, level[0].seq)) {
                                            "Setup"
                                        }
                                    }

                                    li {
                                        // TODO: add this route
                                        // TODO: make it possible to edit the draw
                                        // for arbitrary (uncompleted) rounds at
                                        // the same time
                                        a class="link-light"
                                          href=(
                                            format!(
                                              "/tournaments/{}/rounds/draws/edit?rounds={}",
                                              self.tournament.id,
                                              level.iter().map(|l| l.id.clone()).join(",")
                                            )
                                          ) {
                                            "Manage draws"
                                        }
                                    }

                                    li {
                                        a class="link-light" href=(format!("/tournaments/{}/rounds/{}", self.tournament.id, level[0].seq)) {
                                            "Briefing room"
                                        }
                                    }

                                    li {
                                        a class="link-light" href=(format!("/tournaments/{}/rounds/{}", self.tournament.id, level[0].seq)) {
                                            "Manage ballots"
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
