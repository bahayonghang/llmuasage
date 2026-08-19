pub mod behavior;
pub mod blocks;
pub mod daily;
pub mod hourly;
pub mod models;
pub mod monthly;
pub mod overview;
mod period;
pub mod stats;
pub mod sync_status;
pub mod usage;

pub(crate) fn visible_table_rows(area: ratatui::layout::Rect) -> usize {
    // Outer borders consume two rows; the header plus its bottom margin consume two more.
    area.height.saturating_sub(4).max(1) as usize
}
