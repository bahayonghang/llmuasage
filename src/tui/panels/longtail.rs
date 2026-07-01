//! Shared long-tail collapsing for ranked breakdown tables.
//!
//! Large breakdowns (many models/sources/cost lines) accumulate a long tail of
//! sub-1% rows that push the interesting rows off screen. [`collapse_tail`]
//! folds that trailing tail into a single "+N more" summary row, but only when
//! the list is long enough that collapsing is worthwhile — short lists (and the
//! property-test fixtures) render every row unchanged.

/// Outcome of a long-tail collapse: keep the first `keep` rows and render one
/// summary row describing the folded remainder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Collapsed {
    /// Number of leading rows to render as-is.
    pub keep: usize,
    /// Count of folded trailing rows.
    pub hidden: usize,
    /// Summed metric value of the folded rows.
    pub hidden_value: i64,
    /// Folded rows' combined share of `total`, in `[0, 1]`.
    pub hidden_share: f64,
}

/// Minimum leading rows always kept before any collapsing.
const KEEP_MIN: usize = 8;
/// Trailing rows at or below this share of the total may be folded.
const TAIL_SHARE: f64 = 0.02;

/// Returns a [`Collapsed`] plan when `values` (metric per row) has a foldable
/// tail: more than `KEEP_MIN + 1` rows and at least two trailing rows each at or
/// below [`TAIL_SHARE`] of `total`. Otherwise returns `None` (render all rows).
///
/// `values` is assumed ranked descending, matching the breakdown SQL ordering;
/// only the contiguous small tail is folded.
pub fn collapse_tail(values: &[i64], total: i64) -> Option<Collapsed> {
    if values.len() <= KEEP_MIN + 1 || total <= 0 {
        return None;
    }
    let mut keep = values.len();
    while keep > KEEP_MIN {
        let share = values[keep - 1].max(0) as f64 / total as f64;
        if share <= TAIL_SHARE {
            keep -= 1;
        } else {
            break;
        }
    }
    let hidden = values.len() - keep;
    if hidden < 2 {
        return None;
    }
    let hidden_value: i64 = values[keep..].iter().map(|value| (*value).max(0)).sum();
    Some(Collapsed {
        keep,
        hidden,
        hidden_value,
        hidden_share: hidden_value as f64 / total as f64,
    })
}

/// Human-readable label for a collapsed summary row, e.g. `+5 more · 3%`.
pub fn summary_label(collapsed: &Collapsed) -> String {
    format!(
        "+{} more · {:.0}%",
        collapsed.hidden,
        collapsed.hidden_share * 100.0
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_lists_are_never_collapsed() {
        assert_eq!(collapse_tail(&[10, 5, 1], 16), None);
        // Exactly KEEP_MIN + 1 rows is still short enough to render whole.
        let values: Vec<i64> = (0..9).map(|_| 1).collect();
        assert_eq!(collapse_tail(&values, 9), None);
    }

    #[test]
    fn folds_sub_two_percent_tail() {
        // 8 big rows (100 each) + 4 tiny rows (1 each). Total = 804.
        let mut values = vec![100; 8];
        values.extend([1, 1, 1, 1]);
        let total: i64 = values.iter().sum();
        let collapsed = collapse_tail(&values, total).expect("tail should collapse");
        assert_eq!(collapsed.keep, 8);
        assert_eq!(collapsed.hidden, 4);
        assert_eq!(collapsed.hidden_value, 4);
        assert!(collapsed.hidden_share < 0.02);
    }

    #[test]
    fn keeps_tail_when_rows_are_significant() {
        // 12 evenly-weighted rows: none is <=2%, so nothing folds.
        let values = vec![10; 12];
        assert_eq!(collapse_tail(&values, 120), None);
    }

    #[test]
    fn zero_total_is_ignored() {
        assert_eq!(collapse_tail(&[0; 20], 0), None);
    }
}
