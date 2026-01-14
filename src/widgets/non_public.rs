use hypertext::{Renderable, maud, prelude::*};

pub struct NonPublic<T: Renderable> {
    pub child: T,
    pub title: &'static str,
}

impl<T: Renderable> Renderable for NonPublic<T> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            div class="card bg-light-subtle mb-4" {
                div class="card-header" {
                    h5 class="card-title" {
                       (self.title)
                    }
                    p class="card-subtitle text-muted" {
                        "This is not yet public. It is only visible to you as a superuser."
                    }
                }
                div class="card-body" {
                    (self.child)
                }
            }
        }.render_to(buffer)
    }
}
