use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Application error type that maps to HTTP responses.
#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Conflict(String),
    Unprocessable(String),
    ServiceUnavailable(String),
    Internal(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound(msg) => write!(f, "Not found: {msg}"),
            AppError::BadRequest(msg) => write!(f, "Bad request: {msg}"),
            AppError::Conflict(msg) => write!(f, "Conflict: {msg}"),
            AppError::Unprocessable(msg) => write!(f, "Unprocessable: {msg}"),
            AppError::ServiceUnavailable(msg) => write!(f, "Service unavailable: {msg}"),
            AppError::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            AppError::Unprocessable(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg),
            AppError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        let body = serde_json::json!({ "error": message });
        (status, axum::Json(body)).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => AppError::NotFound("resource not found".into()),
            other => AppError::Internal(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_formats_each_error_variant() {
        let cases = [
            (AppError::NotFound("missing".into()), "Not found: missing"),
            (AppError::BadRequest("bad".into()), "Bad request: bad"),
            (AppError::Conflict("conflict".into()), "Conflict: conflict"),
            (
                AppError::Unprocessable("unprocessable".into()),
                "Unprocessable: unprocessable",
            ),
            (
                AppError::ServiceUnavailable("offline".into()),
                "Service unavailable: offline",
            ),
            (AppError::Internal("boom".into()), "Internal error: boom"),
        ];

        for (error, expected) in cases {
            assert_eq!(error.to_string(), expected);
        }
    }

    #[test]
    fn into_response_maps_status_codes() {
        let cases = [
            (AppError::NotFound("missing".into()), StatusCode::NOT_FOUND),
            (AppError::BadRequest("bad".into()), StatusCode::BAD_REQUEST),
            (AppError::Conflict("conflict".into()), StatusCode::CONFLICT),
            (
                AppError::Unprocessable("unprocessable".into()),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                AppError::ServiceUnavailable("offline".into()),
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (
                AppError::Internal("boom".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];

        for (error, expected_status) in cases {
            assert_eq!(error.into_response().status(), expected_status);
        }
    }

    #[test]
    fn sqlx_error_conversion_preserves_row_not_found_special_case() {
        assert!(matches!(
            AppError::from(sqlx::Error::RowNotFound),
            AppError::NotFound(message) if message == "resource not found"
        ));
        assert!(matches!(
            AppError::from(sqlx::Error::ColumnNotFound("field".into())),
            AppError::Internal(message) if message.contains("field")
        ));
    }
}
