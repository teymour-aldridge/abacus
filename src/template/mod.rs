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

pub struct Page<R1: Renderable, R2: Renderable, const TX: bool> {
    body: Option<R1>,
    user: Option<User<TX>>,
    extra_head: Option<R2>,
    tournament: Option<Tournament>,
    current_rounds: Option<Vec<Round>>,
}

// unfortunate generic argument shenanigans
impl<R1: Renderable, const TX: bool> Page<R1, String, TX> {
    pub fn new() -> Self {
        Default::default()
    }
}

impl<R1: Renderable, R2: Renderable, const TX: bool> Page<R1, R2, TX> {
    pub fn new_full() -> Self {
        Default::default()
    }
}

impl<R1: Renderable, R2: Renderable, const TX: bool> Page<R1, R2, TX> {
    pub fn tournament(mut self, tournament: Tournament) -> Self {
        self.tournament = Some(tournament);
        self
    }

    pub fn body(mut self, body: R1) -> Self {
        self.body = Some(body);
        self
    }

    pub fn user(mut self, user: User<TX>) -> Self {
        self.user = Some(user);
        self
    }

    pub fn extra_head(mut self, content: R2) -> Page<R1, R2, TX> {
        self.extra_head = Some(content);
        self
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

impl<R1: Renderable, R2: Renderable, const TX: bool> Renderable
    for Page<R1, R2, TX>
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
                body class="d-flex flex-column vh-100" {
                    nav class="navbar navbar-expand"
                        style="background-color: #452859; display: flex; justify-content: space-between; align-items: center;"
                        data-bs-theme="dark" {
                        div class="container-fluid" style="display: flex; justify-content: space-between; align-items: center;" {
                            @if let Some(tournament) = &self.tournament {
                                a class="navbar-brand text-white"
                                  href=(format!("/tournaments/{}", tournament.id)) {
                                    (tournament.abbrv)
                                }
                            } @else {
                                a class="navbar-brand text-white" href="/" {
                                    "Home"
                                }
                            }
                            @if let Some(tournament) = &self.tournament {
                                ul class="navbar-nav" style="display: flex; gap: 1rem;" data-bs-theme="dark" {
                                    @if let Some(rounds) = &self.current_rounds {
                                        @if !rounds.is_empty() {
                                            @let seq = rounds[0].seq;
                                            @let is_draw_pub = tournament.show_draws && rounds.iter().any(|r| r.is_draw_public());
                                            @let is_results_pub = rounds.iter().all(|r| r.is_results_public());
                                            (tracing::debug!("Draw pub: {is_draw_pub} & Results pub: {is_results_pub}"))

                                            @if is_results_pub {
                                                li class="nav-item" {
                                                    a class="nav-link text-white" href=(format!("/tournaments/{}/rounds/{}/results", tournament.id, seq)) {
                                                        @if rounds.len() == 1 {
                                                            (format!("Results for {}", rounds[0].name))
                                                        } @else {
                                                            "Current Results"
                                                        }
                                                    }
                                                }
                                            } @else if is_draw_pub {
                                                li class="nav-item" {
                                                    a class="nav-link text-white" href=(format!("/tournaments/{}/rounds/{}/draw", tournament.id, seq)) {
                                                        @if rounds.len() == 1 {
                                                            (format!("Draw for {}", rounds[0].name))
                                                        } @else {
                                                            "Current Draws"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    li class="nav-item" {
                                        a class="nav-link text-white" href=(format!("/tournaments/{}/participants/public", tournament.id)) {
                                            "Participants"
                                        }
                                    }
                                    @if tournament.standings_public || tournament.team_tab_public {
                                        li class="nav-item" {
                                            a class="nav-link text-white" href=(format!("/tournaments/{}/tab/team", tournament.id)) {
                                                "Team Standings"
                                            }
                                        }
                                    }
                                    li class="nav-item" {
                                        a class="nav-link text-white" href=(format!("/tournaments/{}/motions", tournament.id)) {
                                            "Motions"
                                        }
                                    }
                                }
                            }
                            div {
                                ul class="navbar-nav" style="display: flex; gap: 1rem;" data-bs-theme="dark" {
                                    @if let Some(user) = &self.user {
                                        li class="nav-item" {
                                            a class="nav-link text-white" href="/user" {
                                                (user.username)
                                            }
                                        }
                                    } @else {
                                        li class="nav-item" {
                                            a class="nav-link text-white" href="/login" {
                                                "Login"
                                            }
                                        }
                                        li class="nav-item" {
                                            a class="nav-link text-white" href="/register" {
                                                "Register"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div class="flex-grow-1" {
                        @if let Some(body) = &self.body {
                            (body)
                        }
                    }
                }
            }
        }.render_to(buffer)
    }
}

impl<R1: Renderable, R2: Renderable, const TX: bool> Default
    for Page<R1, R2, TX>
{
    fn default() -> Self {
        Self {
            body: Default::default(),
            user: Default::default(),
            tournament: Default::default(),
            extra_head: Default::default(),
            current_rounds: Default::default(),
        }
    }
}
