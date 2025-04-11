use tracing::info;
use tracing_subscriber::{
    fmt::{self, time::ChronoLocal},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

pub fn log() {
    // 加载环境变量
    dotenvy::dotenv().ok();
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_test_writer() // 测试时打印到控制台
                .with_line_number(true), // 打印日志行号
        )
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                let cargo_crate_name = env!("CARGO_CRATE_NAME");
                println!("cargo_crate_name: {}", cargo_crate_name);
                format!("{}=debug", cargo_crate_name).into()
            }),
        )
        .init();
    info!("初始化日志成功");
}
