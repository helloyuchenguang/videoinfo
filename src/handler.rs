use crate::model::{CodeRequest, FileInfo, R};
use crate::thumbnail::{self, OUTPUT_DIR, gen_file_dir_path};
use crate::{dao, es};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use base64::Engine;
use base64::engine::general_purpose;
use futures::future::join_all;
use sqlx::SqlitePool;
use tracing::info;

/// 获取视频缩略图
pub async fn get_thumbnails(
    // 接收查询参数
    Query(code_req): Query<CodeRequest>,
    // 数据库连接池
    State(pool): State<SqlitePool>,
) -> impl IntoResponse {
    let code = code_req.code.clone();
    let files = es::search_files_by_keyword(code).await.unwrap();
    let tasks = files.1.iter().map(async |file| {
        let start = std::time::Instant::now();
        let info = dao::query_and_update_by_file_path(&pool, &file.filepath)
            .await
            .unwrap();
        let out_dir = OUTPUT_DIR.as_str();
        let file_dir_path = gen_file_dir_path(out_dir, &FileInfo::obtain_filename(&info.file_path));
        let gif_path = thumbnail::gen_gif_path(&file_dir_path);
        let encodeds = get_files_to_base64_by_dir(&gif_path);
        info!("文件耗时: {:?}", start.elapsed());
        encodeds
    });
    // 并发执行所有任务,并且拍平收集结果Vec<String>
    let res = join_all(tasks)
        .await
        .iter()
        .flat_map(|l| l.clone())
        .collect::<Vec<_>>();
    R::ok(res)
}

// 指定目录下所有文件(1级目录),并转为base64
fn get_files_to_base64_by_dir(dir: &str) -> Vec<String> {
    let mut encodeds = vec![];
    info!("读取目录: {}", dir);
    let paths = std::fs::read_dir(dir).unwrap();
    for path in paths {
        let path = path.unwrap().path();
        if path.is_file() {
            let mime = match path.extension() {
                Some(ext) => match ext.to_str().unwrap().to_lowercase().as_str() {
                    "png" => "image/png",
                    "jpg" | "jpeg" => "image/jpeg",
                    "gif" => "image/gif",
                    "webp" => "image/webp",
                    "svg" => "image/svg+xml",
                    _ => "application/octet-stream",
                },
                None => "application/octet-stream",
            };

            let file = std::fs::read(path).unwrap();
            let encoded = general_purpose::STANDARD.encode(&file);
            encodeds.push(format!("data:{};base64,{}", mime, encoded));
        }
    }
    encodeds
}
