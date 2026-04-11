//! X Video Downloader GUI
//!
//! 使用 iced 框架的 GUI 界面

use crate::types::{format_bytes, sanitize_filename, VideoInfo};
use crate::yt_dlp::YtDlp;
use tracing::{info, warn};
use iced::widget::{
    button, column, container, scrollable, text, text_input, ProgressBar, Column, Row,
};
use iced::{
    alignment, executor, Application, Command, Element, Length, Settings, Theme,
};
use std::path::PathBuf;
use tokio::process::Command as TokioCommand;

/// 配置文件路径
fn config_path() -> PathBuf {
    directories::ProjectDirs::from("com", "x-video-downloader", "X Video Downloader")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
        .join("gui_config.json")
}

/// 加载保存的配置
fn load_config() -> (String, String) {
    let config_file = config_path();
    if config_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&config_file) {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                let cookie = config
                    .get("cookie_file")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                let output_dir = config
                    .get("output_dir")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                return (cookie, output_dir);
            }
        }
    }
    (String::new(), String::new())
}

/// 保存配置
fn save_config(cookie_file: &str, output_dir: &str) -> Result<(), String> {
    let config_file = config_path();
    if let Some(parent) = config_file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let config = serde_json::json!({
        "cookie_file": cookie_file,
        "output_dir": output_dir
    });
    std::fs::write(
        &config_file,
        serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 验证URL是否为有效的x.com/twitter URL
fn validate_url(url: &str) -> Result<String, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("URL cannot be empty".to_string());
    }
    if !url.contains("x.com") && !url.contains("twitter.com") {
        return Err("Please enter a valid X.com or Twitter.com video link".to_string());
    }
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }
    Ok(url.to_string())
}

/// 应用状态
pub struct App {
    url_input: String,
    output_dir: String,
    cookie_file: String,
    video_info: Option<VideoInfo>,
    selected_format: Option<usize>,
    is_fetching: bool,
    is_downloading: bool,
    download_progress: f32,
    status_message: String,
    error_message: Option<String>,
    ytdlp: YtDlp,
}

#[derive(Debug, Clone)]
pub enum Message {
    UrlChanged(String),
    OutputDirChanged(String),
    CookieChanged(String),
    FetchInfo,
    StartDownload,
    SelectFormat(usize),
    BrowseOutputDir,
    BrowseCookie,
    FolderSelected(Option<String>),
    CookieSelected(Option<String>),
    ClearError,
    SaveCookie,
    #[allow(dead_code)]
    ClearSavedCookie,
    InfoFetched(Result<VideoInfo, String>),
    DownloadCompleted(Result<String, String>),
}

impl Application for App {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        let (saved_cookie, saved_output) = load_config();
        let default_output = directories::UserDirs::new()
            .and_then(|dirs| dirs.video_dir().map(|p| p.to_string_lossy().to_string()))
            .unwrap_or_else(|| "./Downloads".to_string());

        let app = App {
            url_input: String::new(),
            output_dir: if saved_output.is_empty() {
                default_output
            } else {
                saved_output
            },
            cookie_file: saved_cookie,
            video_info: None,
            selected_format: None,
            is_fetching: false,
            is_downloading: false,
            download_progress: 0.0,
            status_message: "Enter video URL".to_string(),
            error_message: None,
            ytdlp: YtDlp::new(),
        };
        (app, Command::none())
    }

    fn title(&self) -> String {
        "X Video Downloader".to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::UrlChanged(url) => {
                self.url_input = url;
                Command::none()
            }
            Message::OutputDirChanged(dir) => {
                self.output_dir = dir;
                Command::none()
            }
            Message::CookieChanged(cookie) => {
                self.cookie_file = cookie;
                Command::none()
            }
            Message::BrowseOutputDir => {
                let current_dir = self.output_dir.clone();
                Command::perform(async move {
                    tokio::task::spawn_blocking(move || {
                        rfd::FileDialog::new()
                            .set_directory(&current_dir)
                            .pick_folder()
                            .map(|p| p.to_string_lossy().to_string())
                    })
                    .await
                    .ok()
                    .flatten()
                }, Message::FolderSelected)
            }
            Message::BrowseCookie => Command::perform(async move {
                tokio::task::spawn_blocking(|| {
                    rfd::FileDialog::new()
                        .add_filter("Cookie files", &["txt", "cookies", "json"])
                        .pick_file()
                        .map(|p| p.to_string_lossy().to_string())
                })
                .await
                .ok()
                .flatten()
            }, Message::CookieSelected),
            Message::CookieSelected(path) => {
                if let Some(p) = path {
                    self.cookie_file = p;
                }
                Command::none()
            }
            Message::FolderSelected(path) => {
                if let Some(p) = path {
                    self.output_dir = p;
                }
                Command::none()
            }
            Message::FetchInfo => match validate_url(&self.url_input) {
                Ok(_) => {
                    self.is_fetching = true;
                    self.status_message = "Fetching video info...".to_string();
                    self.error_message = None;
                    self.video_info = None;

                    let url = self.url_input.clone();
                    let cookie_file = self.cookie_file.clone();
                    let mut ytdlp = self.ytdlp.clone();

                    if !cookie_file.is_empty() {
                        if let Ok(path) = std::path::PathBuf::from(&cookie_file).canonicalize() {
                            ytdlp = ytdlp.with_cookie_file(path);
                        }
                    }

                    Command::perform(
                        async move { ytdlp.get_video_info(&url).map_err(|e| e.to_string()) },
                        Message::InfoFetched,
                    )
                }
                Err(e) => {
                    self.error_message = Some(e);
                    Command::none()
                }
            },
            Message::InfoFetched(result) => {
                self.is_fetching = false;
                match result {
                    Ok(info) => {
                        self.video_info = Some(info.clone());
                        self.selected_format =
                            if info.formats.is_empty() { None } else { Some(0) };
                        self.status_message = format!("Fetched: {}", info.title);
                    }
                    Err(e) => {
                        self.error_message = Some(e);
                        self.status_message = "Failed to fetch video info".to_string();
                    }
                }
                Command::none()
            }
            Message::SelectFormat(index) => {
                self.selected_format = Some(index);
                Command::none()
            }
            Message::StartDownload => {
                if validate_url(&self.url_input).is_err() {
                    self.error_message = Some("Please enter a valid URL".to_string());
                    return Command::none();
                }

                if let Some(ref info) = self.video_info {
                    self.is_downloading = true;
                    self.download_progress = 0.0;
                    self.status_message = "Connecting to server...".to_string();
                    self.error_message = None;

                    let url = self.url_input.clone();
                    let format_idx = self.selected_format.unwrap_or(0);
                    let is_direct_url = url.contains("video.twimg.com");

                    let format = if format_idx < info.formats.len() {
                        info.formats[format_idx].format_id.clone()
                    } else {
                        "best".to_string()
                    };
                    let output_dir = self.output_dir.clone();
                    let title = info.title.clone();

                    let final_format =
                        if format.contains("+") || format == "best" || is_direct_url {
                            if is_direct_url {
                                "best".to_string()
                            } else {
                                format
                            }
                        } else {
                            format!("{}+bestaudio", format)
                        };

                    Command::perform(
                        async move {
                            let output_path = std::path::Path::new(&output_dir);
                            if !output_path.exists() {
                                if let Err(e) = std::fs::create_dir_all(output_path) {
                                    tracing::warn!("创建输出目录失败: {}", e);
                                }
                            }

                            let filename = sanitize_filename(&title);
                            let output_file = output_path.join(format!("{}.%(ext)s", filename));

                            let ytdlp = YtDlp::new();
                            let args = ytdlp.build_download_args(
                                &url,
                                &output_file.to_string_lossy(),
                                Some(&final_format),
                                false,
                                false,
                            );

                            let output = TokioCommand::new(&ytdlp.executable)
                                .args(&args)
                                .output()
                                .await
                                .map_err(|e| format!("Failed to run yt-dlp: {}", e))?;

                            if output.status.success() {
                                Ok::<_, String>(format!("Download completed: {}", title))
                            } else {
                                let stderr = String::from_utf8_lossy(&output.stderr);
                                let stdout = String::from_utf8_lossy(&output.stdout);
                                let error_msg = if !stderr.is_empty() {
                                    stderr.to_string()
                                } else if !stdout.is_empty() {
                                    stdout.to_string()
                                } else {
                                    "Unknown error".to_string()
                                };
                                Err(error_msg)
                            }
                        },
                        Message::DownloadCompleted,
                    )
                } else {
                    self.error_message = Some("Please get video info first".to_string());
                    Command::none()
                }
            }
            Message::DownloadCompleted(result) => {
                self.is_downloading = false;
                match result {
                    Ok(msg) => {
                        self.status_message = msg;
                        self.download_progress = 1.0;
                    }
                    Err(e) => {
                        self.error_message = Some(e);
                        self.status_message = "Download failed".to_string();
                    }
                }
                Command::none()
            }
            Message::ClearError => {
                self.error_message = None;
                Command::none()
            }
            Message::SaveCookie => {
                info!(
                    "保存GUI配置: cookie={}, output={}",
                    self.cookie_file, self.output_dir
                );
                match save_config(&self.cookie_file, &self.output_dir) {
                    Ok(_) => self.status_message = "Cookie saved successfully!".to_string(),
                    Err(e) => {
                        warn!("保存配置失败: {}", e);
                        self.error_message = Some(format!("Failed to save: {}", e));
                    }
                }
                Command::none()
            }
            Message::ClearSavedCookie => {
                info!("清除保存的Cookie配置");
                self.cookie_file = String::new();
                match save_config("", &self.output_dir) {
                    Ok(_) => self.status_message = "Cookie cleared!".to_string(),
                    Err(e) => {
                        warn!("清除配置失败: {}", e);
                        self.error_message = Some(format!("Failed to clear: {}", e));
                    }
                }
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        // 简洁现代的设计
        let title = text("X Video Downloader")
            .size(22)
            .horizontal_alignment(alignment::Horizontal::Center);

        // 主容器
        let content = Column::new()
            .spacing(20)
            .padding(32)
            .push(title)
            // URL 输入
            .push(
                Column::new()
                    .spacing(8)
                    .push(text("URL").size(12))
                    .push(
                        Row::new()
                            .spacing(8)
                            .push(
                                text_input("Paste video link...", &self.url_input)
                                    .on_input(Message::UrlChanged)
                                    .on_submit(Message::FetchInfo)
                                    .width(Length::Fill)
                            )
                            .push(
                                button(
                                    text(if self.is_fetching { "..." } else { "Info" })
                                        .size(13)
                                )
                                .on_press(Message::FetchInfo)
                                .width(Length::Fixed(60.0))
                            )
                    )
            )
            // 输出目录
            .push(
                Column::new()
                    .spacing(8)
                    .push(text("Save to").size(12))
                    .push(
                        Row::new()
                            .spacing(8)
                            .push(
                                text_input("~/Videos", &self.output_dir)
                                    .on_input(Message::OutputDirChanged)
                                    .width(Length::Fill)
                            )
                            .push(
                                button(text("...").size(13))
                                    .on_press(Message::BrowseOutputDir)
                                    .width(Length::Fixed(40.0))
                            )
                    )
            )
            // Cookie
            .push(
                Column::new()
                    .spacing(8)
                    .push(text("Cookie (optional)").size(12))
                    .push(
                        Row::new()
                            .spacing(8)
                            .push(
                                text_input("cookies.txt", &self.cookie_file)
                                    .on_input(Message::CookieChanged)
                                    .width(Length::Fill)
                            )
                            .push(
                                button(text("...").size(13))
                                    .on_press(Message::BrowseCookie)
                                    .width(Length::Fixed(40.0))
                            )
                            .push(
                                button(text("Save").size(11))
                                    .on_press(Message::SaveCookie)
                                    .width(Length::Fixed(45.0))
                            )
                    )
            );

        let mut main = Column::new()
            .spacing(16)
            .push(content);

        // 视频信息
        if let Some(ref info) = self.video_info {
            let duration = info.duration.unwrap_or(0);
            let uploader = info.uploader.as_deref().unwrap_or("");

            main = main.push(
                Column::new()
                    .spacing(6)
                    .padding(12)
                    .width(Length::Fill)
                    .push(text(&info.title).size(14))
                    .push(
                        text(format!("{} - {} @{}", duration / 60, duration % 60, uploader))
                            .size(11)
                    )
            );

            // 格式列表
            let mut formats: Vec<_> = info.formats.iter()
                .filter(|f| !f.audio_only && f.ext != "unknown")
                .take(8)
                .cloned()
                .collect();
            formats.sort_by(|a, b| {
                let size_a = a.filesize.unwrap_or(0);
                let size_b = b.filesize.unwrap_or(0);
                size_b.cmp(&size_a)
            });

            let format_buttons: Vec<Element<'_, Message>> = formats.iter().enumerate().map(|(i, fmt)| {
                let res = fmt.resolution.clone().unwrap_or_else(|| "N/A".to_string());
                let size = fmt.filesize.map(format_bytes).unwrap_or_else(|| "-".to_string());
                let label = format!("{}  {}", res, size);
                button(text(label.as_str()).size(12))
                    .on_press(Message::SelectFormat(i))
                    .width(Length::Fill)
                    .into()
            }).collect();

            main = main.push(
                column![text("Quality").size(11)]
                    .push(scrollable(column![].spacing(4).extend(format_buttons)).height(Length::Fixed(140.0)))
            );

            // 下载按钮和状态
            let status_text = if self.is_downloading {
                format!("{}%", (self.download_progress * 100.0) as i32)
            } else {
                self.status_message.clone()
            };

            main = main.push(
                Row::new()
                    .spacing(12)
                    .push(
                        button(
                            text(if self.is_downloading { "Downloading..." } else { "Download" })
                                .size(14)
                        )
                        .on_press(Message::StartDownload)
                    )
                    .push(text(status_text.as_str()).size(12))
            );

            // 进度条
            if self.is_downloading {
                main = main.push(
                    ProgressBar::new(0.0f32..=1.0, self.download_progress)
                        .height(Length::Fixed(6.0))
                        .width(Length::Fill)
                );
            }
        } else {
            main = main.push(text(&self.status_message).size(12));
        }

        // 错误提示
        if let Some(ref err) = self.error_message {
            main = main.push(
                button(text("X").size(11))
                    .on_press(Message::ClearError)
            );
            main = main.push(text(err.as_str()).size(11));
        }

        container(main)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x()
            .center_y()
            .into()
    }
}

pub fn run_gui() {
    if !YtDlp::new().is_available() {
        eprintln!("Error: yt-dlp not found. Install with: brew install yt-dlp");
        return;
    }
    let _ = App::run(Settings::default());
}
