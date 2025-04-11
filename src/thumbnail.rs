use crate::model::FileInfo;
use anyhow::Result;
use dotenvy::{dotenv, var};
use std::sync::LazyLock;
use tokio::process::Command;
use tracing::info;

pub static OUTPUT_DIR: LazyLock<String> = LazyLock::new(|| {
    dotenv().ok();
    var("OUTPUT_DIR").unwrap_or_else(|_| "D:/video-data".to_string())
});

/// 获取视频总帧数
pub async fn obtain_total_frame_count(file_path: &str) -> Result<u32> {
    // 获取输出目录
    let output_path = OUTPUT_DIR.as_str();
    let filename = &FileInfo::obtain_filename(&file_path);
    // ffprobe -v error -select_streams v:0 -show_entries stream=nb_frames -of default=noprint_wrappers=1:nokey=1 ${filePath}
    let mut cmd = Command::new("ffprobe");
    cmd.arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=nb_frames")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .arg(file_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    // 执行命令并等待输出
    let output = cmd.output().await?;
    // 检查命令是否成功执行
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr)?;
        return Err(anyhow::anyhow!("ffprobe获取视频总帧失败: {}", stderr));
    }
    // 解析标准输出
    let stdout = String::from_utf8(output.stdout)?;
    // 转换为 u32
    let num_frames = stdout.trim().parse::<u32>()?;
    let out_dir_path = gen_file_dir_path(output_path, filename);
    generate_keyframes(file_path, &out_dir_path).await?;
    generate_gif_by_keyframes(&out_dir_path).await?;
    Ok(num_frames)
}

pub fn gen_file_dir_path(output_path: &str, filename: &String) -> String {
    format!("{}/{}", output_path, filename)
}

/// 生成视频关键帧
pub async fn generate_keyframes(file_path: &str, out_dir_path: &str) -> Result<()> {
    // ffmpeg -hwaccel cuda -skip_frame nokey -i ${file_path}  -fps_mode vfr -vf select='not(mod(n\,10))',blackframe=0,metadata=select:key=lavfi.blackframe.pblack:value=80:function=less,scale=320:-1:force_original_aspect_ratio=decrease -q:v 1 -y {}/%04d.png
    let png_path = gen_out_png_path(out_dir_path);
    // 不存在则创建
    if !std::path::Path::new(&png_path).exists() {
        std::fs::create_dir_all(&png_path)?;
    }
    // 目录下png文件数大于20,则返回
    let png_files = std::fs::read_dir(&png_path)?;
    if png_files.count() > 20 {
        info!("png目录下已有文件,跳过生成");
        return Ok(());
    }
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-hwaccel")
        .arg("cuda")
        .arg("-skip_frame")
        .arg("nokey")
        .arg("-i")
        .arg(file_path)
        .arg("-fps_mode")
        .arg("vfr")
        .arg("-vf")
        // 取每10帧的关键帧
        .arg("select='not(mod(n\\,10))',blackframe=0,metadata=select:key=lavfi.blackframe.pblack:value=70:function=less,scale=320:-1:force_original_aspect_ratio=decrease")
        .arg("-q:v")
        .arg("1")
        .arg("-y")
        .arg(format!("{}/%04d.png", png_path));
    let output = cmd.output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr)?;
        return Err(anyhow::anyhow!("ffmpeg获取视频关键帧失败: {}", stderr));
    }
    Ok(())
}

pub fn gen_out_png_path(out_dir_path: &str) -> String {
    let png_path = format!("{}/png", out_dir_path);
    png_path
}

pub fn gen_out_gif_path(out_dir_path: &str) -> String {
    let gif_path = format!("{}/gif", out_dir_path);
    gif_path
}

/// 生成gif
pub async fn generate_gif_by_keyframes(output_dir_path: &str) -> Result<()> {
    // ffmpeg -i ${png_path}/%04d.png -vf scale=320:-1:flags=lanczos,fps=10 -c:v gif -loop 0 -y ${out_path}/gif/${filename}.gif
    let gif_path = gen_out_gif_path(output_dir_path);
    // 不存在则创建
    if !std::path::Path::new(&gif_path).exists() {
        info!("创建gif目录: {}", gif_path);
        std::fs::create_dir_all(&gif_path).expect("创建gif目录失败");
    }
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-i")
        .arg(format!("{}/png/%04d.png", output_dir_path))
        .arg("-vf")
        .arg("scale=320:-1:flags=lanczos,fps=3")
        .arg("-c:v")
        .arg("gif")
        .arg("-loop")
        .arg("0")
        .arg("-y")
        .arg(format!("{}/0.gif", gif_path));
    let output = cmd.output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr)?;
        return Err(anyhow::anyhow!("ffmpeg生成gif失败: {}", stderr));
    }
    Ok(())
}
