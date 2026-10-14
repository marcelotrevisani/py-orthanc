use iced::widget::{
    button, column, container, horizontal_space, mouse_area, row, scrollable, text, tooltip,
};
use iced::{Alignment, Element, Length};

use super::{icons, status_badge, theme};
use crate::app::Message;
use std::collections::HashMap;

use orthanc_lib::models::environment::PythonEnvironment;
use orthanc_lib::services::git::GitStatus;

pub fn view<'a>(
    environments: &'a [PythonEnvironment],
    selected_id: &'a Option<String>,
    context_menu_env_id: Option<&'a str>,
    git_status: &'a HashMap<String, GitStatus>,
) -> Element<'a, Message> {
    // Header bar
    let header = container(
        row![
            text(format!("Environments ({})", environments.len()))
                .size(16)
                .color(theme::TEXT_PRIMARY()),
            horizontal_space(),
            button(text("+ New").size(13).color(theme::TEXT_ON_ACCENT()))
                .on_press(Message::ShowCreateDialog)
                .padding([6, 16])
                .style(theme::primary_button(6.0)),
            button(text("Refresh").size(13).color(theme::TEXT_SECONDARY()))
                .on_press(Message::RefreshEnvironments)
                .padding([6, 12])
                .style(theme::secondary_button(6.0)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .padding([16, 24]);

    // Table header
    let table_header = container(
        row![
            text("Name")
                .size(12)
                .color(theme::TEXT_MUTED())
                .width(Length::FillPortion(3)),
            text("Python")
                .size(12)
                .color(theme::TEXT_MUTED())
                .width(Length::FillPortion(1)),
            text("Status")
                .size(12)
                .color(theme::TEXT_MUTED())
                .width(Length::FillPortion(1)),
            text("Actions")
                .size(12)
                .color(theme::TEXT_MUTED())
                .width(Length::FillPortion(2)),
        ]
        .spacing(12)
        .padding([0, 24]),
    )
    .padding([8, 0])
    .style(|_theme| container::Style {
        border: iced::Border {
            color: theme::BORDER(),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    });

    // Rows
    let rows: Vec<Element<Message>> = if environments.is_empty() {
        vec![container(
            text("No environments yet. Click '+ New' to create one.")
                .size(14)
                .color(theme::TEXT_MUTED()),
        )
        .padding(24)
        .width(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .into()]
    } else {
        environments
            .iter()
            .map(|env| {
                let is_selected = selected_id.as_deref() == Some(&env.id);
                let has_context_menu = context_menu_env_id == Some(env.id.as_str());
                let id = env.id.clone();
                let id_sync = env.id.clone();
                let id_term = env.id.clone();
                let id_del = env.id.clone();
                let id_pc = env.id.clone();
                let id_ctx = env.id.clone();

                let action_btn = |icon: Element<'a, Message>,
                                  label: &str,
                                  msg: Message|
                 -> Element<'a, Message> {
                    let btn = button(icon)
                        .on_press(msg)
                        .padding(5)
                        .style(theme::secondary_button(4.0));
                    tooltip(
                        btn,
                        container(
                            text(label.to_string())
                                .size(11)
                                .color(theme::TEXT_PRIMARY()),
                        )
                        .padding([3, 6])
                        .style(|_| container::Style {
                            background: Some(iced::Background::Color(theme::BG_LIGHT())),
                            border: iced::Border {
                                color: theme::BORDER(),
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        }),
                        tooltip::Position::Top,
                    )
                    .into()
                };
                let icon_color = theme::TEXT_SECONDARY();

                let bg = if is_selected {
                    theme::BG_MID()
                } else {
                    iced::Color::TRANSPARENT
                };

                let env_row = button(
                    row![
                        column![
                            text(env.name.clone()).size(14).color(theme::TEXT_PRIMARY()),
                            text(format!("Repo: {}", env.repo_path.to_string_lossy()))
                                .size(11)
                                .color(theme::TEXT_MUTED()),
                            text(format!("Venv: {}", env.venv_path.to_string_lossy()))
                                .size(11)
                                .color(theme::TEXT_MUTED()),
                        ]
                        .push_maybe(git_status.get(&env.id).map(super::git_status::badge))
                        .spacing(2)
                        .width(Length::FillPortion(3)),
                        text(env.python_version.clone())
                            .size(13)
                            .color(theme::TEXT_SECONDARY())
                            .width(Length::FillPortion(1)),
                        container(status_badge::view(&env.status)).width(Length::FillPortion(1)),
                        row![
                            action_btn(
                                icons::refresh(16.0, icon_color),
                                "Update deps",
                                Message::SyncEnvironment(id_sync)
                            ),
                            action_btn(
                                icons::terminal(16.0, icon_color),
                                "Open terminal",
                                Message::OpenTerminal(id_term)
                            ),
                            action_btn(
                                icons::pycharm(16.0, icon_color),
                                "Open in PyCharm",
                                Message::OpenPycharm(id_pc)
                            ),
                            action_btn(
                                icons::trash(16.0, theme::DANGER()),
                                "Delete",
                                Message::ShowDeleteConfirm(id_del)
                            ),
                        ]
                        .spacing(4)
                        .width(Length::FillPortion(2)),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center)
                    .padding([12, 24]),
                )
                .on_press(Message::SelectEnvironment(Some(id)))
                .width(Length::Fill)
                .style(move |_theme, status| {
                    let bg = if matches!(status, button::Status::Hovered) && !is_selected {
                        theme::BG_DARK()
                    } else {
                        bg
                    };
                    button::Style {
                        background: Some(iced::Background::Color(bg)),
                        text_color: theme::TEXT_PRIMARY(),
                        border: iced::Border {
                            color: theme::BORDER(),
                            width: 0.0,
                            radius: 0.0.into(),
                        },
                        shadow: iced::Shadow::default(),
                    }
                });

                // Wrap in mouse_area for right-click
                let row_with_mouse: Element<Message> = mouse_area(env_row)
                    .on_right_press(Message::EnvContextMenu(id_ctx))
                    .into();

                // If this row has an active context menu, show it below
                let mut row_content = column![row_with_mouse].spacing(0);
                if has_context_menu {
                    row_content = row_content.push(context_menu_view(&env.id));
                }

                container(row_content)
                    .style(|_theme| container::Style {
                        border: iced::Border {
                            color: theme::BORDER(),
                            width: 0.0,
                            radius: 0.0.into(),
                        },
                        ..Default::default()
                    })
                    .into()
            })
            .collect()
    };

    // Wrap the scrollable area in a mouse_area to dismiss context menu on click elsewhere
    let table_content =
        scrollable(column(rows).spacing(0).width(Length::Fill)).height(Length::Fill);

    let table_element: Element<Message> = if context_menu_env_id.is_some() {
        mouse_area(table_content)
            .on_press(Message::HideContextMenu)
            .into()
    } else {
        table_content.into()
    };

    column![header, table_header, table_element,]
        .width(Length::Fill)
        .into()
}

fn context_menu_view<'a>(env_id: &str) -> Element<'a, Message> {
    let id = env_id.to_string();

    let menu_item = |label: &'static str, msg: Message| {
        button(text(label).size(12).color(theme::TEXT_PRIMARY()))
            .on_press(msg)
            .padding([8, 16])
            .width(Length::Fill)
            .style(|_theme, status| {
                let bg = if matches!(status, button::Status::Hovered) {
                    theme::BG_MID()
                } else {
                    theme::BG_DARK()
                };
                button::Style {
                    background: Some(iced::Background::Color(bg)),
                    text_color: theme::TEXT_PRIMARY(),
                    border: iced::Border::default(),
                    shadow: iced::Shadow::default(),
                }
            })
    };

    container(
        column![
            menu_item("Dependencies", Message::ShowDependencies(id.clone())),
            menu_item("Show Dependency Tree", Message::ShowDepTree(id)),
        ]
        .spacing(0),
    )
    .width(180)
    .padding(4)
    .style(|_theme| container::Style {
        background: Some(iced::Background::Color(theme::BG_DARK())),
        border: iced::Border {
            color: theme::BORDER(),
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: iced::Shadow {
            color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.3),
            offset: iced::Vector::new(2.0, 2.0),
            blur_radius: 8.0,
        },
        ..Default::default()
    })
    .into()
}
