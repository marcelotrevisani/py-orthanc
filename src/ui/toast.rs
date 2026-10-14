use iced::widget::{button, column, container, row, text};
use iced::{Alignment, Element, Length};

use super::theme;
use crate::app::{Message, Toast, ToastVariant};

pub fn view(toasts: &[Toast]) -> Element<'_, Message> {
    let toast_elements: Vec<Element<Message>> = toasts
        .iter()
        .map(|toast| {
            let color = match &toast.variant {
                ToastVariant::Info => theme::INFO(),
                ToastVariant::Success => theme::SUCCESS(),
                ToastVariant::Error => theme::DANGER(),
                ToastVariant::Warning => theme::WARNING(),
            };

            let id = toast.id;
            let msg = toast.message.clone();

            container(
                row![
                    text(msg)
                        .size(13)
                        .color(theme::TEXT_PRIMARY())
                        .width(Length::Fill),
                    button(text("x").size(12).color(theme::TEXT_SECONDARY()))
                        .on_press(Message::DismissToast(id))
                        .padding([2, 6])
                        .style(|_theme, _status| button::Style {
                            background: None,
                            text_color: theme::TEXT_SECONDARY(),
                            border: iced::Border::default(),
                            shadow: iced::Shadow::default(),
                        }),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([10, 16])
            .width(360)
            .style(move |_theme| container::Style {
                background: Some(iced::Background::Color(theme::BG_DARK())),
                border: iced::Border {
                    color,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                shadow: iced::Shadow {
                    color: iced::Color::BLACK,
                    offset: iced::Vector::new(0.0, 2.0),
                    blur_radius: 8.0,
                },
                ..Default::default()
            })
            .into()
        })
        .collect();

    container(column(toast_elements).spacing(8).width(360))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .padding(20)
        .into()
}
