use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use hypertext::Rendered;

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

// StandardResponse is Result, which implements IntoResponse if T and E do.

#[derive(Debug)]
pub enum SuccessResponse {
    Success(Rendered<String>),
    SeeOther(Box<Redirect>),
}

impl IntoResponse for SuccessResponse {
    fn into_response(self) -> Response {
        match self {
            SuccessResponse::Success(html) => {
                Html(html.into_inner()).into_response()
            }
            SuccessResponse::SeeOther(redirect) => redirect.into_response(),
        }
    }
}

#[derive(Debug)]
pub enum FailureResponse {
    BadRequest(Rendered<String>),
    NotFound(()),
    Unauthorized(()),
    ServerError(()),
}

impl IntoResponse for FailureResponse {
    fn into_response(self) -> Response {
        match self {
            FailureResponse::BadRequest(html) => {
                (StatusCode::BAD_REQUEST, Html(html.into_inner()))
                    .into_response()
            }
            FailureResponse::NotFound(_) => {
                (StatusCode::NOT_FOUND, "Not Found").into_response()
            }
            FailureResponse::Unauthorized(_) => {
                (StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
            }
            FailureResponse::ServerError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Server Error")
                    .into_response()
            }
        }
    }
}

impl From<diesel::result::Error> for FailureResponse {
    fn from(err: diesel::result::Error) -> Self {
        match err {
            diesel::result::Error::NotFound => FailureResponse::NotFound(()),
            _ => {
                tracing::error!("Database error: {:?}", err);
                FailureResponse::ServerError(())
            }
        }
    }
}
