//! Templating code.
//!
//! This defines the [`Page`] item, which is used in most of the other parts of
//! this crate.

use hypertext::prelude::*;

use crate::{auth::User, tournaments::Tournament};

pub mod form;

pub struct Page<R: Renderable> {
    body: Option<R>,
    user: Option<User>,
}

impl<R: Renderable> Page<R> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn tournament(self, _tournament: Tournament) -> Self {
        // todo: implement
        self
    }

    pub fn body(mut self, body: R) -> Self {
        self.body = Some(body);
        self
    }

    pub fn user(mut self, user: User) -> Self {
        self.user = Some(user);
        self
    }

    pub fn user_opt(mut self, user: Option<User>) -> Self {
        self.user = user;
        self
    }
}

impl<R: Renderable> Renderable for Page<R> {
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
                body {
                    nav class="navbar navbar-expand"
                    style="background-color: #452859"
                    data-bs-theme="dark" {
                        div class="container-fluid" {
                            ul class="nav nav-justify-start"
                                data-bs-theme="dark" {
                                li class="nav-item" {
                                    a
                                        class="nav-link text-white"
                                        href="/" { "Home" }
                                }
                            }
                            ul
                                class="nav nav-justify-end"
                                data-bs-theme="dark" {
                                    @if let Some(user) = &self.user {
                                        a
                                            class="nav-link text-white"
                                            href="/user" { (user.username) }

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

impl<R: Renderable> Default for Page<R> {
    fn default() -> Self {
        Self {
            body: Default::default(),
            user: Default::default(),
        }
    }
}
