#![allow(dead_code)]

//! 国际化 (i18n) 模块
//!
//! 提供多语言支持和字体配置

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use once_cell::sync::Lazy;

/// 当前语言配置
static CURRENT_LANG: Lazy<Mutex<LanguagePack>> = Lazy::new(|| {
    Mutex::new(LanguagePack::default())
});

/// 语言包结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguagePack {
    /// 语言代码 (如 "zh-CN", "en")
    pub lang: String,
    /// 显示名称
    pub display_name: String,
    /// 字体配置
    pub fonts: FontConfig,
    /// UI 文本翻译
    pub texts: HashMap<String, String>,
}

/// 字体配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontConfig {
    /// 首选字体
    pub primary: String,
    /// 备用字体列表
    pub fallback: Vec<String>,
    /// 是否启用字体回退
    pub enable_fallback: bool,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            primary: "System".to_string(),
            fallback: vec![],
            enable_fallback: true,
        }
    }
}

impl Default for LanguagePack {
    fn default() -> Self {
        // 默认返回英文语言包
        Self {
            lang: "en".to_string(),
            display_name: "English".to_string(),
            fonts: FontConfig::default(),
            texts: Self::default_english_texts(),
        }
    }
}

impl LanguagePack {
    /// 默认英文文本
    fn default_english_texts() -> HashMap<String, String> {
        let mut texts = HashMap::new();
        texts.insert("app.title".to_string(), "X Video Downloader".to_string());
        texts.insert("status.enter_url".to_string(), "Enter video URL".to_string());
        texts.insert("btn.fetch".to_string(), "Fetch Info".to_string());
        texts.insert("btn.download".to_string(), "Download".to_string());
        texts.insert("btn.save".to_string(), "Save".to_string());
        texts.insert("btn.clear".to_string(), "Clear".to_string());
        texts.insert("btn.cancel".to_string(), "Cancel".to_string());
        texts.insert("btn.browse".to_string(), "Browse".to_string());
        texts.insert("label.video_url".to_string(), "Video URL".to_string());
        texts.insert("label.output_dir".to_string(), "Output Directory".to_string());
        texts.insert("label.cookie_file".to_string(), "Cookie File (optional)".to_string());
        texts.insert("label.video_info".to_string(), "Video Information".to_string());
        texts.insert("label.settings".to_string(), "Settings".to_string());
        texts.insert("label.downloading".to_string(), "Downloading...".to_string());
        texts.insert("label.speed".to_string(), "Speed".to_string());
        texts.insert("label.eta".to_string(), "ETA".to_string());
        texts.insert("label.threads".to_string(), "Threads".to_string());
        texts
    }

    /// 默认简体中文文本
    fn default_chinese_texts() -> HashMap<String, String> {
        let mut texts = HashMap::new();
        texts.insert("app.title".to_string(), "X 视频下载器".to_string());
        texts.insert("status.enter_url".to_string(), "输入视频链接".to_string());
        texts.insert("btn.fetch".to_string(), "获取信息".to_string());
        texts.insert("btn.download".to_string(), "下载".to_string());
        texts.insert("btn.save".to_string(), "保存".to_string());
        texts.insert("btn.clear".to_string(), "清空".to_string());
        texts.insert("btn.cancel".to_string(), "取消".to_string());
        texts.insert("btn.browse".to_string(), "浏览".to_string());
        texts.insert("label.video_url".to_string(), "视频链接".to_string());
        texts.insert("label.output_dir".to_string(), "输出目录".to_string());
        texts.insert("label.cookie_file".to_string(), "Cookie 文件（可选）".to_string());
        texts.insert("label.video_info".to_string(), "视频信息".to_string());
        texts.insert("label.settings".to_string(), "设置".to_string());
        texts.insert("label.downloading".to_string(), "下载中...".to_string());
        texts.insert("label.speed".to_string(), "速度".to_string());
        texts.insert("label.eta".to_string(), "剩余时间".to_string());
        texts.insert("label.threads".to_string(), "线程".to_string());
        texts
    }

    /// 加载语言包
    #[allow(dead_code)]
    pub fn load_from_file(path: &PathBuf) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse JSON: {}", e))
    }

    /// 加载内置语言包
    pub fn load_builtin(lang: &str) -> Self {
        match lang {
            "zh-CN" | "zh" => Self {
                lang: "zh-CN".to_string(),
                display_name: "简体中文".to_string(),
                fonts: FontConfig {
                    primary: "PingFang SC".to_string(),
                    fallback: vec!["Microsoft YaHei".to_string(), "SimHei".to_string(), "Arial".to_string()],
                    enable_fallback: true,
                },
                texts: Self::default_chinese_texts(),
            },
            _ => Self::default(),
        }
    }
}

/// 获取翻译文本
#[allow(dead_code)]
pub fn tr(key: &str) -> String {
    let lang = CURRENT_LANG.lock().expect("i18n: CURRENT_LANG poisoned");
    lang.texts.get(key).cloned().unwrap_or_else(|| key.to_string())
}

/// 获取当前字体列表（用于 Slint）
pub fn get_font_family() -> String {
    let lang = CURRENT_LANG.lock().expect("i18n: CURRENT_LANG poisoned");
    let fonts = &lang.fonts;

    if fonts.enable_fallback {
        let all_fonts: Vec<&str> = std::iter::once(fonts.primary.as_str())
            .chain(fonts.fallback.iter().map(|s| s.as_str()))
            .collect();
        all_fonts.join(", ")
    } else {
        fonts.primary.clone()
    }
}

/// 切换语言
pub fn set_language(lang: &str) {
    let mut current = CURRENT_LANG.lock().expect("i18n: CURRENT_LANG poisoned");
    *current = LanguagePack::load_builtin(lang);
}

/// 获取当前语言代码
#[allow(dead_code)]
pub fn get_current_lang() -> String {
    let lang = CURRENT_LANG.lock().expect("i18n: CURRENT_LANG poisoned");
    lang.lang.clone()
}

/// 初始化 i18n（根据系统语言自动选择）
pub fn init() {
    // 简单检测：如果是中文环境，使用简体中文
    // 实际项目可使用系统 locale 检测
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            if output_str.contains("zh") {
                set_language("zh-CN");
                return;
            }
        }
    }
    // 默认英文
    set_language("en");
}

/// 获取所有可用语言
#[allow(dead_code)]
pub fn get_available_languages() -> Vec<(&'static str, &'static str)> {
    vec![
        ("en", "English"),
        ("zh-CN", "简体中文"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tr_english() {
        set_language("en");
        assert_eq!(tr("app.title"), "X Video Downloader");
    }

    #[test]
    fn test_tr_chinese() {
        set_language("zh-CN");
        assert_eq!(tr("app.title"), "X 视频下载器");
    }

    #[test]
    fn test_font_family_english() {
        set_language("en");
        let fonts = get_font_family();
        assert_eq!(fonts, "System");
    }

    #[test]
    fn test_font_family_chinese() {
        set_language("zh-CN");
        let fonts = get_font_family();
        assert!(fonts.contains("PingFang SC"));
    }
}