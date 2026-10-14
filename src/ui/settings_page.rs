use iced::widget::{
    button, checkbox, column, container, horizontal_space, row, scrollable, text, text_input,
    toggler,
};
use iced::{Alignment, Element, Length};

use super::theme;
use crate::app::{Message, Orthanc};
use orthanc_lib::models::config::ThemeChoice;

pub fn view(app: &Orthanc) -> Element<'_, Message> {
    let section_title = |title: &str| -> Element<'_, Message> {
        text(title.to_string())
            .size(16)
            .color(theme::TEXT_PRIMARY())
            .into()
    };

    let field_label = |label: &str| -> Element<'_, Message> {
        text(label.to_string())
            .size(12)
            .color(theme::TEXT_MUTED())
            .into()
    };

    // GitHub section
    let github_section = container(
        column![
            section_title("GitHub"),
            column![
                field_label("Personal Access Token"),
                text_input("ghp_...", app.settings.github.pat.as_deref().unwrap_or(""))
                    .on_input(Message::SettingsPATChanged)
                    .size(13)
                    .secure(true),
            ]
            .spacing(4),
            column![
                field_label("API Base URL"),
                text_input("https://api.github.com", &app.settings.github.api_base_url)
                    .on_input(Message::SettingsApiBaseUrlChanged)
                    .size(13),
            ]
            .spacing(4),
            column![
                field_label("Default Organization"),
                text_input(
                    "org-name",
                    app.settings.github.default_org.as_deref().unwrap_or("")
                )
                .on_input(Message::SettingsDefaultOrgChanged)
                .size(13),
            ]
            .spacing(4),
        ]
        .spacing(12),
    )
    .padding(20)
    .style(|_theme| section_style());

    // Defaults section
    let defaults_section = container(
        column![
            section_title("Defaults"),
            row![
                column![
                    field_label("Python Version"),
                    text_input("3.12", &app.settings.defaults.python_version)
                        .on_input(Message::SettingsPythonVersionChanged)
                        .size(13),
                ]
                .spacing(4)
                .width(Length::FillPortion(1)),
                column![
                    field_label("Terminal Command"),
                    text_input(
                        "e.g. wezterm start -- {cmd}",
                        app.settings
                            .general
                            .terminal_command
                            .as_deref()
                            .unwrap_or("")
                    )
                    .on_input(Message::SettingsTerminalCommandChanged)
                    .size(13),
                ]
                .spacing(4)
                .width(Length::FillPortion(1)),
            ]
            .spacing(16),
            column![
                field_label("Default Virtual Environments Directory"),
                text_input(
                    "Leave empty for <repo>/.venv",
                    app.settings
                        .defaults
                        .default_venv_dir
                        .as_deref()
                        .unwrap_or(""),
                )
                .on_input(Message::SettingsDefaultVenvDirChanged)
                .size(13),
            ]
            .spacing(4),
            column![
                field_label("UV Extra Arguments (space-separated)"),
                text_input("e.g. --no-cache --verbose", &app.settings_uv_extra_args,)
                    .on_input(Message::SettingsUvExtraArgsChanged)
                    .size(13),
            ]
            .spacing(4),
            toggler(app.settings.defaults.seed)
                .label("Seed virtual environments (install pip, setuptools, wheel)")
                .on_toggle(Message::SettingsSeedChanged)
                .text_size(13)
                .spacing(10),
        ]
        .spacing(12),
    )
    .padding(20)
    .style(|_theme| section_style());

    // UV Private Index section
    let uv_section = container(
        column![
            section_title("Private Package Index"),
            column![
                field_label("Index URL (leave empty to use public PyPI)"),
                text_input(
                    "https://pypi.example.com/simple",
                    app.settings.uv.default_index.as_deref().unwrap_or(""),
                )
                .on_input(Message::SettingsUvDefaultIndexChanged)
                .size(13),
            ]
            .spacing(4),
            checkbox(
                "Also use public PyPI for packages not in the private index",
                app.settings.uv.use_public_index,
            )
            .on_toggle(Message::SettingsUvUsePublicIndex)
            .text_size(13),
            row![
                column![
                    field_label("Index Name (for env var matching)"),
                    text_input("private", &app.settings.uv.index_name)
                        .on_input(Message::SettingsUvIndexNameChanged)
                        .size(13),
                ]
                .spacing(4)
                .width(Length::FillPortion(1)),
                column![
                    field_label("Username"),
                    text_input(
                        "deploy-user",
                        app.settings.uv.username.as_deref().unwrap_or("")
                    )
                    .on_input(Message::SettingsUvUsernameChanged)
                    .size(13),
                ]
                .spacing(4)
                .width(Length::FillPortion(1)),
            ]
            .spacing(16),
            column![
                field_label("Password / Token"),
                text_input(
                    "token or password",
                    app.settings.uv.password.as_deref().unwrap_or("")
                )
                .on_input(Message::SettingsUvPasswordChanged)
                .size(13)
                .secure(true),
            ]
            .spacing(4),
            text(format!(
                "Sets {} and UV_INDEX_{}_USERNAME/PASSWORD on all uv commands",
                if app.settings.uv.use_public_index {
                    "UV_INDEX"
                } else {
                    "UV_DEFAULT_INDEX"
                },
                app.settings
                    .uv
                    .index_name
                    .to_uppercase()
                    .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
            ))
            .size(11)
            .color(theme::TEXT_MUTED()),
        ]
        .spacing(12),
    )
    .padding(20)
    .style(|_theme| section_style());

    // Appearance section
    let theme_btn =
        |label: &str, choice: ThemeChoice, current: ThemeChoice| -> Element<'_, Message> {
            let is_active = current == choice;
            let accent = theme::ACCENT_LIGHT();
            button(text(label.to_string()).size(13).color(if is_active {
                accent
            } else {
                theme::TEXT_SECONDARY()
            }))
            .on_press(Message::SettingsThemeChanged(choice))
            .padding([8, 20])
            .width(Length::FillPortion(1))
            .style(theme::toggle_button(is_active))
            .into()
        };

    let appearance_section = container(
        column![
            section_title("Appearance"),
            column![
                field_label("Theme"),
                row![
                    theme_btn("Dark", ThemeChoice::Dark, app.settings.general.theme),
                    theme_btn("Light", ThemeChoice::Light, app.settings.general.theme),
                ]
                .spacing(8),
            ]
            .spacing(4),
        ]
        .spacing(12),
    )
    .padding(20)
    .style(|_theme| section_style());

    // Global env vars section
    let global_env_rows: Vec<Element<Message>> = app
        .settings_global_env_var_rows
        .iter()
        .enumerate()
        .map(|(idx, (key, value))| {
            row![
                text_input("Key", key)
                    .on_input(move |v| Message::SettingsGlobalEnvVarKeyChanged(idx, v))
                    .size(13)
                    .width(Length::FillPortion(2)),
                text_input("Value", value)
                    .on_input(move |v| Message::SettingsGlobalEnvVarValueChanged(idx, v))
                    .size(13)
                    .width(Length::FillPortion(3)),
                button(text("x").size(12).color(theme::DANGER()))
                    .on_press(Message::SettingsRemoveGlobalEnvVar(idx))
                    .padding([4, 8])
                    .style(theme::text_button(theme::DANGER(), theme::DANGER(), 4.0)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        })
        .collect();

    let mut global_vars_content = column![section_title("Global Environment Variables")].spacing(8);
    for r in global_env_rows {
        global_vars_content = global_vars_content.push(r);
    }
    global_vars_content = global_vars_content.push(
        button(text("+ Add Variable").size(12).color(theme::ACCENT_LIGHT()))
            .on_press(Message::SettingsAddGlobalEnvVar)
            .padding([6, 12])
            .style(theme::outline_button(6.0)),
    );

    let global_env_section = container(global_vars_content)
        .padding(20)
        .style(|_theme| section_style());

    // Config path info
    let config_path_text = app.config_path.to_string_lossy().to_string();

    // Save button
    let save_bar = row![
        text(format!("Config: {}", config_path_text))
            .size(11)
            .color(theme::TEXT_MUTED()),
        horizontal_space(),
        button(
            text("Save Settings")
                .size(13)
                .color(theme::TEXT_ON_ACCENT())
        )
        .on_press(Message::SaveSettings)
        .padding([8, 24])
        .style(theme::primary_button(6.0)),
    ]
    .spacing(16)
    .align_y(Alignment::Center);

    // Update section
    let update_section = {
        let ver = env!("CARGO_PKG_VERSION");
        let mut col = column![
            section_title("Updates"),
            text(format!("Current version: v{}", ver))
                .size(13)
                .color(theme::TEXT_SECONDARY()),
        ]
        .spacing(8);

        if let Some(ref info) = app.update_available {
            col = col.push(
                container(
                    row![
                        text(format!("v{} available", info.version))
                            .size(13)
                            .color(theme::ACCENT_LIGHT()),
                        horizontal_space(),
                        if app.update_downloading {
                            button(text("Downloading...").size(12).color(theme::TEXT_MUTED()))
                                .padding([6, 16])
                                .style(|_theme, _status| button::Style {
                                    background: Some(iced::Background::Color(theme::BG_MID())),
                                    text_color: theme::TEXT_MUTED(),
                                    border: iced::Border {
                                        radius: 6.0.into(),
                                        ..Default::default()
                                    },
                                    shadow: iced::Shadow::default(),
                                })
                        } else if let Some(ref url) = info.download_url {
                            button(
                                text("Download & Install")
                                    .size(12)
                                    .color(theme::TEXT_ON_ACCENT()),
                            )
                            .on_press(Message::DownloadUpdate(url.clone()))
                            .padding([6, 16])
                            .style(theme::primary_button(6.0))
                        } else {
                            button(
                                text("No binary for this platform")
                                    .size(12)
                                    .color(theme::TEXT_MUTED()),
                            )
                            .padding([6, 16])
                            .style(|_theme, _status| button::Style {
                                background: Some(iced::Background::Color(theme::BG_MID())),
                                text_color: theme::TEXT_MUTED(),
                                border: iced::Border {
                                    radius: 6.0.into(),
                                    ..Default::default()
                                },
                                shadow: iced::Shadow::default(),
                            })
                        },
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .padding(12)
                .style(|_theme| container::Style {
                    background: Some(iced::Background::Color(theme::ACCENT().scale_alpha(0.08))),
                    border: iced::Border {
                        color: theme::ACCENT().scale_alpha(0.3),
                        width: 1.0,
                        radius: 6.0.into(),
                    },
                    ..Default::default()
                }),
            );
        } else {
            col = col.push(
                button(
                    text("Check for Updates")
                        .size(12)
                        .color(theme::ACCENT_LIGHT()),
                )
                .on_press(Message::CheckForUpdate)
                .padding([6, 12])
                .style(theme::outline_button(6.0)),
            );
        }
        container(col).padding(20).style(|_theme| section_style())
    };

    scrollable(
        container(
            column![
                text("Settings").size(20).color(theme::TEXT_PRIMARY()),
                github_section,
                uv_section,
                defaults_section,
                appearance_section,
                global_env_section,
                update_section,
                save_bar,
            ]
            .spacing(20)
            .width(Length::Fill)
            .max_width(700),
        )
        .padding(24),
    )
    .height(Length::Fill)
    .into()
}

fn section_style() -> container::Style {
    container::Style {
        background: Some(iced::Background::Color(theme::BG_DARK())),
        border: iced::Border {
            color: theme::BORDER(),
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}
