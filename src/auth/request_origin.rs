use axum::{
    extract::Request,
    http::{Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

pub async fn check_request_origin(request: Request, next: Next) -> Response {
    let headers = request.headers();
    let websocket = headers.get(header::UPGRADE).is_some_and(|value| {
        value.as_bytes().eq_ignore_ascii_case(b"websocket")
    });
    let writes = !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if writes || websocket {
        let site = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok());
        let origin = headers
            .get(header::ORIGIN)
            .or_else(|| headers.get(header::REFERER));
        let valid = match origin {
            Some(value) => value
                .to_str()
                .ok()
                .is_some_and(|value| same_origin(value, &request)),
            None => !matches!(site, Some("cross-site" | "same-site")),
        };
        if !valid || site == Some("cross-site") {
            return (
                StatusCode::FORBIDDEN,
                "Cross-origin requests are not allowed.",
            )
                .into_response();
        }
    }
    next.run(request).await
}

fn same_origin(source: &str, request: &Request) -> bool {
    let headers = request.headers();
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get(header::HOST))
        .and_then(|value| value.to_str().ok())
        .or_else(|| request.uri().authority().map(|value| value.as_str()));
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .or_else(|| request.uri().scheme_str())
        .unwrap_or("http");
    let Some(host) = host else {
        return false;
    };
    let Ok(target) = url::Url::parse(&format!("{scheme}://{host}")) else {
        return false;
    };
    let Ok(source) = url::Url::parse(source) else {
        return false;
    };
    matches!(source.scheme(), "http" | "https")
        && matches!(target.scheme(), "http" | "https")
        && source.username().is_empty()
        && source.password().is_none()
        && target.username().is_empty()
        && target.password().is_none()
        && target.path() == "/"
        && target.query().is_none()
        && target.fragment().is_none()
        && source.origin() == target.origin()
}
