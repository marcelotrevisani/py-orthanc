use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, text, text_input,
};
use iced::{Alignment, Element, Length};

use super::theme;
use crate::app::Message;
use orthanc_lib::models::github::{PRState, PullRequest};

pub fn view<'a>(
    pull_requests: &'a [PullRequest],
    loading: bool,
    filter_state: &'a str,
    filter_author: &'a str,
) -> Element<'a, Message> {
    // Header
    let header = row![
        text("Pull Requests").size(18).color(theme::TEXT_PRIMARY()),
        horizontal_space(),
        button(text("Refresh").size(12).color(theme::TEXT_SECONDARY()))
            .on_press(Message::LoadOrgPullRequests)
            .padding([6, 14])
            .style(theme::secondary_button(6.0)),
    ]
    .align_y(Alignment::Center);

    // Filters
    let state_btn = |label: &str, value: &str, current: &str| -> Element<'_, Message> {
        let is_active = current == value;
        let value = value.to_string();
        let accent = theme::ACCENT_LIGHT();
        button(text(label.to_string()).size(12).color(if is_active {
            accent
        } else {
            theme::TEXT_SECONDARY()
        }))
        .on_press(Message::PrFilterStateChanged(value))
        .padding([6, 14])
        .style(theme::toggle_button(is_active))
        .into()
    };

    let filters = row![
        state_btn("Open", "open", filter_state),
        state_btn("Closed", "closed", filter_state),
        state_btn("All", "all", filter_state),
        container(
            text_input("Filter by author...", filter_author)
                .on_input(Message::PrFilterAuthorChanged)
                .on_submit(Message::LoadOrgPullRequests)
                .size(12)
                .width(200),
        )
        .padding([0, 8]),
        button(text("Search").size(12).color(theme::TEXT_ON_ACCENT()))
            .on_press(Message::LoadOrgPullRequests)
            .padding([6, 14])
            .style(theme::primary_button(6.0)),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    // Content
    let content: Element<Message> = if loading {
        container(
            text("Loading pull requests...")
                .size(14)
                .color(theme::TEXT_MUTED()),
        )
        .padding(20)
        .into()
    } else if pull_requests.is_empty() {
        container(
            column![
                text("No pull requests found")
                    .size(14)
                    .color(theme::TEXT_MUTED()),
                text("Configure GitHub PAT and default organization in Settings")
                    .size(12)
                    .color(theme::TEXT_MUTED()),
            ]
            .spacing(4),
        )
        .padding(20)
        .into()
    } else {
        // Table header
        let table_header = container(
            row![
                text("PR").size(11).color(theme::TEXT_MUTED()).width(60),
                text("Title")
                    .size(11)
                    .color(theme::TEXT_MUTED())
                    .width(Length::FillPortion(4)),
                text("Repository")
                    .size(11)
                    .color(theme::TEXT_MUTED())
                    .width(Length::FillPortion(2)),
                text("Author")
                    .size(11)
                    .color(theme::TEXT_MUTED())
                    .width(Length::FillPortion(1)),
                text("State").size(11).color(theme::TEXT_MUTED()).width(80),
                text("Created")
                    .size(11)
                    .color(theme::TEXT_MUTED())
                    .width(110),
            ]
            .spacing(12)
            .padding([0, 16]),
        )
        .padding(iced::Padding {
            top: 0.0,
            right: 0.0,
            bottom: 8.0,
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

        let pr_rows: Vec<Element<Message>> = pull_requests
            .iter()
            .map(|pr| {
                let (state_label, state_color) = match &pr.state {
                    PRState::Open => ("open", theme::SUCCESS()),
                    PRState::Closed => ("closed", theme::DANGER()),
                    PRState::Merged => ("merged", theme::ACCENT_LIGHT()),
                };

                let url = pr.html_url.clone();

                let pr_row = button(
                    row![
                        text(format!("#{}", pr.number))
                            .size(12)
                            .color(theme::TEXT_MUTED())
                            .width(60),
                        text(pr.title.clone())
                            .size(13)
                            .color(theme::TEXT_PRIMARY())
                            .width(Length::FillPortion(4)),
                        text(pr.repo_full_name.clone())
                            .size(12)
                            .color(theme::TEXT_SECONDARY())
                            .width(Length::FillPortion(2)),
                        text(pr.author.clone())
                            .size(12)
                            .color(theme::ACCENT_LIGHT())
                            .width(Length::FillPortion(1)),
                        container(text(state_label.to_string()).size(11).color(state_color),)
                            .width(80),
                        text(pr.created_at.format("%Y-%m-%d").to_string())
                            .size(12)
                            .color(theme::TEXT_MUTED())
                            .width(110),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::OpenUrl(url))
                .padding([10, 16])
                .width(Length::Fill)
                .style(|_theme, status| {
                    let bg = if matches!(status, button::Status::Hovered) {
                        theme::BG_MID()
                    } else {
                        theme::BG_DARKEST()
                    };
                    button::Style {
                        background: Some(iced::Background::Color(bg)),
                        text_color: theme::TEXT_PRIMARY(),
                        border: iced::Border {
                            color: theme::BORDER(),
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        shadow: iced::Shadow::default(),
                    }
                });

                pr_row.into()
            })
            .collect();

        let mut list = column![].spacing(4);
        for r in pr_rows {
            list = list.push(r);
        }

        column![table_header, scrollable(list).height(Length::Fill),]
            .spacing(4)
            .into()
    };

    column![header, filters, content,]
        .spacing(16)
        .padding(24)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
