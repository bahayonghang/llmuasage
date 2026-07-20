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

### 5. Good/Base/Bad Cases

- Good: an idle dashboard blocks in `recv` and performs no draw calls for ten
  seconds while auto-refresh timing remains active.
- Good: a 40-row Models table in a 7-row viewport formats seven rows and renders
  the same buffer as the equivalent visible prefix.
- Base: loading/sync progress continues to animate at the existing 250 ms tick.
- Bad: drawing at the top of every loop iteration, which restores permanent 4
  fps work even when no state changed.
- Bad: draining ticks by discarding the first queued key event.
- Bad: appending a collapsed summary row after every visible slice even when its
  absolute row is outside the viewport.

### 6. Tests Required

- Assert the initial frame renders once, 40 idle ticks request zero frames, and
  active animation ticks request frames.
- Assert tick bursts coalesce while preserving following input order.
- Assert a frame snapshot stays stable across a global theme change and existing
  all-theme/no-color buffer tests remain green.
- Assert Models and Cost full-dataset buffers equal their visible-prefix buffers;
  keep Blocks rendering tests and source scans proving every scroll iterator has
  a visible `take` bound.
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
