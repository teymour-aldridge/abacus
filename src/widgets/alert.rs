use hypertext::prelude::*;

pub struct ErrorAlert<S> {
    pub msg: S,
}

impl<S: ToString> Renderable for ErrorAlert<S> {
    fn render_to(
        &self,
        buffer: &mut hypertext::Buffer<hypertext::context::Node>,
    ) {
        maud!({
            div class="container py-5" style="max-width: 800px;" {
                header class="mb-5" {
                    h1 class="display-4 fw-bold mb-3" {
                        "Error"
                    }
                }

                div class="bg-light" style="border-left: 2px solid var(--bs-danger); padding: 1.5rem; margin-bottom: 2rem;" {
                    p style="font-size: 0.875rem; line-height: 1.6; \
        color: var(--bs-gray-900); margin: 0;" {
                        (self.msg.to_string())
                    }
                }
            }
        })
        .render_to(buffer);
    }
}
