use hypertext::Rendered;
use rocket::{Responder, response::Redirect};

#[derive(Responder)]
pub enum GenerallyUsefulResponse {
    Success(Redirect),
    #[response(status = 400)]
    BadRequest(Rendered<String>),
    #[response(status = 404)]
    NotFound(()),
}
