use iced::widget::{button, column, container, horizontal_space, row, text};
use iced::{Alignment, Element, Length};

use super::{dependency_list, env_var_editor, git_status, pipeline_status, status_badge, theme};
use crate::app::{DetailsTab, Message, Orthanc};
use orthanc_lib::models::environment::PythonEnvironment;
use orthanc_lib::services::git::GitStatus;

pub fn view<'a>(env: &'a PythonEnvironment, app: &'a Orthanc) -> Element<'a, Message> {
    let active_tab = app.active_details_tab;
    // Header
    let header = container(
        row![
            text(env.name.clone()).size(16).color(theme::TEXT_PRIMARY()),
            container(status_badge::view(&env.status)).padding([0, 8]),
            horizontal_space(),
            button(text("x").size(14).color(theme::TEXT_SECONDARY()))
                .on_press(Message::SelectEnvironment(None))
                .padding([4, 10])
                .style(theme::ghost_button(theme::TEXT_SECONDARY(), 6.0)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([12, 24])
    .style(|_theme| container::Style {
        border: iced::Border {
            color: theme::BORDER(),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    });

    // Tab bar
    let tab_btn = |label: &str, tab: DetailsTab, is_active: bool| -> Element<'_, Message> {
        let label = label.to_string();
        let accent = theme::ACCENT_LIGHT();
        button(text(label).size(13).color(if is_active {
            accent
        } else {
            theme::TEXT_SECONDARY()
        }))
        .on_press(Message::SwitchDetailsTab(tab))
        .padding([10, 16])
        .style(move |_theme, status| {
            let border_bottom = if is_active {
                accent
            } else {
                iced::Color::TRANSPARENT
            };
            let bg = if matches!(status, button::Status::Hovered) && !is_active {
                theme::BG_DARK()
            } else {
                iced::Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: if is_active {
                    accent
                } else {
                    theme::TEXT_SECONDARY()
                },
                border: iced::Border {
                    color: border_bottom,
                    width: if is_active { 2.0 } else { 0.0 },
                    radius: 0.0.into(),
                },
                shadow: iced::Shadow::default(),
            }
        })
        .into()
    };

    let tabs = container(
        row![
            tab_btn("Info", DetailsTab::Info, active_tab == DetailsTab::Info),
            tab_btn(
                "Env Vars",
                DetailsTab::EnvVars,
                active_tab == DetailsTab::EnvVars
            ),
            tab_btn(
                "Dependencies",
                DetailsTab::Dependencies,
                active_tab == DetailsTab::Dependencies
            ),
            tab_btn(
                "Pipelines",
                DetailsTab::Pipelines,
                active_tab == DetailsTab::Pipelines
            ),
        ]
        .spacing(0),
    )
    .style(|_theme| container::Style {
        border: iced::Border {
            color: theme::BORDER(),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    })
    .padding([0, 24]);

    // Tab content
    let content: Element<Message> = match active_tab {
        DetailsTab::Info => info_tab(env, app.git_status.get(&env.id)),
        DetailsTab::EnvVars => env_var_editor::view(&app.env_var_rows),
        DetailsTab::Dependencies => dependency_list::view(app),
        DetailsTab::Pipelines => {
            if app.loading_pipelines {
                container(
                    text("Loading pipelines...")
                        .size(13)
                        .color(theme::TEXT_MUTED()),
                )
                .padding(16)
                .into()
            } else {
                pipeline_status::view(&app.pipeline_runs)
            }
        }
    };

    let content_container = container(content)
        .padding([16, 24])
        .width(Length::Fill)
        .max_height(300);

    container(column![header, tabs, content_container].spacing(0))
        .width(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(theme::BG_DARK())),
            border: iced::Border {
                color: theme::BORDER(),
                width: 1.0,
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}

fn info_tab<'a>(env: &'a PythonEnvironment, git: Option<&'a GitStatus>) -> Element<'a, Message> {
    let info_row = |label: &str, value: String| -> Element<'_, Message> {
        column![
            text(label.to_string()).size(12).color(theme::TEXT_MUTED()),
            text(value).size(13).color(theme::TEXT_PRIMARY()),
        ]
        .spacing(2)
        .into()
    };

    let created = env.created_at.format("%Y-%m-%d %H:%M").to_string();
    let synced = env
        .last_synced_at
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "Never".to_string());

    let env_id = env.id.clone();
    let repo_path_row: Element<'_, Message> = column![
        text("Repository Path").size(12).color(theme::TEXT_MUTED()),
        row![
            text(env.repo_path.to_string_lossy().to_string())
                .size(13)
                .color(theme::TEXT_PRIMARY()),
            button(text("Change").size(11).color(theme::ACCENT_LIGHT()))
                .on_press(Message::PickRepoPath(env_id))
                .padding([3, 10])
                .style(theme::outline_button(4.0)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    ]
    .spacing(2)
    .into();

    let mut rows = column![
        row![
            container(info_row("Name", env.name.clone())).width(Length::FillPortion(1)),
            container(info_row("Python Version", env.python_version.clone()))
                .width(Length::FillPortion(1)),
        ]
        .spacing(24),
        row![
            container(repo_path_row).width(Length::FillPortion(1)),
            container(info_row(
                "Venv Path",
                env.venv_path.to_string_lossy().to_string(),
            ))
            .width(Length::FillPortion(1)),
        ]
        .spacing(24),
        row![
            container(info_row("Created", created)).width(Length::FillPortion(1)),
            container(info_row("Last Updated", synced)).width(Length::FillPortion(1)),
        ]
        .spacing(24),
    ]
    .spacing(16);

    if let Some(status) = git {
        rows = rows.push(git_status::details(status));
    }

    if env.staleness.is_stale {
        let reason = env
            .staleness
            .reason
            .clone()
            .unwrap_or_else(|| "Dependencies may be outdated".to_string());
        rows = rows.push(
            column![
                text("Staleness").size(12).color(theme::TEXT_MUTED()),
                text(reason).size(13).color(theme::WARNING()),
            ]
            .spacing(2),
        );
    }

    rows.into()
}
