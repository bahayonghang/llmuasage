# TUI Runtime Contracts

## Scenario: Efficient event-driven rendering

### 1. Scope / Trigger

- Apply this contract when changing `src/tui/{mod,event,draw,theme}.rs`, panel
  loading/sync animation state, or a scrollable table renderer.
- Query payload semantics remain governed by `dashboard-performance-contracts.md`;
  this contract covers when and how an accepted state is rendered.

### 2. Signatures

```text
RedrawState::{initial, request, take}()
AppState::on_tick(animation_active: bool) -> bool
ScrollState::{scroll_up, scroll_down, page_up, page_down,
              select_first, select_last, set_total, visible_range}()
SortState::{header, apply}(...)
EventHandler::recv() -> Result<TuiEvent>
theme::with_render_snapshot(|| render_frame())
models|cost::render_with_plan(..., collapse: Option<Collapsed>)
```

### 3. Contracts

- The initial frame is dirty. Keyboard/mouse input, resize, dialog actions,
  accepted panel results, sync updates, refresh transitions, and theme changes
  request another frame.
- A pure 250 ms `Tick` does not request a frame. Loading or sync activity keeps
  animation ticks dirty; auto-refresh checks still run on every tick.
- Panel and sync result channels are drained before the tick redraw decision.
  An accepted result therefore becomes visible on the next frame without a
  blocking query on the render thread.
- Consecutive queued ticks collapse to one notification. The first following
  key, mouse, or resize event is retained in order and must never be dropped.
- `draw::draw` enters one frame-scoped `ThemeState` snapshot. Semantic theme
  accessors use that thread-local snapshot during the frame, so the global
  `RwLock` is read once per frame and a mid-frame theme change applies next frame.
- Scrollable bordered tables build only `area.height - 4` visible rows after the current
  offset. Models and Cost compute their long-tail collapse plans when a matching
  data generation is accepted, reuse the plan while scrolling, and invalidate
  it whenever the payload is invalidated.
- Windowing and memoization are internal only: the same payload, scroll offset,
  terminal size, and theme must produce the same `TestBackend` cells.
- Models, Daily, Hourly, Cost, Blocks, and the Stats source table use one
  `ScrollState` for selection and windowing. Single-row movement wraps; paging
  and Home/End clamp; every rendered selected row uses `theme::selection_style()`.
- Models, Daily, Cost, and Blocks keep independent `SortState` values. `o`
  cycles the panel's supported columns, `O` reverses direction, stable in-memory
  sorting preserves ties and the row collection, and the active header shows an
  arrow. An unsorted Models/Cost view uses its collapsed row count; a sorted view
  uses the raw payload length and disables long-tail collapse.
- Mouse wheel events map to the same row movement actions as the keyboard.
  Footer spinner frames are fixed-width ASCII and render only while a panel load
  or sync is active.

### 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| 40 idle ticks after initial frame | 0 draw requests |
| loading or sync active on tick | Advance animation and request a frame |
| panel/sync result received | Mutate state and request a frame |
| three ticks followed by a key | Return one tick, then the key |
| theme changes inside a frame | Current frame stays on its snapshot |
| Models/Cost generation changes | Recompute collapse plan once on acceptance |
| scroll offset changes | Reuse collapse plan and format visible rows only |
| payload/filter/window invalidated | Clear payload and its derived plan |
| row movement at first/last item | Wrap for single-row movement; never leave bounds |
| page movement past either edge | Clamp at first/last item and keep it visible |
| sort direction changes | Reorder the loaded references only; do not query or mutate payload |
| Models/Cost sort becomes active | Use raw length and suppress the ranked-order collapse plan |
| no panel load or sync active | Render no spinner and keep idle ticks clean |

### 5. Good/Base/Bad Cases

- Good: an idle dashboard blocks in `recv` and performs no draw calls for ten
  seconds while auto-refresh timing remains active.
- Good: a 40-row Models table in a 7-row viewport formats seven rows and renders
  the same buffer as the equivalent visible prefix.
- Good: sorting Daily by tokens updates the arrow and detail strip to follow the
  selected row while leaving the loaded `Vec<DailyTrendPoint>` unchanged.
- Base: loading/sync progress continues to animate at the existing 250 ms tick.
- Bad: drawing at the top of every loop iteration, which restores permanent 4
  fps work even when no state changed.
- Bad: draining ticks by discarding the first queued key event.
- Bad: appending a collapsed summary row after every visible slice even when its
  absolute row is outside the viewport.
- Bad: using `row_highlight_style` without `TableState`, or maintaining a second
  panel-local offset that can diverge from `ScrollState`.

### 6. Tests Required

- Assert the initial frame renders once, 40 idle ticks request zero frames, and
  active animation ticks request frames.
- Assert tick bursts coalesce while preserving following input order.
- Assert a frame snapshot stays stable across a global theme change and existing
  all-theme/no-color buffer tests remain green.
- Assert Models and Cost full-dataset buffers equal their visible-prefix buffers;
  keep Blocks rendering tests and source scans proving every scroll iterator has
  a visible `take` bound.
- Property-test selection bounds, wrap, paging, and selected-row visibility.
  Assert stable ascending/descending sorting preserves ties and the collection,
  and render-test an arrow plus selection-dependent detail. Assert wheel mapping
  and spinner active/idle frames through `TestBackend`.
- Source-scan every selectable live table for `visible_range` and
  `selection_style`; removed TUI-only panel modules must not remain exported or
  referenced. Query APIs used by web/snapshots remain outside this cleanup.
- Run `cargo fmt --all -- --check`, strict clippy, and serial full tests.

### 7. Wrong vs Correct

#### Wrong

```rust
loop {
    terminal.draw(|frame| draw(frame, state))?;
    handle(events.recv()?);
}
```

#### Correct

```rust
loop {
    if redraw.take() {
        terminal.draw(|frame| draw(frame, state))?;
    }
    handle_and_request_redraw_when_state_changes(events.recv()?);
}
```

The correct form makes state transitions own redraw requests while pure ticks
remain scheduling notifications rather than frames.

For selectable tables, styling must be applied to the manually windowed row:

```rust
// Wrong: no TableState is passed, so this never highlights a row.
Table::new(rows, widths).row_highlight_style(theme::selection_style())

// Correct: absolute indexes stay aligned with ScrollState and sorted references.
if absolute == scroll.selected {
    row.style(theme::selection_style())
} else {
    row
}
```
