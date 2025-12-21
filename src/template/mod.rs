//! Templating code.
//!
//! This defines the [`Page`] item, which is used in most of the other parts of
//! this crate.

use hypertext::prelude::*;

use crate::{
    auth::User,
    tournaments::{Tournament, rounds::Round},
};

pub mod form;

pub struct Page<R1: Renderable, R2: Renderable, R3: Renderable, const TX: bool>
{
    body: Option<R1>,
    user: Option<User<TX>>,
    extra_head: Option<R2>,
    sidebar: Option<R3>,
    tournament: Option<Tournament>,
    current_rounds: Option<Vec<Round>>,
}

// unfortunate generic argument shenanigans
impl<const TX: bool> Page<String, String, String, TX> {
    pub fn new() -> Self {
        Default::default()
    }
}

impl<R1: Renderable, R2: Renderable, R3: Renderable, const TX: bool>
    Page<R1, R2, R3, TX>
{
    pub fn new_full() -> Self {
        Default::default()
    }
}

impl<R1: Renderable, R2: Renderable, R3: Renderable, const TX: bool>
    Page<R1, R2, R3, TX>
{
    pub fn tournament(mut self, tournament: Tournament) -> Self {
        self.tournament = Some(tournament);
        self
    }

    pub fn body<NewR1: Renderable>(
        self,
        body: NewR1,
    ) -> Page<NewR1, R2, R3, TX> {
        Page {
            body: Some(body),
            user: self.user,
            extra_head: self.extra_head,
            sidebar: self.sidebar,
            tournament: self.tournament,
            current_rounds: self.current_rounds,
        }
    }

    pub fn user(mut self, user: User<TX>) -> Self {
        self.user = Some(user);
        self
    }

    pub fn extra_head<NewR2: Renderable>(
        self,
        content: NewR2,
    ) -> Page<R1, NewR2, R3, TX> {
        Page {
            body: self.body,
            user: self.user,
            extra_head: Some(content),
            sidebar: self.sidebar,
            tournament: self.tournament,
            current_rounds: self.current_rounds,
        }
    }

    pub fn sidebar<NewR3: Renderable>(
        self,
        sidebar: NewR3,
    ) -> Page<R1, R2, NewR3, TX> {
        Page {
            body: self.body,
            user: self.user,
            extra_head: self.extra_head,
            sidebar: Some(sidebar),
            tournament: self.tournament,
            current_rounds: self.current_rounds,
        }
    }

    pub fn user_opt(mut self, user: Option<User<TX>>) -> Self {
        self.user = user;
        self
    }

    pub fn current_rounds(mut self, rounds: Vec<Round>) -> Self {
        self.current_rounds = Some(rounds);
        self
    }
}

impl<R1: Renderable, R2: Renderable, R3: Renderable, const TX: bool> Renderable
    for Page<R1, R2, R3, TX>
{
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            html {
                head {
                    title { "Abacus" }
                    script src="https://cdn.jsdelivr.net/npm/htmx.org@2.0.7/dist/htmx.min.js" integrity="sha384-ZBXiYtYQ6hJ2Y0ZNoYuI+Nq5MqWBr+chMrS/RkXpNzQCApHEhOt2aY8EJgqwHLkJ" crossorigin="anonymous" {
                    }
                    script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/js/bootstrap.bundle.min.js" integrity="sha384-YvpcrYf0tY3lHB60NNkmXc5s9fDVZLESaAA55NDzOxhy9GkcIdslK1eN7N6jIeHz" crossorigin="anonymous" {}
                    link href="https://fonts.googleapis.com/icon?family=Material+Icons" rel="stylesheet";
                    style {
                        (include_str!(concat!(env!("OUT_DIR"), "/style.css")))
                    }
                    meta
                        name="viewport"
                        content="width=device-width, initial-scale=1";
                    @if let Some(extra) = &self.extra_head {
                        (extra)
                    }
                }
                body class="d-flex flex-column vh-100 overflow-hidden" {
                    // Header / Navbar (Full Width)
                    div class="border-bottom bg-white flex-shrink-0" {
                        div class="container-fluid py-3" {
                            div class="d-flex align-items-center justify-content-between mb-3" {
                                div class="d-flex align-items-center gap-3" {
                                    @if let Some(tournament) = &self.tournament {
                                        a class="h2 mb-0 text-dark text-decoration-none fw-bold"
                                            href=(format!("/tournaments/{}", tournament.id)) {
                                            (tournament.name)
                                        }
                                    } @else {
                                        a class="h2 mb-0 text-dark text-decoration-none fw-bold" href="/" {
                                            "Abacus"
                                        }
                                    }
                                }

                                div {
                                    @if let Some(user) = &self.user {
                                        a class="text-secondary text-decoration-none fw-semibold" href="/user" {
                                            (user.username)
                                        }
                                    } @else {
                                        a class="btn btn-sm btn-outline-primary me-2" href="/login" { "Login" }
                                        a class="btn btn-sm btn-primary" href="/register" { "Register" }
                                    }
                                }
                            }

                            @if let Some(tournament) = &self.tournament {
                                div {
                                    nav class="nav nav-pills gap-3" {
                                        @if let Some(rounds) = &self.current_rounds {
                                            @if !rounds.is_empty() {
                                                @let seq = rounds[0].seq;
                                                @let is_draw_pub = tournament.show_draws && rounds.iter().any(|r| r.is_draw_public());
                                                @let is_results_pub = rounds.iter().all(|r| r.is_results_public());

                                                @if is_results_pub {
                                                    a class="nav-link px-0 d-flex align-items-center text-secondary" href=(format!("/tournaments/{}/rounds/{}/results", tournament.id, seq)) {
                                                        span class="material-icons me-2 fs-5" { "assessment" }
                                                        "Results"
                                                    }
                                                } @else if is_draw_pub {
                                                    a class="nav-link px-0 d-flex align-items-center text-secondary" href=(format!("/tournaments/{}/rounds/{}/draw", tournament.id, seq)) {
                                                        span class="material-icons me-2 fs-5" { "grid_view" }
                                                        "Draw"
                                                    }
                                                }
                                            }
                                        }

                                        a class="nav-link px-0 d-flex align-items-center text-secondary" href=(format!("/tournaments/{}/participants", tournament.id)) {
                                            span class="material-icons me-2 fs-5" { "groups" }
                                            "Participants"
                                        }

                                        @if tournament.standings_public || tournament.team_tab_public {
                                            a class="nav-link px-0 d-flex align-items-center text-secondary" href=(format!("/tournaments/{}/tab/team", tournament.id)) {
                                                span class="material-icons me-2 fs-5" { "leaderboard" }
                                                "Standings"
                                            }
                                        }

                                        a class="nav-link px-0 d-flex align-items-center text-secondary" href=(format!("/tournaments/{}/motions", tournament.id)) {
                                            span class="material-icons me-2 fs-5" { "article" }
                                            "Motions"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Secondary Sidebar / Toolbar (Horizontal)
                    @if let Some(sidebar) = &self.sidebar {
                        div class="border-bottom bg-light" {
                            (sidebar)
                        }
                    }

                    // Main Content (Full Width)
                    div class="flex-grow-1 overflow-auto bg-white" {
                        div class="container-fluid px-4 py-4" {
                            @if let Some(body) = &self.body {
                                (body)
                            }
                        }
                    }
                }
            }
        }.render_to(buffer)
    }
}

impl<R1: Renderable, R2: Renderable, R3: Renderable, const TX: bool> Default
    for Page<R1, R2, R3, TX>
{
    fn default() -> Self {
        Self {
            body: Default::default(),
            user: Default::default(),
            tournament: Default::default(),
            extra_head: Default::default(),
            sidebar: Default::default(),
            current_rounds: Default::default(),
        }
    }
}
