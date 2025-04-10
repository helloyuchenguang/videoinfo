use anyhow::Result;
use std::fs::File;
use std::io;
use std::io::{BufReader, Read, Seek, SeekFrom};
use xxhash_rust::xxh3::Xxh3;

/// 计算采样哈希
pub fn compute_sample_hash(path: &str) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let sample_size = 1024 * 1024; // 1MB
    let file_size = reader.get_ref().metadata()?.len();

    let mut hasher = Xxh3::new();
    let mut buffer = vec![0u8; sample_size];

    // 读取开头
    read_sample(&mut reader, 0, &mut buffer, &mut hasher)?;

    // 读取中间
    let mid_pos = file_size.saturating_sub(sample_size as u64) / 2;
    read_sample(&mut reader, mid_pos, &mut buffer, &mut hasher)?;

    // 读取末尾
    let end_pos = file_size.saturating_sub(sample_size as u64);
    if end_pos != mid_pos {
        // 避免重复读取
        read_sample(&mut reader, end_pos, &mut buffer, &mut hasher)?;
    }

    // 生成128位哈希
    Ok(hasher.digest128().to_string())
}

// 从指定位置读取样本
fn read_sample<R: Read + Seek>(
    reader: &mut R,
    pos: u64,
    buffer: &mut [u8],
    hasher: &mut Xxh3,
) -> io::Result<()> {
    reader.seek(SeekFrom::Start(pos))?;
    let n = reader.read(buffer)?;
    hasher.update(&buffer[..n]);
    Ok(())
}
