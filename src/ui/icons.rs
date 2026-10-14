//! Small inline SVG icons (24×24, stroke-based). Drawn in black and tinted at render time.

use iced::widget::svg;
use iced::{Color, Element};

fn icon<'a, M: 'a>(body: &str, size: f32, color: Color) -> Element<'a, M> {
    let doc = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{}</svg>"#,
        body
    );
    svg(svg::Handle::from_memory(doc.into_bytes()))
        .width(size)
        .height(size)
        .style(move |_theme, _status| svg::Style { color: Some(color) })
        .into()
}

pub fn terminal<'a, M: 'a>(size: f32, color: Color) -> Element<'a, M> {
    icon(
        r#"<rect x="2" y="3" width="20" height="18" rx="2"/><path d="M6 9l4 3-4 3"/><path d="M12 16h6"/>"#,
        size,
        color,
    )
}

pub fn trash<'a, M: 'a>(size: f32, color: Color) -> Element<'a, M> {
    icon(
        r#"<path d="M3 6h18"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="M10 11v6"/><path d="M14 11v6"/>"#,
        size,
        color,
    )
}

pub fn refresh<'a, M: 'a>(size: f32, color: Color) -> Element<'a, M> {
    icon(
        r#"<path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16"/><path d="M16 16h5v5"/>"#,
        size,
        color,
    )
}

pub fn branch<'a, M: 'a>(size: f32, color: Color) -> Element<'a, M> {
    icon(
        r#"<path d="M6 3v12"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/>"#,
        size,
        color,
    )
}

/// Generic "PC" badge for PyCharm (not the JetBrains logo).
pub fn pycharm<'a, M: 'a>(size: f32, color: Color) -> Element<'a, M> {
    icon(
        r#"<rect x="2" y="2" width="20" height="20" rx="3"/><path d="M6 16V7h3a2.5 2.5 0 0 1 0 5H6"/><path d="M18 8.5a3 3 0 0 0-5 2.2v2.6a3 3 0 0 0 5 2.2"/><path d="M6 19h6"/>"#,
        size,
        color,
    )
}

pub fn heart<'a, M: 'a>(size: f32, color: Color) -> Element<'a, M> {
    icon(
        r#"<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/>"#,
        size,
        color,
    )
}
