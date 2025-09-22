//! Templating code.
//!
//! This defines the [`Page`] item, which is used in most of the other parts of
//! this crate.

use hypertext::prelude::*;

use crate::{auth::User, tournaments::Tournament};

pub mod form;

pub struct Page<R: Renderable, const TX: bool> {
    body: Option<R>,
    user: Option<User<TX>>,
    hx_ext: Option<String>,
    tournament: Option<Tournament>,
}

impl<R: Renderable, const TX: bool> Page<R, TX> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn tournament(mut self, tournament: Tournament) -> Self {
        self.tournament = Some(tournament);
        self
    }

    pub fn body(mut self, body: R) -> Self {
        self.body = Some(body);
        self
    }

    pub fn user(mut self, user: User<TX>) -> Self {
        self.user = Some(user);
        self
    }

    pub fn user_opt(mut self, user: Option<User<TX>>) -> Self {
        self.user = user;
        self
    }

    pub fn hx_ext(mut self, set: &str) -> Self {
        self.hx_ext = Some(set.to_string());
        self
    }
}

impl<R: Renderable, const TX: bool> Renderable for Page<R, TX> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            html {
                head {
                    title { "Abacus" }
                    script src="https://unpkg.com/htmx.org@2.0.2"
                        integrity="sha384-Y7hw+L/jvKeWIRRkqWYfPcvVxHzVzn5REgzbawhxAuQGwX1XWe70vji+VSeHOThJ"
                        crossorigin="anonymous" {}
                    link
                        href="https://cdn.jsdelivr.net/npm/bootstrap@5.3.3/dist/css/bootstrap.min.css"
                        rel="stylesheet"
                        integrity="sha384-QWTKZyjpPEjISv5WaRU9OFeRpok6YctnYmDr5pNlyT2bRjXh0JMhjY6hW+ALEwIH"
                        crossorigin="anonymous";
                    meta
                        name="viewport"
                        content="width=device-width, initial-scale=1";
                }
                body hx-ext=(self.hx_ext) {
                    nav class="navbar navbar-expand"
                        style="background-color: #452859"
                        data-bs-theme="dark" {
                        div class="container-fluid" {
                            ul class="nav nav-justify-start" data-bs-theme="dark" {
                                li class="nav-item" {
                                    @if let Some(tournament) = &self.tournament {
                                        a class="nav-link text-white"
                                          href=(format!("/tournaments/{}", tournament.id)) {
                                            (tournament.abbrv)
                                        }
                                    } @else {
                                        a class="nav-link text-white" href="/" {
                                            "Home"
                                        }
                                    }
                                }
                            }
                            ul class="nav nav-justify-end" data-bs-theme="dark" {
                                @if let Some(user) = &self.user {
                                    li class="nav-item" {
                                        a class="nav-link text-white" href="/user" {
                                            (user.username)
                                        }
                                    }
                                }
                            }
                        }
                    }
                    @if let Some(body) = &self.body {
                        div class="container" {
                            div class="mt-4" {
                                (body)
                            }
                        }
                    }
                }
            }
        }.render_to(buffer)
    }
}

impl<R: Renderable, const TX: bool> Default for Page<R, TX> {
    fn default() -> Self {
        Self {
            body: Default::default(),
            user: Default::default(),
            hx_ext: Default::default(),
            tournament: Default::default(),
        }
    }
}
