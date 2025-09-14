use hypertext::Rendered;
use rocket::{Responder, response::Redirect};

#[derive(Responder)]
pub enum GenerallyUsefulResponse {
    Success(Rendered<String>),
    SeeOther(Redirect),
    #[response(status = 400)]
    BadRequest(Rendered<String>),
    #[response(status = 404)]
    NotFound(()),
    #[response(status = 403)]
    Unauthorized(()),
}
