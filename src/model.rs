use crate::fhash;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, sqlx::FromRow)]
pub struct FileInfo {
    pub id: u32,
    pub hash_key: String,
    pub total_frame: u32,
    pub file_path: String,
    pub file_size: u32,
}

impl FileInfo {
    pub fn new(
        id: u32,
        hash_key: String,
        total_frame: u32,
        file_path: String,
        file_size: u32,
    ) -> Self {
        Self {
            id,
            hash_key,
            total_frame,
            file_path,
            file_size,
        }
    }

    pub fn obtain_filename(file_path: &str) -> String {
        // 获取文件名(去除后缀)
        let filename = std::path::Path::new(file_path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        // 去除后缀  a.a.a.mp4 -> a.a.a
        let filename = filename
            .split('.')
            .take(filename.split('.').count() - 1)
            .collect::<Vec<&str>>()
            .join(".");
        filename
    }

    pub fn from_path(path: &str) -> Self {
        let file_size = std::fs::metadata(path).map(|m| m.len() as u32).unwrap_or(0);
        Self {
            id: 0,
            hash_key: fhash::compute_sample_hash(path).unwrap_or_default(),
            total_frame: 0,
            file_path: path.to_string(),
            file_size,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CodeRequest {
    pub code: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct R<T> {
    pub code: i32,
    pub data: Option<T>,
    pub msg: Option<String>,
}
impl<T> R<T> {
    pub fn new(code: i32, data: Option<T>, msg: Option<String>) -> Self {
        Self { code, data, msg }
    }
    pub fn ok(data: T) -> Self {
        Self::new(0, Some(data), None)
    }

    pub fn err(code: impl Into<i32>, msg: impl Into<String>) -> Self {
        Self::new(code.into(), None, Some(msg.into()))
    }
}

impl<T: Serialize> IntoResponse for R<T> {
    fn into_response(self) -> Response {
        let body = Json(self);
        body.into_response()
    }
}
