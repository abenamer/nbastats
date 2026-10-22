//! Map application errors to HTTP responses. Internal details stay in the logs.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::application::dynasty::DynastyError;
use crate::application::imports::ImportError;
use crate::application::jobs::JobError;
use crate::application::ports::StoreError;
use crate::application::state::SeasonLoadError;
use crate::domain::validation::ValidationError;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Unavailable(String),
    Internal(String),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        ApiError::BadRequest(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        ApiError::NotFound(msg.into())
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        ApiError::Conflict(msg.into())
    }
}

impl From<ValidationError> for ApiError {
    fn from(e: ValidationError) -> Self {
        ApiError::BadRequest(e.to_string())
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::Unavailable(m) => ApiError::Unavailable(m),
            StoreError::Query(m) | StoreError::InvalidData(m) => ApiError::Internal(m),
        }
    }
}

impl From<SeasonLoadError> for ApiError {
    fn from(e: SeasonLoadError) -> Self {
        match e {
            SeasonLoadError::NoGames(s) => ApiError::NotFound(format!("no data for season {s}")),
            SeasonLoadError::Store(s) => s.into(),
        }
    }
}

impl From<JobError> for ApiError {
    fn from(e: JobError) -> Self {
        match e {
            JobError::Invalid(v) => v.into(),
            JobError::Params(m) => ApiError::BadRequest(m),
            JobError::Store(s) => s.into(),
            JobError::Execution(m) => ApiError::Internal(m),
        }
    }
}

impl From<DynastyError> for ApiError {
    fn from(e: DynastyError) -> Self {
        match e {
            DynastyError::UnknownFranchise(f) => {
                ApiError::NotFound(format!("unknown franchise {f}"))
            }
            DynastyError::Store(s) => s.into(),
            DynastyError::Season(s) => s.into(),
        }
    }
}

impl From<ImportError> for ApiError {
    fn from(e: ImportError) -> Self {
        match e {
            ImportError::Parse(p) => ApiError::BadRequest(format!("import failed: {p}")),
            ImportError::Download(d) => ApiError::BadRequest(format!("refresh failed: {d}")),
            ImportError::Io(io) => ApiError::Internal(io.to_string()),
            ImportError::Store(s) => s.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, m),
            ApiError::Unavailable(m) => {
                tracing::error!(error = %m, "store unavailable");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "database unavailable".to_string(),
                )
            }
            ApiError::Internal(m) => {
                tracing::error!(error = %m, "request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

pub type ApiResult<T> = Result<Json<T>, ApiError>;
