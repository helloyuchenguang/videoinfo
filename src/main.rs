use anyhow::Result;
use axum::routing::get;
use axum::Router;
use dotenvy::var;
use tokio::net::TcpListener;
use tracing::info;
use videoinfo::{dao, handler, init};

#[tokio::main]
async fn main() -> Result<()> {
    init::log();
    // 连接数据库
    let pool = dao::connect_pool().await?;
    let app = Router::new()
        .route("/thumbnails", get(handler::get_thumbnails))
        .with_state(pool);
    let server = var("SERVER").unwrap_or("0.0.0.0:3000".to_string());
    let listener = TcpListener::bind(&server).await?;
    info!("服务启动在 http://{server}");
    axum::serve(listener, app).await?;
    Ok(())
}
