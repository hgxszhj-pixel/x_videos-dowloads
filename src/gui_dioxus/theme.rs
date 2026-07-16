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

// ============================================================================
// Style tokens — 共享 CSS 片段（不依赖主题色），避免 inline style 字符串重复
// ============================================================================

/// 通用 button 共享属性
pub(super) const STYLE_BTN_BASE: &str = "color: white; border: none; cursor: pointer; transition: all 0.3s ease;";
/// 圆形 icon button 共享属性
pub(super) const STYLE_BTN_ICON: &str = "color: white; border: none; border-radius: 50%; cursor: pointer;";
/// 裸 input（无边框，嵌在父容器里）
pub(super) const STYLE_INPUT_BARE: &str = "flex: 1; border: none; outline: none;";
/// 进度条底槽（不含 height/radius，由调用方指定；多数场景 height=10px radius=5px）
pub(super) const STYLE_PROGRESS_TRACK: &str = "background: #e8e8e8; border-radius: 5px; overflow: hidden;";

// ---------------------------------------------------------------------------
// 中性色 token — 跨主题不变的灰阶/UI 色（不是风格 token；避免散落 hex）
// ---------------------------------------------------------------------------
/// 中等灰文字（次要 label/status）— 替代 `#888`
pub(super) const TEXT_MUTED: &str = "#888";
/// 主要文字（标题/数据）— 替代 `#333`
pub(super) const TEXT_PRIMARY: &str = "#333";
/// 次要文字（label）— 替代 `#555`
pub(super) const TEXT_SECONDARY: &str = "#555";
/// 浅灰背景（次级按钮/输入框底）— 替代 `#f0f0f0`
pub(super) const BG_SUBTLE: &str = "#f0f0f0";
/// 默认边框（输入框/分隔）— 替代 `#ddd`
pub(super) const BORDER_DEFAULT: &str = "#ddd";
/// 强对比危险红（独立于 `color_danger`，用于"取消下载 / 清空历史"等强 destructive 胶囊按钮）— 替代 `#ff4444`
pub(super) const ALERT_RED: &str = "#ff4444";

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

    // ------------------------------------------------------------------------
    // 语义色 — 跨主题一致的语义色（danger/success/primary），避免散落 hex
    // ------------------------------------------------------------------------
    pub(super) fn color_danger(&self) -> &'static str { "#ea4335" }
    pub(super) fn color_success(&self) -> &'static str { "#34a853" }
    pub(super) fn color_primary(&self) -> &'static str { "#4285f4" }
    pub(super) fn color_text_danger(&self) -> &'static str { "#721c24" }
    /// 主题强调色（用于 batch progress 文字等跟随主题变化的 highlight）
    pub(super) fn color_accent(&self) -> &'static str {
        match self {
            AppTheme::Purple => "#667eea",
            AppTheme::Ocean => "#0066ff",
            AppTheme::Sunset => "#ff6b6b",
            AppTheme::Forest => "#11998e",
        }
    }
    /// 主题色环边框色（对应 `gradient()` 起始 hex，用于 ThemeSwatch 边框）
    pub(super) fn border_color(&self) -> &'static str {
        match self {
            AppTheme::Purple => "#667eea",
            AppTheme::Ocean => "#0066ff",
            AppTheme::Sunset => "#ff6b6b",
            AppTheme::Forest => "#11998e",
        }
    }

    // ------------------------------------------------------------------------
    // Button style helpers — 返回 String 因为依赖 theme.gradient()
    // ------------------------------------------------------------------------
    /// 主题色填充的实心按钮（如"获取信息"主操作）
    pub(super) fn btn_primary(&self, padding: &str, radius: &str, font_size: &str) -> String {
        format!("{STYLE_BTN_BASE} background: {}; padding: {padding}; border-radius: {radius}; font-size: {font_size}; font-weight: 600;", self.gradient())
    }
    /// 成功色填充（绿色渐变，用于下载/确认）
    pub(super) fn btn_success(&self, padding: &str, radius: &str, font_size: &str) -> String {
        format!("{STYLE_BTN_BASE} background: linear-gradient(135deg, {} 0%, {} 100%); padding: {padding}; border-radius: {radius}; font-size: {font_size}; font-weight: 600; cursor: pointer; box-shadow: 0 4px 15px rgba(52, 168, 83, 0.4);", self.color_success(), self.color_success())
    }
    /// 危险色填充（红色渐变，用于取消/删除）
    pub(super) fn btn_danger(&self, padding: &str, radius: &str, font_size: &str) -> String {
        format!("{STYLE_BTN_BASE} background: linear-gradient(135deg, {} 0%, #c62828 100%); padding: {padding}; border-radius: {radius}; font-size: {font_size}; font-weight: 600; cursor: pointer; box-shadow: 0 4px 15px rgba(234, 67, 53, 0.4);", self.color_danger())
    }
    /// 次要按钮（实色填充，无渐变）
    pub(super) fn btn_solid(&self, bg: &str, padding: &str, radius: &str, font_size: &str) -> String {
        format!("{STYLE_BTN_BASE} background: {bg}; padding: {padding}; border-radius: {radius}; font-size: {font_size}; font-weight: 600;")
    }

    // ------------------------------------------------------------------------
    // 背景渐变 — 跨主题一致（不依赖 AppTheme）
    // ------------------------------------------------------------------------
    /// 错误提示背景（淡红渐变 + 红边 + 红阴影）
    pub(super) fn bg_error(&self) -> &'static str {
        "padding: 16px; background: linear-gradient(135deg, #fce8e8 0%, #ffe8e8 100%); border: 1px solid #f5c6cb; border-radius: 12px; box-shadow: 0 4px 15px rgba(234, 67, 53, 0.15);"
    }
    /// 统计卡片背景（淡灰渐变）
    pub(super) fn bg_card_subtle(&self) -> &'static str {
        "background: linear-gradient(135deg, #f8f9fa 0%, #f0f0f0 100%); border-radius: 12px;"
    }
    /// 列表卡片背景（白渐变）
    pub(super) fn bg_card_white(&self) -> &'static str {
        "background: linear-gradient(135deg, #f8f9fa 0%, #fff 100%); border-radius: 12px; border: 1px solid #e8e8e8;"
    }
    /// 历史/成功图标背景（绿渐变）
    pub(super) fn bg_icon_success(&self) -> &'static str {
        "background: linear-gradient(135deg, #34a853 0%, #2ecc71 100%);"
    }
    /// 下载/批量 progress 填充渐变（仅渐变函数，不含 `background:` 前缀，便于嵌入到 `background: ...;` 组合）
    pub(super) fn progress_gradient_success(&self) -> &'static str {
        "linear-gradient(135deg, #34a853 0%, #2ecc71 100%)"
    }
}
