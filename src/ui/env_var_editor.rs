use iced::widget::{button, column, container, horizontal_space, row, text, text_input};
use iced::{Alignment, Element, Length};

use super::theme;
use crate::app::{EnvVarRow, EnvVarSource, Message};

pub fn view(rows: &[EnvVarRow]) -> Element<'_, Message> {
    let header = row![
        text("Key")
            .size(12)
            .color(theme::TEXT_MUTED())
            .width(Length::FillPortion(3)),
        text("Value")
            .size(12)
            .color(theme::TEXT_MUTED())
            .width(Length::FillPortion(4)),
        text("Source")
            .size(12)
            .color(theme::TEXT_MUTED())
            .width(Length::FillPortion(1)),
        text("").width(60),
    ]
    .spacing(8)
    .padding(iced::Padding {
        top: 0.0,
        right: 0.0,
        bottom: 8.0,
        left: 0.0,
    });

    let var_rows: Vec<Element<Message>> = rows
        .iter()
        .enumerate()
        .map(|(idx, var_row)| {
            let (source_label, source_color) = match &var_row.source {
                EnvVarSource::Local => ("local", theme::ACCENT_LIGHT()),
                EnvVarSource::Global => ("global", theme::TEXT_MUTED()),
                EnvVarSource::Override => ("override", theme::WARNING()),
            };

            let is_global = var_row.source == EnvVarSource::Global;
            let key_val = var_row.key.clone();

            let key_input: Element<Message> = if is_global {
                text(var_row.key.clone())
                    .size(13)
                    .color(theme::TEXT_SECONDARY())
                    .width(Length::FillPortion(3))
                    .into()
            } else {
                text_input("Key", &var_row.key)
                    .on_input(move |v| Message::EnvVarKeyChanged(idx, v))
                    .size(13)
                    .width(Length::FillPortion(3))
                    .into()
            };

            let value_input: Element<Message> = if is_global {
                text(var_row.value.clone())
                    .size(13)
                    .color(theme::TEXT_SECONDARY())
                    .width(Length::FillPortion(4))
                    .into()
            } else {
                text_input("Value", &var_row.value)
                    .on_input(move |v| Message::EnvVarValueChanged(idx, v))
                    .size(13)
                    .width(Length::FillPortion(4))
                    .into()
            };

            let source_el = container(text(source_label.to_string()).size(11).color(source_color))
                .width(Length::FillPortion(1));

            let action: Element<Message> = if is_global {
                button(text("Override").size(11).color(theme::WARNING()))
                    .on_press(Message::OverrideGlobalVar(key_val))
                    .padding([2, 6])
                    .style(|_theme, _status| button::Style {
                        background: None,
                        text_color: theme::WARNING(),
                        border: iced::Border {
                            color: theme::WARNING(),
                            width: 1.0,
                            radius: 4.0.into(),
                        },
                        shadow: iced::Shadow::default(),
                    })
                    .into()
            } else {
                button(text("x").size(12).color(theme::DANGER()))
                    .on_press(Message::RemoveEnvVar(idx))
                    .padding([2, 8])
                    .style(theme::text_button(theme::DANGER(), theme::DANGER(), 4.0))
                    .into()
            };

            container(
                row![
                    key_input,
                    value_input,
                    source_el,
                    container(action).width(60)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([4, 0])
            .into()
        })
        .collect();

    let footer = row![
        button(text("+ Add Variable").size(12).color(theme::ACCENT_LIGHT()))
            .on_press(Message::AddEnvVar)
            .padding([6, 12])
            .style(theme::outline_button(6.0)),
        horizontal_space(),
        button(text("Save").size(12).color(theme::TEXT_ON_ACCENT()))
            .on_press(Message::SaveEnvVars)
            .padding([6, 20])
            .style(theme::primary_button(6.0)),
    ]
    .spacing(8)
    .padding(iced::Padding {
        top: 12.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    });

    let mut content = column![header].spacing(0);
    for r in var_rows {
        content = content.push(r);
    }
    content = content.push(footer);

    content.into()
}
