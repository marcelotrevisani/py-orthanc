use iced::widget::{button, column, container, row, text, vertical_space};
use iced::{Alignment, Element, Length};

use super::{icons, theme};
use crate::app::{Message, Page};

/// GitHub Sponsors page for the project author.
pub const SPONSOR_URL: &str = "https://github.com/sponsors/marcelotrevisani";

pub fn view(active_page: Page) -> Element<'static, Message> {
    let nav_button = |label: &str, page: Page, is_active: bool| -> Element<'static, Message> {
        let label = label.to_string();
        let btn = button(text(label).size(14).color(if is_active {
            theme::TEXT_PRIMARY()
        } else {
            theme::TEXT_SECONDARY()
        }))
        .on_press(Message::NavigateTo(page))
        .padding([10, 20])
        .width(Length::Fill)
        .style(move |_theme, status| {
            let bg = if is_active {
                theme::BG_MID()
            } else if matches!(status, button::Status::Hovered) {
                theme::BG_DARK()
            } else {
                iced::Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: theme::TEXT_PRIMARY(),
                border: iced::Border::default(),
                shadow: iced::Shadow::default(),
            }
        });
        btn.into()
    };

    let content = column![
        container(text("Orthanc").size(20).color(theme::ACCENT_LIGHT())).padding(iced::Padding {
            top: 20.0,
            right: 20.0,
            bottom: 10.0,
            left: 20.0
        }),
        container(
            text("Python Env Manager")
                .size(11)
                .color(theme::TEXT_MUTED())
        )
        .padding(iced::Padding {
            top: 0.0,
            right: 20.0,
            bottom: 20.0,
            left: 20.0
        }),
        nav_button(
            "Environments",
            Page::Environments,
            active_page == Page::Environments
        ),
        nav_button(
            "Pull Requests",
            Page::PullRequests,
            active_page == Page::PullRequests
        ),
        nav_button("Settings", Page::Settings, active_page == Page::Settings),
        vertical_space(),
        container(
            button(
                row![
                    icons::heart(14.0, theme::DANGER()),
                    text("Sponsor").size(13),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .on_press(Message::OpenUrl(SPONSOR_URL.to_string()))
            .padding([8, 12])
            .width(Length::Fill)
            .style(theme::secondary_button(6.0)),
        )
        .padding([0, 12]),
        container(
            text(concat!("v", env!("CARGO_PKG_VERSION")))
                .size(11)
                .color(theme::TEXT_MUTED())
        )
        .padding([10, 20]),
    ]
    .width(200)
    .height(Length::Fill)
    .align_x(Alignment::Start);

    container(content)
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(theme::BG_DARK())),
            border: iced::Border {
                color: theme::BORDER(),
                width: 0.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}
