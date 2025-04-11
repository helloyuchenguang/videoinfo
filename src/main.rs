use anyhow::Result;
use axum::Router;
use axum::routing::get;
use dotenvy::var;
use tokio::net::TcpListener;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer, ExposeHeaders};
use tracing::info;
use videoinfo::{dao, handler, init};

#[tokio::main]
async fn main() -> Result<()> {
    init::log();
    // 连接数据库
    let pool = dao::connect_pool().await?;
    let app = Router::new()
        .route("/thumbnails", get(handler::get_thumbnails))
        .route("/sse", get(handler::sse_handler))
        .with_state(pool)
        // 配置CORS
        .layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::any()) // 允许所有来源
                .allow_methods(AllowMethods::any()) // 允许所有HTTP方法
                .allow_headers(AllowHeaders::any()) // 允许所有请求头
                .expose_headers(ExposeHeaders::any()), // 允许所有响应头
        );
    let server = var("SERVER").unwrap_or("0.0.0.0:3000".to_string());
    let listener = TcpListener::bind(&server).await?;
    info!("服务启动在 http://{server}");
    axum::serve(listener, app).await?;
    Ok(())
}
