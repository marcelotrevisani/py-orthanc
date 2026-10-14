// Colour accessors are UPPER_CASE functions so call sites read like the constants they replace.
#![allow(non_snake_case)]

use iced::Color;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag: false = dark, true = light
static IS_LIGHT: AtomicBool = AtomicBool::new(false);

pub fn set_light_theme(light: bool) {
    IS_LIGHT.store(light, Ordering::Relaxed);
}

pub fn is_light() -> bool {
    IS_LIGHT.load(Ordering::Relaxed)
}

// ── Dark palette ────────────────────────────────────────────────────────

mod dark {
    use iced::color;
    use iced::Color;

    pub const BG_DARKEST: Color = color!(0x0f172a);
    pub const BG_DARK: Color = color!(0x1e293b);
    pub const BG_MID: Color = color!(0x334155);
    pub const BG_LIGHT: Color = color!(0x475569);

    pub const TEXT_PRIMARY: Color = color!(0xf1f5f9);
    pub const TEXT_SECONDARY: Color = color!(0x94a3b8);
    pub const TEXT_MUTED: Color = color!(0x64748b);

    pub const ACCENT: Color = color!(0x6366f1);
    pub const ACCENT_LIGHT: Color = color!(0x818cf8);
    pub const ACCENT_DARK: Color = color!(0x4f46e5);

    pub const SUCCESS: Color = color!(0x4ade80);
    pub const WARNING: Color = color!(0xfbbf24);
    pub const DANGER: Color = color!(0xf87171);
    pub const INFO: Color = color!(0x60a5fa);

    pub const BORDER: Color = color!(0x334155);
    pub const TEXT_ON_ACCENT: Color = color!(0xf1f5f9); // always light for text on accent bg
}

// ── Light palette ───────────────────────────────────────────────────────

mod light {
    use iced::color;
    use iced::Color;

    pub const BG_DARKEST: Color = color!(0xffffff);
    pub const BG_DARK: Color = color!(0xf8fafc); // slate-50
    pub const BG_MID: Color = color!(0xe2e8f0); // slate-200
    pub const BG_LIGHT: Color = color!(0xcbd5e1); // slate-300

    pub const TEXT_PRIMARY: Color = color!(0x0f172a); // slate-900
    pub const TEXT_SECONDARY: Color = color!(0x475569); // slate-600
    pub const TEXT_MUTED: Color = color!(0x94a3b8); // slate-400

    pub const ACCENT: Color = color!(0x4f46e5); // indigo-600
    pub const ACCENT_LIGHT: Color = color!(0x6366f1); // indigo-500
    pub const ACCENT_DARK: Color = color!(0x4338ca); // indigo-700

    pub const SUCCESS: Color = color!(0x16a34a); // green-600
    pub const WARNING: Color = color!(0xd97706); // amber-600
    pub const DANGER: Color = color!(0xdc2626); // red-600
    pub const INFO: Color = color!(0x2563eb); // blue-600

    pub const BORDER: Color = color!(0xcbd5e1); // slate-300
    pub const TEXT_ON_ACCENT: Color = color!(0xffffff); // white text on accent bg
}

// ── Accessors ───────────────────────────────────────────────────────────

pub fn BG_DARKEST() -> Color {
    if is_light() {
        light::BG_DARKEST
    } else {
        dark::BG_DARKEST
    }
}
pub fn BG_DARK() -> Color {
    if is_light() {
        light::BG_DARK
    } else {
        dark::BG_DARK
    }
}
pub fn BG_MID() -> Color {
    if is_light() {
        light::BG_MID
    } else {
        dark::BG_MID
    }
}
pub fn BG_LIGHT() -> Color {
    if is_light() {
        light::BG_LIGHT
    } else {
        dark::BG_LIGHT
    }
}
pub fn TEXT_PRIMARY() -> Color {
    if is_light() {
        light::TEXT_PRIMARY
    } else {
        dark::TEXT_PRIMARY
    }
}
pub fn TEXT_SECONDARY() -> Color {
    if is_light() {
        light::TEXT_SECONDARY
    } else {
        dark::TEXT_SECONDARY
    }
}
pub fn TEXT_MUTED() -> Color {
    if is_light() {
        light::TEXT_MUTED
    } else {
        dark::TEXT_MUTED
    }
}
pub fn ACCENT() -> Color {
    if is_light() {
        light::ACCENT
    } else {
        dark::ACCENT
    }
}
pub fn ACCENT_LIGHT() -> Color {
    if is_light() {
        light::ACCENT_LIGHT
    } else {
        dark::ACCENT_LIGHT
    }
}
pub fn ACCENT_DARK() -> Color {
    if is_light() {
        light::ACCENT_DARK
    } else {
        dark::ACCENT_DARK
    }
}
pub fn SUCCESS() -> Color {
    if is_light() {
        light::SUCCESS
    } else {
        dark::SUCCESS
    }
}
pub fn WARNING() -> Color {
    if is_light() {
        light::WARNING
    } else {
        dark::WARNING
    }
}
pub fn DANGER() -> Color {
    if is_light() {
        light::DANGER
    } else {
        dark::DANGER
    }
}
pub fn INFO() -> Color {
    if is_light() {
        light::INFO
    } else {
        dark::INFO
    }
}
pub fn BORDER() -> Color {
    if is_light() {
        light::BORDER
    } else {
        dark::BORDER
    }
}
pub fn TEXT_ON_ACCENT() -> Color {
    if is_light() {
        light::TEXT_ON_ACCENT
    } else {
        dark::TEXT_ON_ACCENT
    }
}

pub fn orthanc_theme() -> iced::Theme {
    if is_light() {
        iced::Theme::custom(
            "Orthanc Light".to_string(),
            iced::theme::Palette {
                background: light::BG_DARKEST,
                text: light::TEXT_PRIMARY,
                primary: light::ACCENT,
                success: light::SUCCESS,
                danger: light::DANGER,
            },
        )
    } else {
        iced::Theme::custom(
            "Orthanc Dark".to_string(),
            iced::theme::Palette {
                background: dark::BG_DARKEST,
                text: dark::TEXT_PRIMARY,
                primary: dark::ACCENT,
                success: dark::SUCCESS,
                danger: dark::DANGER,
            },
        )
    }
}

// ── Button styles ───────────────────────────────────────────────────────

use iced::widget::button;

fn button_style(
    rest: Color,
    hover: Color,
    text: Color,
    border: Option<Color>,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_theme, status| button::Style {
        background: Some(iced::Background::Color(
            if matches!(status, button::Status::Hovered) {
                hover
            } else {
                rest
            },
        )),
        text_color: text,
        border: iced::Border {
            color: border.unwrap_or(Color::TRANSPARENT),
            width: if border.is_some() { 1.0 } else { 0.0 },
            radius: radius.into(),
        },
        shadow: iced::Shadow::default(),
    }
}

/// Filled accent button (main action).
pub fn primary_button(radius: f32) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    button_style(ACCENT(), ACCENT_DARK(), TEXT_ON_ACCENT(), None, radius)
}

/// Bordered neutral button (Cancel, Refresh, …).
pub fn secondary_button(radius: f32) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    button_style(
        Color::TRANSPARENT,
        BG_MID(),
        TEXT_SECONDARY(),
        Some(BORDER()),
        radius,
    )
}

/// Accent-bordered button (Add, Browse, Change).
pub fn outline_button(radius: f32) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    button_style(
        Color::TRANSPARENT,
        ACCENT().scale_alpha(0.1),
        ACCENT_LIGHT(),
        Some(ACCENT()),
        radius,
    )
}

/// Borderless text button tinted with `tint` on hover (e.g. Remove in DANGER).
pub fn text_button(
    text: Color,
    tint: Color,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    button_style(
        Color::TRANSPARENT,
        tint.scale_alpha(0.1),
        text,
        None,
        radius,
    )
}

/// Borderless button that only shows a background on hover.
pub fn ghost_button(
    text: Color,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    button_style(Color::TRANSPARENT, BG_MID(), text, None, radius)
}

/// Segmented-control option: accent-tinted when active.
pub fn toggle_button(active: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    if active {
        button_style(
            ACCENT().scale_alpha(0.15),
            ACCENT().scale_alpha(0.15),
            ACCENT_LIGHT(),
            Some(ACCENT_LIGHT()),
            6.0,
        )
    } else {
        button_style(
            Color::TRANSPARENT,
            BG_MID(),
            TEXT_SECONDARY(),
            Some(BORDER()),
            6.0,
        )
    }
}
