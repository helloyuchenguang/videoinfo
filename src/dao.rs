use crate::{fhash, model, thumbnail};

use anyhow::Result;
use dotenvy::var;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use tracing::info;

pub async fn connect_pool() -> Result<SqlitePool> {
    // 判断文件是否存在
    let db_path = var("DATABASE_PATH").unwrap_or("data.sqlite3".to_string());
    if !std::path::Path::new(&db_path).exists() {
        std::fs::File::create(&db_path)?;
    }
    // 连接数据库
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&format!("sqlite:{}", db_path))
        .await
        .expect("连接数据库失败");
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS file_info (
                id INTEGER PRIMARY KEY,
                hash_key TEXT NOT NULL,
                total_frame INTEGER NOT NULL,
                file_path TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                UNIQUE(hash_key)
            )"#,
    )
    .execute(&pool)
    .await?;
    Ok(pool)
}

/// 根据文件hash值查询文件信息
pub async fn query_by_hash_key(
    pool: &SqlitePool,
    hash_key: impl Into<String>,
) -> Result<Option<model::FileInfo>> {
    let file_info = sqlx::query_as::<_, model::FileInfo>(
        "SELECT id, hash_key, total_frame, file_path, file_size FROM file_info WHERE hash_key = ?",
    )
    .bind(hash_key.into())
    .fetch_optional(pool)
    .await?;
    Ok(file_info)
}

pub async fn query_by_file_path(
    pool: &SqlitePool,
    file_path: &str,
) -> Result<Option<model::FileInfo>> {
    // 计算文件hash值dd
    let hash_key = fhash::compute_sample_hash(file_path)?;
    info!("文件路径: {}, hash_key: {}", file_path, hash_key);
    Ok(query_by_hash_key(pool, hash_key).await?)
}

/// 查询文件信息，如果不存在则插入新记录
pub async fn query_and_update_by_file_path(
    pool: &SqlitePool,
    file_path: &str,
) -> Result<model::FileInfo> {
    let file_info = query_by_file_path(pool, file_path).await?;
    if let Some(fi) = file_info {
        return Ok(fi);
    }
    // 如果没有记录，则插入新记录
    let new_file_info = model::FileInfo::new(
        0,
        fhash::compute_sample_hash(file_path)?,
        // 获取视频总帧数
        thumbnail::obtain_total_frame_count(file_path).await?,
        file_path.to_string(),
        std::fs::metadata(file_path)?.len() as u32,
    );
    sqlx::query(
        "INSERT INTO file_info (hash_key, total_frame, file_path,filename, file_size) VALUES (?, ?, ?, ?)",
    )
    .bind(&new_file_info.hash_key)
    .bind(new_file_info.total_frame)
    .bind(&new_file_info.file_path)
    .bind(new_file_info.file_size)
    .execute(pool)
    .await?;
    Ok(new_file_info)
}
