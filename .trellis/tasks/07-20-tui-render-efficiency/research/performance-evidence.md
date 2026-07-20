# Render efficiency evidence

## Redraw policy

- Baseline source inspection: the event loop drew before every event. At the
  fixed 250 ms tick rate that is 40 full draw calls in a 10 second idle window.
- After: `idle_ticks_produce_zero_draws_after_the_initial_frame` consumes the
  initial dirty frame, simulates 40 idle ticks, and observes 0 draw requests.
- Animation control: the same test observes 4 draw requests for 4 explicitly
  active animation ticks. `AppState::on_tick` advances the spinner only while a
  panel load or sync job is active; auto-refresh timing remains tick-driven.

## Event backlog

- `coalesces_ticks_without_dropping_following_input` queues three ticks, a key,
  and another tick. The receiver returns one tick, the key, then the later tick.
  Tick backlog is bounded by coalescing while user input order is preserved.

## Theme access

- `draw::draw` wraps the complete nav/panel/footer/dialog render in
  `theme::with_render_snapshot`.
- The snapshot performs one `RwLock` read. All `active_theme` and `color_mode`
  calls inside that frame resolve from the thread-local snapshot.
- `render_snapshot_is_stable_until_the_frame_finishes` changes the global theme
  inside a snapshot and proves the current frame retains its old palette while
  the next access observes the new theme.

## Row construction

- Every scrollable table iterator now applies a visible-height `take` after its
  offset. Models and Cost additionally cache long-tail collapse plans when the
  matching data generation is accepted, so scrolling does not rescan the full
  payload for the derived plan.
- Models/Cost equivalence tests use 40 rows and a 7-row viewport. Rendering the
  full dataset produces the same `TestBackend` buffer as rendering its visible
  prefix, while only seven rows are constructed by the production path.
- Existing Blocks render tests remain green after its visible bound was added.

## Validation

- `cargo test tui:: --lib -- --test-threads=1`: 59 passed, 1 ignored.
- `cargo test --test tui_panels_prop -- --test-threads=1`: 27 passed.
- `cargo clippy --lib --tests --all-features -- -D warnings`: passed.
- Full serial library suite: 358 run, 356 passed, 2 ignored; all integration
  and doc tests passed.

No manual TTY CPU profile was run. The accepted A1/R6 path uses the injected
draw counter and deterministic cell-level buffer evidence rather than treating
missing manual evidence as a pass.
