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
                    span class="badge text-bg-danger" { "Unpublished" }
                }
                div class="card-body" {
                    (self.child)
                }
            }
        }
        .render_to(buffer)
    }
}

impl<C: ?Sized, T: Renderable> super::View<C> for NonPublic<T> {
    fn render_to(&self, _: &C, output: &mut super::HtmlBuffer) {
        Renderable::render_to(self, output);
    }
}
