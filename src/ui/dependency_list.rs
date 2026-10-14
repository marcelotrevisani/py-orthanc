use iced::widget::{
    button, checkbox, column, container, horizontal_space, row, scrollable, text, text_input,
};
use iced::{Alignment, Element, Length};

use super::theme;
use crate::app::{DepInfo, DepSource, InstallForm, InstallSource, Message, Orthanc};

pub fn view(app: &Orthanc) -> Element<'_, Message> {
    let deps = &app.dependencies;
    let editing_index = app.dep_editing_index;
    let editing_version = app.dep_editing_version.as_str();
    if app.loading_dependencies {
        return container(
            text("Loading dependencies...")
                .size(13)
                .color(theme::TEXT_MUTED()),
        )
        .padding(8)
        .into();
    }

    // If we have dep info to show, render the detail panel
    if let Some(info) = &app.dep_info {
        return view_dep_info(info);
    }

    if app.loading_dep_info {
        return container(
            text("Loading package info...")
                .size(13)
                .color(theme::TEXT_MUTED()),
        )
        .padding(8)
        .into();
    }

    if deps.is_empty() {
        let install_row = container(install_form(&app.dep_install)).padding(iced::Padding {
            top: 12.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        });

        return container(
            column![
                text("No dependencies found")
                    .size(14)
                    .color(theme::TEXT_MUTED()),
                text("Click Refresh to list installed packages, or install a new one below")
                    .size(12)
                    .color(theme::TEXT_MUTED()),
                install_row,
            ]
            .spacing(4),
        )
        .padding(8)
        .into();
    }

    let header = row![
        text(format!("{} packages", deps.len()))
            .size(14)
            .color(theme::TEXT_PRIMARY()),
        horizontal_space(),
        button(text("Refresh").size(12).color(theme::TEXT_SECONDARY()))
            .on_press(Message::ListDependencies)
            .padding([4, 10])
            .style(theme::secondary_button(4.0)),
    ]
    .align_y(Alignment::Center)
    .padding(iced::Padding {
        top: 0.0,
        right: 0.0,
        bottom: 8.0,
        left: 0.0,
    });

    let table_header = container(
        row![
            text("Package")
                .size(12)
                .color(theme::TEXT_MUTED())
                .width(Length::FillPortion(4)),
            text("Version")
                .size(12)
                .color(theme::TEXT_MUTED())
                .width(Length::FillPortion(3)),
            text("Source")
                .size(12)
                .color(theme::TEXT_MUTED())
                .width(Length::FillPortion(2)),
            text("").size(12).width(Length::FillPortion(1)),
        ]
        .spacing(8),
    )
    .padding(iced::Padding {
        top: 0.0,
        right: 0.0,
        bottom: 6.0,
        left: 0.0,
    })
    .style(|_theme| container::Style {
        border: iced::Border {
            color: theme::BORDER(),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    });

    let dep_rows: Vec<Element<Message>> = deps
        .iter()
        .enumerate()
        .map(|(idx, dep)| {
            let (source_label, source_color) = match &dep.source {
                DepSource::Direct => ("direct", theme::ACCENT_LIGHT()),
                DepSource::Dev => ("dev", theme::WARNING()),
                DepSource::Transitive => ("transitive", theme::TEXT_MUTED()),
                DepSource::Editable => ("editable", theme::SUCCESS()),
            };

            let is_editing = editing_index == Some(idx);

            // Package name — clickable (double-click opens info)
            let name_cell: Element<Message> =
                button(text(dep.name.clone()).size(13).color(theme::TEXT_PRIMARY()))
                    .on_press(Message::DepClick(idx))
                    .padding([2, 4])
                    .width(Length::FillPortion(4))
                    .style(theme::ghost_button(theme::TEXT_PRIMARY(), 3.0))
                    .into();

            let version_cell: Element<Message> = if is_editing {
                text_input("version", editing_version)
                    .on_input(move |v| Message::DepVersionEditing(idx, v))
                    .on_submit(Message::DepVersionSubmit(idx))
                    .size(13)
                    .width(Length::FillPortion(3))
                    .into()
            } else {
                let ver = dep.version.clone();
                button(
                    text(dep.version.clone())
                        .size(13)
                        .color(theme::TEXT_SECONDARY()),
                )
                .on_press(Message::DepVersionEditing(idx, ver))
                .padding([2, 4])
                .width(Length::FillPortion(3))
                .style(theme::ghost_button(theme::TEXT_SECONDARY(), 3.0))
                .into()
            };

            let delete_btn: Element<Message> = button(text("x").size(11).color(theme::DANGER()))
                .on_press(Message::DepUninstall(idx))
                .padding([2, 6])
                .width(Length::FillPortion(1))
                .style(theme::text_button(theme::DANGER(), theme::DANGER(), 3.0))
                .into();

            container(
                row![
                    name_cell,
                    version_cell,
                    container(text(source_label.to_string()).size(11).color(source_color),)
                        .width(Length::FillPortion(2)),
                    delete_btn,
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .padding([4, 0])
            .into()
        })
        .collect();

    let mut list = column![].spacing(0);
    for r in dep_rows {
        list = list.push(r);
    }

    let count_direct = deps
        .iter()
        .filter(|d| d.source == DepSource::Direct)
        .count();
    let count_dev = deps.iter().filter(|d| d.source == DepSource::Dev).count();
    let count_transitive = deps
        .iter()
        .filter(|d| d.source == DepSource::Transitive)
        .count();
    let count_editable = deps
        .iter()
        .filter(|d| d.source == DepSource::Editable)
        .count();

    let mut summary_parts = Vec::new();
    if count_direct > 0 {
        summary_parts.push(format!("{} direct", count_direct));
    }
    if count_dev > 0 {
        summary_parts.push(format!("{} dev", count_dev));
    }
    if count_transitive > 0 {
        summary_parts.push(format!("{} transitive", count_transitive));
    }
    if count_editable > 0 {
        summary_parts.push(format!("{} editable", count_editable));
    }

    let summary = text(format!(
        "{} packages ({})",
        deps.len(),
        summary_parts.join(", ")
    ))
    .size(11)
    .color(theme::TEXT_MUTED());

    let install_row = install_form(&app.dep_install);

    let hint = text("Double-click a package name to see its dependency tree")
        .size(11)
        .color(theme::TEXT_MUTED());

    column![
        header,
        summary,
        install_row,
        table_header,
        scrollable(list).height(Length::Fill),
        hint,
    ]
    .spacing(4)
    .into()
}

/// Source switch (PyPI / Local folder / GitHub) plus the fields for that source.
fn install_form(form: &InstallForm) -> Element<'_, Message> {
    let tab = |label: &'static str, source: InstallSource| {
        let active = form.source == source;
        button(text(label).size(12).color(if active {
            theme::TEXT_ON_ACCENT()
        } else {
            theme::TEXT_SECONDARY()
        }))
        .on_press(Message::DepInstallSourceChanged(source))
        .padding([4, 10])
        .style(move |_theme, status| {
            let bg = if active {
                theme::ACCENT()
            } else if matches!(status, button::Status::Hovered) {
                theme::BG_MID()
            } else {
                iced::Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced::Background::Color(bg)),
                text_color: theme::TEXT_SECONDARY(),
                border: iced::Border {
                    color: theme::BORDER(),
                    width: 1.0,
                    radius: 4.0.into(),
                },
                shadow: iced::Shadow::default(),
            }
        })
    };
    let source_tabs = row![
        tab("PyPI", InstallSource::PyPi),
        tab("Local folder", InstallSource::Local),
        tab("GitHub", InstallSource::GitHub),
    ]
    .spacing(4);

    let fields: Element<Message> = match form.source {
        InstallSource::PyPi => row![
            text_input("Package name (e.g. requests or requests==2.31)", &form.name)
                .on_input(Message::DepInstallNameChanged)
                .on_submit(Message::DepInstallSubmit)
                .size(13)
                .width(Length::Fill),
            install_button(),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        InstallSource::Local => row![
            text_input("Path to a Python project folder", &form.path)
                .on_input(Message::DepInstallPathChanged)
                .on_submit(Message::DepInstallSubmit)
                .size(13)
                .width(Length::Fill),
            button(text("Browse…").size(12).color(theme::TEXT_SECONDARY()))
                .on_press(Message::DepInstallPickPath)
                .padding([6, 10])
                .style(theme::secondary_button(4.0)),
            checkbox("Editable (dev mode)", form.editable)
                .on_toggle(Message::DepInstallEditableToggled)
                .size(14)
                .text_size(12),
            install_button(),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        InstallSource::GitHub => row![
            text_input("org/repo or https://github.com/org/repo", &form.git_url)
                .on_input(Message::DepInstallGitUrlChanged)
                .on_submit(Message::DepInstallSubmit)
                .size(13)
                .width(Length::FillPortion(3)),
            text_input("branch / tag / commit (optional)", &form.git_ref)
                .on_input(Message::DepInstallGitRefChanged)
                .on_submit(Message::DepInstallSubmit)
                .size(13)
                .width(Length::FillPortion(2)),
            install_button(),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
    };

    column![source_tabs, fields].spacing(6).into()
}

fn install_button<'a>() -> Element<'a, Message> {
    button(text("Install").size(12).color(theme::TEXT_ON_ACCENT()))
        .on_press(Message::DepInstallSubmit)
        .padding([6, 14])
        .style(theme::primary_button(4.0))
        .into()
}

fn view_dep_info<'a>(info: &DepInfo) -> Element<'a, Message> {
    let back_btn = button(
        text("<  Back to list")
            .size(12)
            .color(theme::ACCENT_LIGHT()),
    )
    .on_press(Message::DepCloseInfo)
    .padding([4, 8])
    .style(theme::text_button(
        theme::ACCENT_LIGHT(),
        theme::ACCENT(),
        4.0,
    ));

    let title = row![
        text(info.name.clone())
            .size(18)
            .color(theme::TEXT_PRIMARY()),
        text(info.version.clone())
            .size(14)
            .color(theme::TEXT_SECONDARY()),
    ]
    .spacing(8)
    .align_y(Alignment::End);

    let field_label = |label: &str| -> Element<'a, Message> {
        text(label.to_string())
            .size(12)
            .color(theme::TEXT_MUTED())
            .into()
    };

    let mut details = column![].spacing(8);

    if !info.summary.is_empty() {
        details = details.push(
            column![
                field_label("Summary"),
                text(info.summary.clone())
                    .size(13)
                    .color(theme::TEXT_PRIMARY()),
            ]
            .spacing(2),
        );
    }

    if !info.location.is_empty() {
        details = details.push(
            column![
                field_label("Location"),
                text(info.location.clone())
                    .size(12)
                    .color(theme::TEXT_SECONDARY()),
            ]
            .spacing(2),
        );
    }

    // Requires section (dependency tree - what this package depends on)
    let requires_section = if info.requires.is_empty() {
        column![
            field_label("Depends on"),
            text("No dependencies").size(13).color(theme::TEXT_MUTED()),
        ]
        .spacing(2)
    } else {
        let mut col = column![field_label(&format!(
            "Depends on ({} packages)",
            info.requires.len()
        )),]
        .spacing(4);
        for req in &info.requires {
            col = col.push(
                container(
                    row![
                        text("  ").size(13),
                        text(req.clone()).size(13).color(theme::ACCENT_LIGHT()),
                    ]
                    .spacing(4),
                )
                .padding([2, 0]),
            );
        }
        col
    };

    // Required-by section (reverse tree - what depends on this package)
    let required_by_section = if info.required_by.is_empty() {
        column![
            field_label("Required by"),
            text("No packages depend on this")
                .size(13)
                .color(theme::TEXT_MUTED()),
        ]
        .spacing(2)
    } else {
        let mut col = column![field_label(&format!(
            "Required by ({} packages)",
            info.required_by.len()
        )),]
        .spacing(4);
        for dep in &info.required_by {
            col = col.push(
                container(
                    row![
                        text("  ").size(13),
                        text(dep.clone()).size(13).color(theme::SUCCESS()),
                    ]
                    .spacing(4),
                )
                .padding([2, 0]),
            );
        }
        col
    };

    let content =
        container(column![title, details, requires_section, required_by_section,].spacing(16))
            .padding(16)
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(theme::BG_DARK())),
                border: iced::Border {
                    color: theme::BORDER(),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            });

    scrollable(column![back_btn, content,].spacing(8))
        .height(Length::Fill)
        .into()
}
