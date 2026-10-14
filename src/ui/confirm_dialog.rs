use iced::widget::{button, column, container, horizontal_space, opaque, row, text};
use iced::{Element, Length};

use super::theme;
use crate::app::Message;

pub fn view_owned(title: String, message: String) -> Element<'static, Message> {
    view_inner(title, message)
}

fn view_inner(title: String, message: String) -> Element<'static, Message> {
    let dialog = container(
        column![
            text(title.to_string())
                .size(18)
                .color(theme::TEXT_PRIMARY()),
            text(message.to_string())
                .size(14)
                .color(theme::TEXT_SECONDARY()),
            row![
                horizontal_space(),
                button(text("Cancel").size(13).color(theme::TEXT_SECONDARY()))
                    .on_press(Message::HideConfirmDialog)
                    .padding([8, 16])
                    .style(theme::secondary_button(6.0)),
                button(text("Delete").size(13).color(theme::TEXT_PRIMARY()))
                    .on_press(Message::ConfirmDelete)
                    .padding([8, 20])
                    .style(|_theme, status| {
                        let bg = if matches!(status, button::Status::Hovered) {
                            iced::Color::from_rgb(0.7, 0.2, 0.2)
                        } else {
                            theme::DANGER()
                        };
                        button::Style {
                            background: Some(iced::Background::Color(bg)),
                            text_color: theme::TEXT_PRIMARY(),
                            border: iced::Border {
                                radius: 6.0.into(),
                                ..Default::default()
                            },
                            shadow: iced::Shadow::default(),
                        }
                    }),
            ]
            .spacing(8),
        ]
        .spacing(16)
        .width(420),
    )
    .padding(24)
    .style(|_theme| container::Style {
        background: Some(iced::Background::Color(theme::BG_DARK())),
        border: iced::Border {
            color: theme::BORDER(),
            width: 1.0,
            radius: 12.0.into(),
        },
        shadow: iced::Shadow {
            color: iced::Color::BLACK,
            offset: iced::Vector::new(0.0, 4.0),
            blur_radius: 20.0,
        },
        ..Default::default()
    });

    // Backdrop (opaque: blocks clicks to the table underneath)
    opaque(
        container(
            container(dialog)
                .width(Length::Shrink)
                .height(Length::Shrink),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(iced::Color::from_rgba(
                0.0, 0.0, 0.0, 0.5,
            ))),
            ..Default::default()
        }),
    )
}
