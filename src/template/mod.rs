//! Templating code.
//!
//! This defines the [`Page`] item, which is used in most of the other parts of
//! this crate.

use hypertext::prelude::*;

use crate::{auth::User, tournaments::Tournament};

pub mod form;

pub struct Page<R1: Renderable, R2: Renderable, const TX: bool> {
    body: Option<R1>,
    user: Option<User<TX>>,
    extra_head: Option<R2>,
    tournament: Option<Tournament>,
}

impl<R1: Renderable, const TX: bool> Page<R1, String, TX> {
    pub fn new() -> Self {
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

    pub fn extra_head(mut self, content: R2) -> Self {
        self.extra_head = Some(content);
        self
    }

    pub fn user_opt(mut self, user: Option<User<TX>>) -> Self {
        self.user = user;
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
                body {
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
                    @if let Some(body) = &self.body {
                        (body)
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
        }
    }
}
