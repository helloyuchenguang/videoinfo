use anyhow::Result;
use everything_sdk::{EverythingError, EverythingItem, RequestFlags, SortType};
use serde::{Deserialize, Serialize};
use tracing::info;

/// 调用everything_sdk查询文件
pub async fn search_files_by_keyword(keyword: String) -> Result<(String, Vec<SdkFileItem>)> {
    let start = std::time::Instant::now();
    //这里我们使用异步版本[`futures:：Mutex`]，所以等待它。
    let mut everything = everything_sdk::global().lock().await;
    let data = match everything.is_db_loaded() {
        Ok(false) => panic!("Everything数据库现在尚未完全加载。"),
        Err(EverythingError::Ipc) => panic!("everything需要在后台运行。"),
        _ => {
            // 创建一个搜索器
            let mut searcher = everything.searcher();
            let keyword = format!(
                "size:>128MB .mp4|.avi|.wmv|.mkv|.mpg|.rmvb|.iso|.bt.xltd {}",
                keyword
            );
            // 设置搜索关键字
            searcher.set_search(&keyword);
            // 设置搜索类型
            searcher
                .set_request_flags(
                    RequestFlags::EVERYTHING_REQUEST_FILE_NAME
                        | RequestFlags::EVERYTHING_REQUEST_PATH
                        | RequestFlags::EVERYTHING_REQUEST_SIZE
                        | RequestFlags::EVERYTHING_REQUEST_ATTRIBUTES
                        | RequestFlags::EVERYTHING_REQUEST_DATE_CREATED
                        | RequestFlags::EVERYTHING_REQUEST_EXTENSION,
                )
                // 最大结果数
                .set_max(20)
                // 忽略大小写
                .set_match_case(false)
                // 默认：按照文件名排序
                .set_sort(SortType::EVERYTHING_SORT_NAME_ASCENDING);
            // 执行查询
            let results = searcher.query().await;
            let list = results
                .into_iter()
                .map(|ei| SdkFileItem::from(ei))
                .collect::<Vec<_>>();
            (keyword, list)
        }
    };
    info!(
        "es查询【{}】({})耗时: {:?}",
        data.0,
        data.1.len(),
        start.elapsed()
    );
    Ok(data)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SdkFileItem {
    pub index: u32,
    // 完全路径
    pub filepath: String,
    // 文件名
    pub filename: String,
    // 扩展名
    pub ext: String,
    // 文件路径
    pub path: String,
    // 文件大小
    pub size: u64,
    // 创建时间
    pub date_created: String,
    // 是否是目录
    pub is_dir: bool,
}

impl<'a> From<EverythingItem<'a>> for SdkFileItem {
    fn from(ei: EverythingItem<'a>) -> Self {
        let binding = ei.filepath().unwrap();
        let filepath = binding.to_str().unwrap();
        let filename = ei.filename().unwrap().to_str().unwrap().to_string();
        let is_dir = ei.is_folder();
        SdkFileItem {
            index: ei.index(),
            filepath: filepath.into(),
            filename: filename.clone(),
            path: ei.path().unwrap().to_str().unwrap().into(),
            ext: ei.extension().unwrap().into_string().unwrap(),
            size: ei.size().unwrap(),
            date_created: ei.date_created().unwrap().to_string(),
            is_dir,
        }
    }
}
