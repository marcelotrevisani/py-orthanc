use iced::widget::{container, row, text, Space};
use iced::Element;

use super::theme;
use crate::app::Message;
use orthanc_lib::models::environment::EnvStatus;

pub fn view(status: &EnvStatus) -> Element<'static, Message> {
    let (label, color) = match status {
        EnvStatus::Ready => ("Ready", theme::SUCCESS()),
        EnvStatus::Creating => ("Creating", theme::INFO()),
        EnvStatus::Syncing => ("Updating", theme::INFO()),
        EnvStatus::Error(_) => ("Error", theme::DANGER()),
        EnvStatus::Stale => ("Stale", theme::WARNING()),
    };

    let label = label.to_string();
    let dot = container(Space::new(0, 0))
        .width(8)
        .height(8)
        .style(move |_theme| container::Style {
            background: Some(iced::Background::Color(color)),
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    container(
        row![dot, text(label).size(12).color(color)]
            .spacing(6)
            .align_y(iced::Alignment::Center),
    )
    .padding([4, 10])
    .style(move |_theme| container::Style {
        background: Some(iced::Background::Color(color.scale_alpha(0.1))),
        border: iced::Border {
            radius: 12.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}
