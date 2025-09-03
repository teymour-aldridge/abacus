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
            div class="alert alert-danger" role="alert" {
                (self.msg.to_string())
            }
        })
        .render_to(buffer);
    }
}
