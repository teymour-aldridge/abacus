use hypertext::Rendered;
use rocket::{Responder, response::Redirect};

pub fn see_other_ok(r: Redirect) -> StandardResponse {
    Ok(SuccessResponse::SeeOther(Box::new(r)))
}

pub fn err_not_found() -> StandardResponse {
    Err(FailureResponse::NotFound(()))
}

pub fn bad_request(html: Rendered<String>) -> StandardResponse {
    Err(FailureResponse::BadRequest(html))
}

pub fn success(html: Rendered<String>) -> StandardResponse {
    Ok(SuccessResponse::Success(html))
}

pub fn unauthorized() -> StandardResponse {
    Err(FailureResponse::Unauthorized(()))
}

pub type StandardResponse = Result<SuccessResponse, FailureResponse>;

#[derive(Responder)]
pub enum SuccessResponse {
    Success(Rendered<String>),
    SeeOther(Box<Redirect>),
}

#[derive(Responder, Debug)]
pub enum FailureResponse {
    #[response(status = 400)]
    BadRequest(Rendered<String>),
    #[response(status = 404)]
    NotFound(()),
    #[response(status = 403)]
    Unauthorized(()),
    #[response(status = 500)]
    ServerError(()),
}
