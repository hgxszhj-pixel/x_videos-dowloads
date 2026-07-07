//! 主题配置

/// 主题配置
#[derive(Default, Clone, PartialEq)]
pub(super) enum AppTheme {
    #[default]
    Purple,      // 紫色渐变
    Ocean,       // 海洋蓝
    Sunset,      // 日落橙
    Forest,      // 森林绿
}

impl AppTheme {
    pub(super) fn gradient(&self) -> &'static str {
        match self {
            AppTheme::Purple => "linear-gradient(135deg, #667eea 0%, #764ba2 100%)",
            AppTheme::Ocean => "linear-gradient(135deg, #0066ff 0%, #00d4ff 100%)",
            AppTheme::Sunset => "linear-gradient(135deg, #ff6b6b 0%, #ffa502 100%)",
            AppTheme::Forest => "linear-gradient(135deg, #11998e 0%, #38ef7d 100%)",
        }
    }
    /// 主背景色
    pub(super) fn background_color(&self) -> &'static str {
        match self {
            AppTheme::Purple => "#1a1a2e",
            AppTheme::Ocean => "#0a192f",
            AppTheme::Sunset => "#2d1f1f",
            AppTheme::Forest => "#0d261f",
        }
    }
    /// 卡片/容器背景色
    pub(super) fn card_background(&self) -> &'static str {
        match self {
            AppTheme::Purple => "rgba(102, 126, 234, 0.15)",
            AppTheme::Ocean => "rgba(0, 102, 255, 0.15)",
            AppTheme::Sunset => "rgba(255, 107, 107, 0.15)",
            AppTheme::Forest => "rgba(17, 153, 142, 0.15)",
        }
    }
    /// 文字颜色
    pub(super) fn text_color(&self) -> &'static str {
        match self {
            AppTheme::Purple => "#e8e8e8",
            AppTheme::Ocean => "#e0e0e0",
            AppTheme::Sunset => "#f0e0e0",
            AppTheme::Forest => "#e0f0ed",
        }
    }
}
