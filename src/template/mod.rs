//! Templating code.
//!
//! This defines the [`Page`] item, which is used in most of the other parts of
//! this crate.

use hypertext::{Raw, prelude::*};

use crate::{
    auth::User,
    tournaments::{Tournament, rounds::Round},
};
use uuid::Uuid;

pub mod form;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveNav {
    Results,
    Draw,
    Participants,
    Standings,
    Motions,
    Rooms,
}

#[derive(Debug, Clone, Copy)]
struct ChosenColor {
    accent: &'static str,
    accent_deep: &'static str,
    accent_soft: &'static str,
    accent_wash: &'static str,
}

fn colour(id: Uuid) -> ChosenColor {
    let colours = [
        ChosenColor {
            accent: "#6aaec5",
            accent_deep: "#2f6f82",
            accent_soft: "#d8edf3",
            accent_wash: "#f4fbfd",
        },
        ChosenColor {
            accent: "#d889aa",
            accent_deep: "#9b4667",
            accent_soft: "#f3d8e5",
            accent_wash: "#fff7fb",
        },
        ChosenColor {
            accent: "#a9bf62",
            accent_deep: "#687d23",
            accent_soft: "#e8f0c8",
            accent_wash: "#fbfdf2",
        },
        ChosenColor {
            accent: "#d0b75b",
            accent_deep: "#82702a",
            accent_soft: "#f1e5b7",
            accent_wash: "#fffbee",
        },
        ChosenColor {
            accent: "#9c8bc2",
            accent_deep: "#62518b",
            accent_soft: "#e5def2",
            accent_wash: "#faf7ff",
        },
        ChosenColor {
            accent: "#d8906a",
            accent_deep: "#9c5434",
            accent_soft: "#f3d9ca",
            accent_wash: "#fff8f4",
        },
        ChosenColor {
            accent: "#86b8a0",
            accent_deep: "#3f765d",
            accent_soft: "#dcefe6",
            accent_wash: "#f5fcf8",
        },
        ChosenColor {
            accent: "#9db6d8",
            accent_deep: "#526f9a",
            accent_soft: "#dfe9f6",
            accent_wash: "#f6faff",
        },
    ];

    let hash =
        id.as_bytes()
            .iter()
            .fold(0xcbf29ce484222325_u64, |hash, byte| {
                hash.wrapping_mul(0x100000001b3).wrapping_add(*byte as u64)
            });

    colours[(hash as usize) % colours.len()]
}

fn tournament_theme(tournament: Option<&Tournament>) -> String {
    let Some(tournament) = tournament else {
        return String::new();
    };

    let color = Uuid::parse_str(&tournament.id)
        .map(colour)
        .unwrap_or_else(|_| colour(Uuid::nil()));
    let ChosenColor {
        accent,
        accent_deep,
        accent_soft,
        accent_wash,
    } = color;

    format!(
        "--tournament-accent: {accent}; --tournament-accent-deep: {accent_deep}; --tournament-accent-soft: {accent_soft}; --tournament-accent-wash: {accent_wash};"
    )
}

pub struct Page<R1: Renderable, R2: Renderable, R3: Renderable, const TX: bool>
{
    body: Option<R1>,
    user: Option<User<TX>>,
    extra_head: Option<R2>,
    sidebar: Option<R3>,
    tournament: Option<Tournament>,
    current_rounds: Option<Vec<Round>>,
    active_nav: Option<ActiveNav>,
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
            body: Some(body.into()),
            user: self.user,
            extra_head: self.extra_head,
            sidebar: self.sidebar,
            tournament: self.tournament,
            current_rounds: self.current_rounds,
            active_nav: self.active_nav,
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
            active_nav: self.active_nav,
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
            active_nav: self.active_nav,
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

    pub fn active_nav(mut self, nav: ActiveNav) -> Self {
        self.active_nav = Some(nav);
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
            (Raw::dangerously_create("<!DOCTYPE html>"))
            html {
                head {
                    title { "Abacus" }
                    meta charset="utf-8";

                    script src="https://cdn.jsdelivr.net/npm/htmx.org@2.0.7/dist/htmx.min.js" integrity="sha384-ZBXiYtYQ6hJ2Y0ZNoYuI+Nq5MqWBr+chMrS/RkXpNzQCApHEhOt2aY8EJgqwHLkJ" crossorigin="anonymous" {
                    }
                    script src="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/js/bootstrap.bundle.min.js" integrity="sha384-YvpcrYf0tY3lHB60NNkmXc5s9fDVZLESaAA55NDzOxhy9GkcIdslK1eN7N6jIeHz" crossorigin="anonymous" {}
                    link href="https://fonts.googleapis.com/icon?family=Material+Icons" rel="stylesheet";
                    link rel="stylesheet" href="/style.css";
                    meta
                        name="viewport"
                        content="width=device-width, initial-scale=1";

                    @if let Some(extra) = &self.extra_head {
                        (extra)
                    }
                }

                body class=(format!("abacus-shell d-flex flex-column vh-100 overflow-hidden {}", if self.tournament.is_some() { "abacus-has-tournament" } else { "" }))
                    style=(tournament_theme(self.tournament.as_ref())) {

                    div class=(format!("abacus-topbar flex-shrink-0 {}", if self.user.is_some() { "abacus-signed-in-interface" } else { "abacus-public-interface" })) {
                        div class="container-fluid px-4" {
                            div class="abacus-masthead d-flex align-items-center justify-content-between pt-3 pb-2" {
                                div class="abacus-brand-line d-flex align-items-center gap-2" {
                                    a class="abacus-wordmark text-decoration-none" href="/" {
                                        "Abacus"
                                    }
                                    @if let Some(tournament) = &self.tournament {
                                        span class="abacus-breadcrumb-mark" { "/" }
                                        a class="abacus-tournament-link text-decoration-none"
                                            href=(format!("/tournaments/{}", tournament.id)) {
                                            (tournament.name)
                                        }
                                    }
                                }


                                div class="abacus-session-line d-flex align-items-center gap-2 gap-md-3" {
                                    @if self.user.is_some() {
                                        span class="abacus-interface-badge abacus-interface-signed-in" { "Signed in" }
                                    } @else if self.tournament.is_some() {
                                        span class="abacus-interface-badge abacus-interface-public" { "Public view" }
                                    }

                                    @if let Some(user) = &self.user {
                                        a class="abacus-user-link text-decoration-none" href="/user" {
                                            (user.username)
                                        }
                                    } @else {
                                        a class="btn btn-sm btn-outline-secondary me-1" href="/login" { "Sign in" }
                                        a class="btn btn-sm btn-primary" href="/register" { "Sign up" }
                                    }
                                }
                            }

                            @if let Some(tournament) = &self.tournament {
                                nav class="abacus-primary-nav d-flex gap-2 pb-3" aria-label="Tournament navigation" {
                                    @if let Some(rounds) = &self.current_rounds {
                                        @if !rounds.is_empty() {
                                            @let seq = rounds[0].seq;
                                            @let is_draw_pub = tournament.show_draws && rounds.iter().any(|r| r.is_draw_public());
                                            @let is_results_pub = rounds.iter().all(|r| r.is_results_public());

                                            @if is_results_pub {
                                                 a class=(format!("nav-tab-link text-decoration-none {}", if self.active_nav == Some(ActiveNav::Results) { "nav-tab-active" } else { "" }))
                                                     href=(format!("/tournaments/{}/rounds/{}/results", tournament.id, seq)) {
                                                     "Results"
                                                 }
                                             } @else if is_draw_pub {
                                                 a class=(format!("nav-tab-link text-decoration-none {}", if self.active_nav == Some(ActiveNav::Draw) { "nav-tab-active" } else { "" }))
                                                     href=(format!("/tournaments/{}/rounds/{}/draw", tournament.id, seq)) {
                                                     "Draw"
                                                 }
                                             }
                                         }
                                     }

                                     a class=(format!("nav-tab-link text-decoration-none {}", if self.active_nav == Some(ActiveNav::Participants) { "nav-tab-active" } else { "" }))
                                         href=(format!("/tournaments/{}/participants", tournament.id)) {
                                         "Participants"
                                     }

                                     @if tournament.standings_public || tournament.team_tab_public {
                                         a class=(format!("nav-tab-link text-decoration-none {}", if self.active_nav == Some(ActiveNav::Standings) { "nav-tab-active" } else { "" }))
                                             href=(format!("/tournaments/{}/tab/team", tournament.id)) {
                                             "Standings"
                                         }
                                     }

                                     a class=(format!("nav-tab-link text-decoration-none {}", if self.active_nav == Some(ActiveNav::Motions) { "nav-tab-active" } else { "" }))
                                         href=(format!("/tournaments/{}/motions", tournament.id)) {
                                         "Motions"
                                     }

                                     a class=(format!("nav-tab-link text-decoration-none {}", if self.active_nav == Some(ActiveNav::Rooms) { "nav-tab-active" } else { "" }))
                                         href=(format!("/tournaments/{}/rooms", tournament.id)) {
                                         "Rooms"
                                     }
                                }
                            }
                        }
                    }

                    @if let Some(sidebar) = &self.sidebar {
                        div class="abacus-context-nav" {
                            div class="container-fluid px-4" {
                                (sidebar)
                            }
                        }
                    }


                    div class="abacus-page flex-grow-1 overflow-auto" {
                        @if let Some(body) = &self.body {
                            (body)
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
            active_nav: Default::default(),
        }
    }
}
