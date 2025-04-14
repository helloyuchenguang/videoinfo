use crate::model::{CodeRequest, FileInfo, R};
use crate::thumbnail::{self, gen_file_dir_path, OUTPUT_DIR};
use crate::{dao, es};
use async_walkdir::WalkDir;
use axum::extract::{Query, State};
use axum::response::{sse, IntoResponse, Sse};
use base64::engine::general_purpose;
use base64::Engine;
use futures::future::join_all;
use futures::Stream as FuturesStream;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;
use tokio_stream::StreamExt as TokioStreamExt;
use tracing::{error, info};

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
        let gif_path = thumbnail::gen_out_gif_path(&file_dir_path);
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
            encodeds.push(gen_imgbase64_by_path(path));
        }
    }
    encodeds
}

pub fn gen_imgbase64_by_path(path: PathBuf) -> String {
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
    format!("data:{};base64,{}", mime, encoded)
}

/// 新的SSE处理器，监听文件变化并添加超时
pub async fn sse_handler(
    // 接收查询参数
    Query(code_req): Query<CodeRequest>,
    // 数据库连接池
    State(pool): State<SqlitePool>,
) -> Sse<impl FuturesStream<Item = Result<sse::Event, std::convert::Infallible>>> {
    let code = code_req.code.clone();
    let file = es::search_files_by_keyword(code)
        .await
        .unwrap()
        .1
        .first()
        .unwrap()
        .clone();
    let out_dir = OUTPUT_DIR.as_str();
    // 输出目录
    let out_file_dir_path = gen_file_dir_path(out_dir, &FileInfo::obtain_filename(&file.filepath));
    // 输出gif路径
    let _gif_path = thumbnail::gen_out_gif_path(&out_file_dir_path);
    // 输出png路径
    let _png_path = thumbnail::gen_out_png_path(&out_file_dir_path);
    let file_info = dao::query_by_file_path(&pool, &file.filepath)
        .await
        .unwrap();
    let stream = match file_info {
        None => {
            tokio::spawn(async move {
                // 生成缩略图
                dao::create_file_info(&pool, &file.filepath).await.unwrap();
            });
            convert_pin_box_stream(gen_watch_dir_stream(out_file_dir_path))
        }
        Some(_) => convert_pin_box_stream(gen_read_current_file_stream(out_file_dir_path)),
    };
    Sse::new(stream)
}
fn convert_pin_box_stream<I>(
    stream: impl FuturesStream<Item = I> + Send + 'static,
) -> Pin<Box<dyn FuturesStream<Item = I> + Send>> {
    Box::pin(stream)
}

fn gen_read_current_file_stream(
    path: String,
) -> impl FuturesStream<Item = Result<sse::Event, std::convert::Infallible>> {
    info!("读取目录: {}", path);
    Box::pin(async_stream::stream! {
        let mut entries = WalkDir::new(path);
        loop {
            match entries.next().await {
                Some(Ok(entry)) => {
                    let ft = entry.file_type().await.unwrap();
                    if ft.is_file() {
                        yield Ok(sse::Event::default().data(gen_imgbase64_by_path(entry.path().to_path_buf())));
                    }
                },
                Some(Err(e)) => {
                    error!("读取文件夹失败: {}", e);
                    break;
                },
                None => break,
            }
        }
    })
}

fn gen_watch_dir_stream(
    path: String,
) -> impl futures::Stream<Item = Result<sse::Event, std::convert::Infallible>> {
    info!("监听目录: {}", path);
    // 不存在则创建
    if !std::path::Path::new(&path).exists() {
        std::fs::create_dir_all(&path).unwrap();
    }
    async_stream::stream! {
        // 创建监听器和接收通道
        let (mut watcher, mut rx) = async_watcher().expect("创建watcher失败");
        // 添加监听路径 NonRecursive: 只监听当前目录
        watcher.watch(path.as_ref(), RecursiveMode::Recursive).unwrap();
        // 循环处理事件（使用 tokio 的 next 方法）
        while let Some(Ok(res)) = rx.recv().await  {
            if let EventKind::Create(_) = res.kind {
                yield Ok(sse::Event::default().data(gen_imgbase64_by_path(res.paths[0].to_path_buf())));
            }
        }
        // 自动清理
        watcher.unwatch(path.as_ref()).unwrap();
    }
}

/// 监听者
fn async_watcher() -> notify::Result<(
    RecommendedWatcher,
    tokio::sync::mpsc::Receiver<notify::Result<notify::Event>>,
)> {
    let (tx, rx) = tokio::sync::mpsc::channel(10);
    // 监听器配置
    let config = Config::default()
        // 设置防抖时间
        .with_poll_interval(Duration::from_millis(50));
    let watcher = RecommendedWatcher::new(
        move |res| {
            futures::executor::block_on(async {
                tx.send(res).await.unwrap();
            })
        },
        config,
    )?;
    Ok((watcher, rx))
}
