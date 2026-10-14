//! Git branch / working-tree status: a compact badge for the environment row
//! and a fuller section for the Info tab.

use iced::widget::{column, row, text};
use iced::{Alignment, Element};

use orthanc_lib::services::git::GitStatus;

use super::{icons, theme};
use crate::app::Message;

/// One-line badge, e.g. `⎇ main  ● 3 changed  ↑2 ↓1` or `⎇ main  ✓ clean`.
pub fn badge(status: &GitStatus) -> Element<'_, Message> {
    let mut line = row![
        icons::branch(12.0, theme::TEXT_MUTED()),
        text(status.branch.clone())
            .size(11)
            .color(theme::TEXT_SECONDARY()),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    line = if status.is_clean() {
        line.push(text("✓ clean").size(11).color(theme::SUCCESS()))
    } else {
        line.push(
            text(format!("● {} changed", status.changed()))
                .size(11)
                .color(theme::WARNING()),
        )
    };
    if let Some(sync) = ahead_behind(status) {
        line = line.push(text(sync).size(11).color(theme::INFO()));
    }
    line.spacing(8).into()
}

/// "Git" section for the Info tab.
pub fn details(status: &GitStatus) -> Element<'_, Message> {
    let branch = if status.detached {
        format!("detached at {}", status.branch)
    } else {
        match &status.upstream {
            Some(up) => format!("{} → {}", status.branch, up),
            None => format!("{} (no upstream)", status.branch),
        }
    };

    let changes = if status.is_clean() {
        "Working tree clean".to_string()
    } else {
        format!(
            "{} staged · {} modified · {} untracked · {} conflicts",
            status.staged, status.modified, status.untracked, status.conflicted
        )
    };
    let changes_color = if status.conflicted > 0 {
        theme::DANGER()
    } else if status.is_clean() {
        theme::SUCCESS()
    } else {
        theme::WARNING()
    };

    let mut col = column![
        text("Git").size(12).color(theme::TEXT_MUTED()),
        row![
            icons::branch(13.0, theme::TEXT_SECONDARY()),
            text(branch).size(13).color(theme::TEXT_PRIMARY()),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
        text(changes).size(13).color(changes_color),
    ]
    .spacing(2);

    if let Some(up) = &status.upstream {
        let sync = if status.ahead == 0 && status.behind == 0 {
            format!("Up to date with {} (as of last fetch)", up)
        } else {
            format!(
                "{} ahead, {} behind {} (as of last fetch)",
                status.ahead, status.behind, up
            )
        };
        col = col.push(text(sync).size(12).color(theme::TEXT_SECONDARY()));
    }
    col.into()
}

/// `↑2 ↓1`, or `None` when in sync.
fn ahead_behind(status: &GitStatus) -> Option<String> {
    let parts: Vec<String> = [("↑", status.ahead), ("↓", status.behind)]
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(arrow, n)| format!("{}{}", arrow, n))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ahead_behind_only_shows_nonzero() {
        let mut s = GitStatus::default();
        assert_eq!(ahead_behind(&s), None);
        s.ahead = 2;
        assert_eq!(ahead_behind(&s).as_deref(), Some("↑2"));
        s.behind = 1;
        assert_eq!(ahead_behind(&s).as_deref(), Some("↑2 ↓1"));
        s.ahead = 0;
        assert_eq!(ahead_behind(&s).as_deref(), Some("↓1"));
    }
}
