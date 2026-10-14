use iced::widget::{button, column, container, horizontal_space, row, text, text_input};
use iced::{Alignment, Element, Length};

use super::theme;
use crate::app::Message;

pub fn view<'a>(
    repo_path: &'a str,
    env_name: &'a str,
    venv_path: &'a str,
    default_venv_dir: Option<&str>,
    python_version: &'a str,
    creating: bool,
) -> Element<'a, Message> {
    let display_name = if env_name.is_empty() {
        std::path::Path::new(repo_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        env_name.to_string()
    };

    // Compute the effective venv path for preview
    let venv_preview = if !venv_path.is_empty() {
        venv_path.to_string()
    } else if !repo_path.is_empty() {
        crate::app::default_venv_path(
            std::path::Path::new(repo_path),
            &display_name,
            default_venv_dir,
        )
        .to_string_lossy()
        .to_string()
    } else {
        String::new()
    };

    let browse_btn = |msg: Message| -> Element<'_, Message> {
        button(text("Browse").size(12).color(theme::TEXT_PRIMARY()))
            .on_press(msg)
            .padding([6, 12])
            .style(|_theme, status| {
                let bg = if matches!(status, button::Status::Hovered) {
                    theme::BG_MID()
                } else {
                    theme::BG_LIGHT()
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
            })
            .into()
    };

    let mut dialog_content = column![
        text("Create New Environment")
            .size(18)
            .color(theme::TEXT_PRIMARY()),
        // Repo path
        column![
            text("Repository Path").size(12).color(theme::TEXT_MUTED()),
            row![
                text_input("Select repository folder...", repo_path)
                    .on_input(Message::CreateRepoPathChanged)
                    .size(13)
                    .width(Length::Fill),
                browse_btn(Message::PickFolder),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(4),
        // Environment name
        column![
            text("Environment Name").size(12).color(theme::TEXT_MUTED()),
            text_input("Defaults to project folder name", env_name)
                .on_input(Message::CreateEnvNameChanged)
                .size(13),
        ]
        .spacing(4),
        // Venv path
        column![
            text("Virtual Environment Path")
                .size(12)
                .color(theme::TEXT_MUTED()),
            row![
                text_input("Leave empty for default location", venv_path)
                    .on_input(Message::CreateVenvPathChanged)
                    .size(13)
                    .width(Length::Fill),
                browse_btn(Message::PickVenvFolder),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(4),
    ]
    .spacing(16)
    .width(500);

    // Effective path preview
    if !venv_preview.is_empty() {
        dialog_content = dialog_content.push(
            column![
                text("Effective Venv Location")
                    .size(12)
                    .color(theme::TEXT_MUTED()),
                text(venv_preview).size(12).color(theme::TEXT_SECONDARY()),
            ]
            .spacing(2),
        );
    }

    // Python version
    dialog_content = dialog_content.push(
        column![
            text("Python Version").size(12).color(theme::TEXT_MUTED()),
            text_input("3.12", python_version)
                .on_input(Message::CreatePythonVersionChanged)
                .size(13),
        ]
        .spacing(4),
    );

    // Actions
    dialog_content = dialog_content.push(
        row![
            horizontal_space(),
            button(text("Cancel").size(13).color(theme::TEXT_SECONDARY()))
                .on_press(Message::HideCreateDialog)
                .padding([8, 16])
                .style(theme::secondary_button(6.0)),
            if creating {
                button(text("Creating...").size(13).color(theme::TEXT_SECONDARY()))
                    .padding([8, 20])
                    .style(|_theme, _status| button::Style {
                        background: Some(iced::Background::Color(theme::BG_MID())),
                        text_color: theme::TEXT_SECONDARY(),
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..Default::default()
                        },
                        shadow: iced::Shadow::default(),
                    })
            } else {
                button(text("Create").size(13).color(theme::TEXT_ON_ACCENT()))
                    .on_press(Message::SubmitCreateEnvironment)
                    .padding([8, 20])
                    .style(theme::primary_button(6.0))
            },
        ]
        .spacing(8),
    );

    let dialog = container(dialog_content)
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

    // Backdrop
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
    })
    .into()
}
