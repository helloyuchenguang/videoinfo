use crate::dao::query_by_file_path;
use crate::model::{CodeRequest, FileInfo, R};
use crate::thumbnail::{self, OUTPUT_DIR, gen_file_dir_path};
use crate::{dao, es};
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Sse, sse};
use base64::Engine;
use base64::engine::general_purpose;
use futures::Stream as FuturesStream;
use futures::StreamExt as FuturesStreamExt;
use futures::future::join_all;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;
use tokio_stream::StreamExt as TokioStreamExt;
use tokio_stream::wrappers::ReceiverStream;
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
    let gif_path = thumbnail::gen_out_gif_path(&out_file_dir_path);
    // 输出png路径
    let png_path = thumbnail::gen_out_png_path(&out_file_dir_path);
    let file_info = query_by_file_path(&pool, &file.filepath).await.unwrap();
    let stream = match file_info {
        None => {
            let out_file_dir_path_clone = out_file_dir_path.clone();
            tokio::spawn(async move {
                // 生成缩略图
                thumbnail::generate_keyframes(&file.filepath, &out_file_dir_path).await;
            });
            Box::pin(gen_watch_dir_stream(out_file_dir_path_clone))
                as Pin<
                    Box<
                        dyn futures::stream::Stream<
                                Item = Result<axum::response::sse::Event, std::convert::Infallible>,
                            > + Send
                            + Sync,
                    >,
                >
        }
        Some(_) => Box::pin(gen_read_current_file_stream())
            as Pin<
                Box<
                    dyn futures::stream::Stream<
                            Item = Result<axum::response::sse::Event, std::convert::Infallible>,
                        > + Send
                        + Sync,
                >,
            >,
    };
    Sse::new(stream)
}

fn gen_read_current_file_stream()
-> impl FuturesStream<Item = Result<sse::Event, std::convert::Infallible>> {
    async_stream::stream! {
        loop {
            yield Ok(sse::Event::default().data("hi!"));
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
}

fn gen_watch_dir_stream(
    path: String,
) -> impl futures::Stream<Item = Result<sse::Event, std::convert::Infallible>> {
    // 创建监听器和接收通道
    let (mut watcher, rx) = async_watcher().expect("创建watcher失败");
    info!("监听目录: {}", path);
    watcher
        .watch(path.as_ref(), RecursiveMode::NonRecursive)
        .unwrap();
    // 转换为接收流（此时 rx 的所有权已转移）
    let event_stream = ReceiverStream::new(rx);
    // 创建超时流（10秒无事件关闭）
    let timeout = tokio::time::sleep(Duration::from_secs(10));
    // 合并流（使用 tokio_stream 的 take_until）
    let combined_stream = event_stream.take_until(timeout);
    let mut pinned_stream = Box::pin(combined_stream);
    let stream = async_stream::stream! {
        // 循环处理事件（使用 futures 的 next 方法）
        while let Some(Ok(res)) = TokioStreamExt::next(&mut pinned_stream.as_mut()).await  {
            if let EventKind::Create(_)=res.kind {
                yield Ok(sse::Event::default().data(format!("文件创建: {:?}", res.paths)));
            }
        }

        // 自动清理
        watcher.unwatch(path.as_ref()).unwrap();
        yield Ok(sse::Event::default().data("连接超时关闭"));
    };
    stream
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
        .with_poll_interval(Duration::from_millis(100));
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
