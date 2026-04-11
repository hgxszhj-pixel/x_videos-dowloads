//! 应用主题定义
//! 
//! 定义深色/浅色主题及颜色方案

use iced::Color;
use std::sync::atomic::{AtomicBool, Ordering};

/// 全局主题状态
static DARK_MODE: AtomicBool = AtomicBool::new(true);

/// 获取当前是否为深色模式
pub fn is_dark_mode() -> bool {
    DARK_MODE.load(Ordering::Relaxed)
}

/// 切换主题模式
pub fn toggle_theme() {
    DARK_MODE.fetch_xor(true, Ordering::Relaxed);
}

/// 深色主题配色
pub mod dark {
    use super::*;

    pub const BACKGROUND: Color = Color::from_rgb(0.07, 0.07, 0.12);
    pub const SURFACE: Color = Color::from_rgb(0.12, 0.12, 0.18);
    pub const SURFACE_HOVER: Color = Color::from_rgb(0.18, 0.18, 0.25);
    pub const PRIMARY: Color = Color::from_rgb(0.31, 0.76, 0.97);
    pub const PRIMARY_HOVER: Color = Color::from_rgb(0.45, 0.88, 1.0);
    pub const SUCCESS: Color = Color::from_rgb(0.30, 0.69, 0.31);
    pub const DANGER: Color = Color::from_rgb(0.96, 0.26, 0.21);
    pub const WARNING: Color = Color::from_rgb(1.0, 0.76, 0.03);
    pub const TEXT: Color = Color::from_rgb(0.95, 0.95, 0.95);
    pub const TEXT_SECONDARY: Color = Color::from_rgb(0.60, 0.60, 0.65);
    pub const BORDER: Color = Color::from_rgb(0.25, 0.25, 0.32);
    pub const INPUT_BG: Color = Color::from_rgb(0.10, 0.10, 0.15);
}

/// 浅色主题配色
pub mod light {
    use super::*;

    pub const BACKGROUND: Color = Color::from_rgb(0.96, 0.96, 0.98);
    pub const SURFACE: Color = Color::from_rgb(1.0, 1.0, 1.0);
    pub const SURFACE_HOVER: Color = Color::from_rgb(0.95, 0.95, 0.97);
    pub const PRIMARY: Color = Color::from_rgb(0.0, 0.59, 0.78);
    pub const PRIMARY_HOVER: Color = Color::from_rgb(0.0, 0.75, 0.95);
    pub const SUCCESS: Color = Color::from_rgb(0.20, 0.60, 0.20);
    pub const DANGER: Color = Color::from_rgb(0.85, 0.15, 0.15);
    pub const WARNING: Color = Color::from_rgb(0.85, 0.65, 0.0);
    pub const TEXT: Color = Color::from_rgb(0.10, 0.10, 0.12);
    pub const TEXT_SECONDARY: Color = Color::from_rgb(0.45, 0.45, 0.50);
    pub const BORDER: Color = Color::from_rgb(0.80, 0.80, 0.85);
    pub const INPUT_BG: Color = Color::from_rgb(0.98, 0.98, 0.98);
}

/// 卡片样式属性
#[derive(Clone, Copy)]
pub struct CardStyle {
    pub background: Color,
    pub border_color: Color,
    pub shadow_color: Color,
}

impl CardStyle {
    pub fn dark() -> Self {
        CardStyle {
            background: dark::SURFACE,
            border_color: dark::BORDER,
            shadow_color: Color::from_rgb(0.0, 0.0, 0.0),
        }
    }

    pub fn light() -> Self {
        CardStyle {
            background: light::SURFACE,
            border_color: light::BORDER,
            shadow_color: Color::from_rgb(0.0, 0.0, 0.0),
        }
    }
}

/// 获取当前主题的颜色方案
pub struct AppColors {
    pub background: Color,
    pub surface: Color,
    pub surface_hover: Color,
    pub primary: Color,
    pub primary_hover: Color,
    pub success: Color,
    pub danger: Color,
    pub warning: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub border: Color,
    pub input_bg: Color,
    pub card: CardStyle,
}

impl AppColors {
    pub fn dark() -> Self {
        AppColors {
            background: dark::BACKGROUND,
            surface: dark::SURFACE,
            surface_hover: dark::SURFACE_HOVER,
            primary: dark::PRIMARY,
            primary_hover: dark::PRIMARY_HOVER,
            success: dark::SUCCESS,
            danger: dark::DANGER,
            warning: dark::WARNING,
            text: dark::TEXT,
            text_secondary: dark::TEXT_SECONDARY,
            border: dark::BORDER,
            input_bg: dark::INPUT_BG,
            card: CardStyle::dark(),
        }
    }

    pub fn light() -> Self {
        AppColors {
            background: light::BACKGROUND,
            surface: light::SURFACE,
            surface_hover: light::SURFACE_HOVER,
            primary: light::PRIMARY,
            primary_hover: light::PRIMARY_HOVER,
            success: light::SUCCESS,
            danger: light::DANGER,
            warning: light::WARNING,
            text: light::TEXT,
            text_secondary: light::TEXT_SECONDARY,
            border: light::BORDER,
            input_bg: light::INPUT_BG,
            card: CardStyle::light(),
        }
    }

    pub fn current() -> Self {
        if is_dark_mode() {
            Self::dark()
        } else {
            Self::light()
        }
    }
}
