use crate::model::R;
use axum::response::{IntoResponse, Response};
use everything_sdk::EverythingError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IError {
    /// 数据库错误
    #[error("数据库(sqlx)错误")]
    DatabaseError(#[from] sqlx::Error),
    #[error("everything错误")]
    EsError(#[from] EverythingError),
}

impl IntoResponse for IError {
    fn into_response(self) -> Response {
        let body = format!("错误: {}", self);
        R::<String>::err(-1, body).into_response()
    }
}
