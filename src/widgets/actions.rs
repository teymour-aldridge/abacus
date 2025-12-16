use hypertext::prelude::*;

pub struct Actions<'r> {
    pub options: &'r [(&'r str, &'r str)],
}

impl<'r> Renderable for Actions<'r> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud! {
            div class = "d-flex flex-wrap gap-3 mt-3 mb-3" {
                @for (link, text) in self.options {
                    a class="btn btn-outline-dark"
                        href=(link) {
                        (text)
                    }
                }
            }
        }
        .render_to(buffer);
    }
}
