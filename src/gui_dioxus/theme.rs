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
/// 进度条底槽
pub(super) const STYLE_PROGRESS_TRACK: &str = "height: 10px; background: #e8e8e8; border-radius: 5px; overflow: hidden;";

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
}
