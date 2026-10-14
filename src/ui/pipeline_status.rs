use iced::widget::{button, column, container, horizontal_space, row, text, Space};
use iced::{Alignment, Element, Length};

use super::theme;
use crate::app::Message;
use orthanc_lib::models::github::{WorkflowConclusion, WorkflowRun, WorkflowStatus};

pub fn view(runs: &[WorkflowRun]) -> Element<'_, Message> {
    if runs.is_empty() {
        return container(
            column![
                text("No pipeline runs found")
                    .size(14)
                    .color(theme::TEXT_MUTED()),
                text("Configure GitHub PAT in Settings to view workflows")
                    .size(12)
                    .color(theme::TEXT_MUTED()),
            ]
            .spacing(4),
        )
        .padding(8)
        .into();
    }

    let header = row![
        text("Pipelines").size(14).color(theme::TEXT_PRIMARY()),
        horizontal_space(),
        button(text("Refresh").size(12).color(theme::TEXT_SECONDARY()))
            .on_press(Message::RefreshPipelines)
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

    let run_elements: Vec<Element<Message>> = runs
        .iter()
        .take(10)
        .map(|run| {
            let status_color = match (&run.status, &run.conclusion) {
                (_, Some(WorkflowConclusion::Success)) => theme::SUCCESS(),
                (_, Some(WorkflowConclusion::Failure)) => theme::DANGER(),
                (_, Some(WorkflowConclusion::Cancelled)) => theme::TEXT_MUTED(),
                (_, Some(WorkflowConclusion::TimedOut)) => theme::WARNING(),
                (_, Some(WorkflowConclusion::Skipped)) => theme::TEXT_MUTED(),
                (WorkflowStatus::InProgress, _) => theme::INFO(),
                (WorkflowStatus::Queued, _) => theme::WARNING(),
                _ => theme::TEXT_SECONDARY(),
            };

            let status_text = match (&run.status, &run.conclusion) {
                (_, Some(c)) => format!("{:?}", c).to_lowercase(),
                (s, None) => format!("{:?}", s).to_lowercase(),
            };

            let run_id = run.id;
            let name = run.name.clone();
            let branch = run.head_branch.clone();
            let time = run.updated_at.format("%Y-%m-%d %H:%M").to_string();

            container(
                row![
                    container(Space::new(0, 0))
                        .width(8)
                        .height(8)
                        .style(move |_theme| container::Style {
                            background: Some(iced::Background::Color(status_color)),
                            border: iced::Border {
                                radius: 4.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                    column![
                        text(name).size(13).color(theme::TEXT_PRIMARY()),
                        row![
                            text(branch).size(11).color(theme::TEXT_MUTED()),
                            text(" | ").size(11).color(theme::TEXT_MUTED()),
                            text(status_text).size(11).color(status_color),
                            text(" | ").size(11).color(theme::TEXT_MUTED()),
                            text(time).size(11).color(theme::TEXT_MUTED()),
                        ]
                        .spacing(2),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    button(text("Rerun").size(11).color(theme::TEXT_SECONDARY()))
                        .on_press(Message::RerunWorkflow(run_id))
                        .padding([4, 8])
                        .style(theme::secondary_button(4.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([8, 12])
            .style(|_theme| container::Style {
                background: Some(iced::Background::Color(theme::BG_DARKEST())),
                border: iced::Border {
                    color: theme::BORDER(),
                    width: 1.0,
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .into()
        })
        .collect();

    let mut content = column![header].spacing(6);
    for el in run_elements {
        content = content.push(el);
    }

    content.into()
}
