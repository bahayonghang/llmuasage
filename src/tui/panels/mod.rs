pub mod behavior;
pub mod blocks;
pub mod cost;
pub mod daily;
pub mod health;
pub mod hourly;
pub mod longtail;
pub mod models;
pub mod overview;
pub mod projects;
pub mod sources;
pub mod stats;
pub mod trends;
pub mod usage;

pub(crate) fn visible_table_rows(area: ratatui::layout::Rect) -> usize {
    // Outer borders consume two rows; the header plus its bottom margin consume two more.
    area.height.saturating_sub(4).max(1) as usize
}
