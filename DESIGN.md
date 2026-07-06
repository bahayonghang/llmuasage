---
name: llmusage
description: Local-first AI CLI usage analytics, rendered as a warm, monospace-data dashboard that never leaves your machine.
colors:
  terracotta: "#c8553d"
  terracotta-soft: "#f3dfd8"
  terracotta-deep: "#9a3e2b"
  sage: "#4f7a5c"
  sage-soft: "#dde8de"
  ochre: "#c08a3b"
  brick: "#b84a3a"
  warm-paper: "#f6f3ee"
  surface: "#ffffff"
  surface-raised: "#faf7f2"
  ink: "#1c1a17"
  ink-2: "#3a352f"
  ink-strong: "#000000"
  muted: "#736b60"
  muted-2: "#b8b0a4"
  line: "#e8e2d6"
  line-2: "#ece6db"
  instrument-ink: "#1a1815"
  instrument-surface: "#25221e"
  instrument-line: "#34302a"
  instrument-text: "#f0ebe1"
  instrument-muted: "#8d867a"
typography:
  display:
    fontFamily: "system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif"
    fontSize: "38px"
    fontWeight: 600
    lineHeight: 1.05
    letterSpacing: "-0.025em"
  headline:
    fontFamily: "system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif"
    fontSize: "24px"
    fontWeight: 600
    letterSpacing: "-0.018em"
  title:
    fontFamily: "system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif"
    fontSize: "18px"
    fontWeight: 600
    letterSpacing: "-0.018em"
  body:
    fontFamily: "system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.55
  label:
    fontFamily: "ui-monospace, SFMono-Regular, Consolas, 'Liberation Mono', Menlo, monospace"
    fontSize: "10.5px"
    fontWeight: 600
    letterSpacing: "0.14em"
  mono:
    fontFamily: "ui-monospace, SFMono-Regular, Consolas, 'Liberation Mono', Menlo, monospace"
    fontSize: "32px"
    fontWeight: 500
    lineHeight: 1
    letterSpacing: "-0.025em"
    fontFeature: "tnum"
rounded:
  xs: "5px"
  sm: "8px"
  md: "10px"
  lg: "12px"
  xl: "14px"
  feature: "16px"
  hero: "18px"
  pill: "999px"
spacing:
  xs: "6px"
  sm: "10px"
  md: "14px"
  lg: "18px"
  xl: "24px"
  section: "56px"
components:
  button:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink-2}"
    rounded: "{rounded.sm}"
    padding: "7px 12px"
  button-hover:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
  button-primary:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.warm-paper}"
    rounded: "{rounded.sm}"
    padding: "7px 12px"
  button-primary-hover:
    backgroundColor: "{colors.ink-strong}"
  input:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink-2}"
    rounded: "{rounded.sm}"
    padding: "7px 10px"
  tag:
    backgroundColor: "{colors.surface-raised}"
    textColor: "{colors.ink-2}"
    rounded: "{rounded.xs}"
    padding: "3px 8px"
  tag-local:
    backgroundColor: "{colors.terracotta-soft}"
    textColor: "{colors.terracotta-deep}"
  panel:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.xl}"
    padding: "22px 24px"
  kpi-card:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.xl}"
    padding: "18px 20px"
  nav-item-active:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.warm-paper}"
    rounded: "{rounded.sm}"
  instrument-card:
    backgroundColor: "{colors.instrument-ink}"
    textColor: "{colors.instrument-text}"
    rounded: "{rounded.feature}"
    padding: "26px 28px"
---

# Design System: llmusage

## 1. Overview

**Creative North Star: "The Local Instrument"**

llmusage looks like a precision meter that lives on your machine. The page is warm paper; the data lives on dark, calibrated readout panels set into it; every number is monospace. The split is the whole idea: the warm light surface is the housing you read in, and the dark instrument cards (trends, run status, the sync command center) are the lit display where the measurements actually appear. Depth comes from that contrast, not from drop shadows. Nothing glows, nothing bounces, nothing shouts a vanity metric at you.

The voice is the one set in PRODUCT.md: trustworthy, precise, calm. The interface earns trust by being legible and forthright. Costs are labeled estimates, low-sample sources degrade visibly, and the local-only posture is shown on the surface (the `仅本地` / local chip, the `127.0.0.1` endpoint, the offline snapshot mode) rather than asserted in copy. This is an instrument, so it is dense where density answers a real question and quiet everywhere else.

It explicitly rejects four neighbors. It is not a hype-y SaaS dashboard (no gradient hero-metric template, no "supercharge your workflow"). It is not a dense neon-on-black ops cockpit that values widget count over legibility. It is not a generic admin template of identical icon-heading-text cards. And it is not a playful consumer-fintech spending tracker. It is a developer's measuring tool.

**Key Characteristics:**

- Warm paper light theme (`#f6f3ee`) and a warm near-black dark theme (`#14120f`); full `data-theme` parity.
- A single brand accent, Terracotta (`#c8553d`). Every other color is either neutral or a status semantic.
- Monospace carries all data; the sans carries prose and titles. The split is strict.
- Dark "instrument" cards embedded in the light page; depth by contrast, not shadow.
- Flat surfaces, 1px hairline borders, restrained radii (8–18px), pills reserved for toggles and tags.
- Bilingual (EN / ZH) and theme-switchable, with the chrome built to stay legible in all four combinations.

## 2. Colors

A warm, earthen palette: one terracotta accent against paper-and-ink neutrals, with a dark "instrument" family for data surfaces and three muted status hues.

### Primary

- **Terracotta** (`#c8553d`): the only brand color. Carries section eyebrows, bar fills, links, active toggle/preset states, the featured KPI, and chart bars. In dark theme it lifts to `#d96a52`.
- **Terracotta Soft** (`#f3dfd8`): bar tracks, the `local` tag, the featured-KPI gradient wash, soft active-state fills.
- **Terracotta Deep** (`#9a3e2b`): pressed/active accent text and the cap edge on bar fills; the higher-contrast end of the accent for text on soft backgrounds.

### Secondary (status semantics, never decoration)

- **Sage** (`#4f7a5c`): healthy / OK states, the live endpoint pulse, "good" insight tone, low-percentage chips.
- **Ochre** (`#c08a3b`): warnings, degraded or unsupported source levels, running-job button state.
- **Brick** (`#b84a3a`): errors and high-severity findings. Defined as `--danger` (light `#b84a3a`; dark brightens to `#e0654f` so it reads on the dark instrument/surface). High-severity findings carry it; medium/low use `--warn` / `--accent`.

### Neutral

- **Warm Paper** (`#f6f3ee`): page background. The committed warm identity, not a default cream; warmth is the brand, carried here and in the accent.
- **Surface** (`#ffffff`) and **Surface Raised** (`#faf7f2`): card and rail backgrounds. The three-step `paper → raised → white` layering is how flat surfaces separate.
- **Ink** (`#1c1a17`) / **Ink-2** (`#3a352f`): primary and secondary text; Ink also backs the active nav item and the primary button.
- **Muted** (`#736b60`) / **Muted-2** (`#b8b0a4`): secondary and tertiary text, labels, meta. Muted clears AA on Warm Paper (4.7:1; 5.3:1 on Surface); it was darkened from `#8a8278` (~3.4:1) in 2026-06 for AA. Dark-theme muted (`#8d867a`) stays 5.2:1. Muted-2 is reserved for decorative/inactive cases only (the breadcrumb separator, outside-month date cells, the active-nav badge on dark Ink), never body text.
- **Line** (`#e8e2d6`) / **Line-2** (`#ece6db`): hairline borders and dividers. The primary separation device on light surfaces.

### The Instrument Family (dark, theme-independent)

- **Instrument Ink** (`#1a1815`), **Instrument Surface** (`#25221e`), **Instrument Line** (`#34302a`), **Instrument Text** (`#f0ebe1`), **Instrument Muted** (`#8d867a`): the always-dark readout cards (trends, status panel, sync command center). These hold their values in both light and dark themes so the "lit display" reading never collapses into a flat second dark layer.

### Named Rules

**The Single Voice Rule.** Terracotta is the only brand color and stays at roughly ≤15% of any surface. Sage, Ochre, and Brick are status signals, never styling; if a color isn't carrying meaning, it's neutral.

**The Instrument Rule.** Data-dense readouts render on the dark instrument family in every theme. Depth is the contrast between warm housing and dark display, not a shadow.

## 3. Typography

**Display / Body Font:** system-ui stack (`-apple-system, BlinkMacSystemFont, 'Segoe UI'`)
**Data / Label Font:** ui-monospace stack (`SFMono-Regular, Consolas, 'Liberation Mono', Menlo`)

**Character:** Two faces, one strict division of labor. The system sans handles every human sentence: titles, hero copy, descriptions. The monospace handles every machine value: numbers, units, labels, eyebrows, table cells, endpoints, nav badges, the breadcrumb. Tabular figures (`font-variant-numeric: tabular-nums`, `tnum`) keep columns of data aligned. Body text enables stylistic sets `ss01` and `cv11`. There are no loaded web fonts; the system stacks are deliberate (fast, local, no network).

### Hierarchy

- **Display** (sans, 600, 38px, line-height 1.05, `-0.025em`): the page H1 / hero title; one per view, accent on a single emphasized word.
- **Headline** (sans, 600, 24px, `-0.018em`): section titles.
- **Title** (sans, 600, 18–22px, `-0.018em`): panel and sub-section headings (sources, projects, trends title).
- **Body** (sans, 400, 13–14.5px, line-height ~1.55): descriptions and prose; secondary copy in Muted.
- **Label** (mono, 600, 10.5px, `0.14em`, UPPERCASE): section eyebrows (Terracotta), field labels, table headers, status labels.
- **Metric** (mono, 500, 19–32px, `-0.02em` to `-0.025em`, tabular): KPI values, stat values, the numbers that are the point of each panel.

### Named Rules

**The Mono-Data Rule.** Every number, unit, code-like token, ID, table cell, and label is set in the monospace face with tabular figures. The sans face is for prose and titles only. A metric never appears in the sans face, and a sentence never appears in mono.

**The One Accent Word Rule.** The hero title emphasizes exactly one word in Terracotta (`.hero-title .accent`). Hierarchy otherwise comes from size and weight, not color.

## 4. Elevation

Flat by default, depth by contrast. Light surfaces sit flat on the paper and separate through 1px `--line` hairline borders and the warm three-step tonal layering (`warm-paper → surface-raised → surface`), never through drop shadows. The only real elevation in the system is the dark instrument family: those cards read as "raised displays" purely because they are dark against warm-light, an optical lift with no `box-shadow` doing the work.

Shadows appear in exactly three controlled places: the sync command center hero card (a deep ambient `0 20px 50px rgba(26,24,21,0.24)` plus accent glow), overlay surfaces (the date-picker popover), and soft lifts on active toggle states. Everything else is borderless-flat or hairline-bordered.

### Shadow Vocabulary

- **Ambient hero** (`box-shadow: 0 20px 50px rgba(26,24,21,0.24)`): the sync command center only. A single deep, soft shadow that marks the one true "floating" surface.
- **Soft lift** (`--shadow-soft`, for popovers and active toggles): light `0 8px 20px rgba(26,24,21,0.12)`, dark `0 8px 24px rgba(0,0,0,0.5)`. A quiet ambient lift, not a hard edge; used by the date-picker popover and the active refresh-interval pill.

### Named Rules

**The Flat-By-Default Rule.** Surfaces are flat at rest. Separation is a 1px hairline or a tonal step. Shadow is reserved for the one hero card and for overlays; it is never added to a panel, KPI, or button for decoration.

## 5. Components

The character across the board is **precise and tool-like**: tight radii, hairline borders, monospace figures, accent-capped data bars. Components read as parts of a measuring instrument, not as marketing surfaces.

### Buttons

- **Shape:** gently rounded (8px / `{rounded.sm}`).
- **Default (`.btn`):** white Surface fill, Ink-2 text, 1px Line border, padding `7px 12px`. Hover darkens the border to Ink (no shadow, no lift).
- **Primary (`.btn-primary`):** Ink fill, paper text, Ink border; hover deepens to pure Ink-strong. The running-job state swaps to Ochre/`--warn`.
- **No ghost-card:** buttons never pair the 1px border with a drop shadow.

### Tags & Pills

- **Tag (`.tag`):** mono 10.5px, Surface-Raised fill, 1px Line border, 5px radius, padding `3px 8px`. Semantic variants: `local` (Terracotta soft/deep), `ok` (Sage), `degraded`/`unsupported` (Ochre).
- **Pills (999px):** reserved for segmented toggles (range presets, refresh interval, trends window), status pills, and the `show-more` control. Pill radius is never used on cards.

### Cards & Panels

- **Panel (`.panel`):** Surface fill, 1px Line border, 14px radius (`{rounded.xl}`), padding `22px 24px`. The workhorse light container.
- **KPI (`.kpi`):** same shape, padding `18px 20px`; hover darkens border to Ink-2. The `featured` variant uses the Terracotta-soft gradient wash and accent-deep label.
- **Instrument cards (`.trends-card`, `.status-panel`, `.sync-command-center`):** dark Instrument family, 16–18px radius, generous padding. The signature surface; see The Instrument Rule.
- **Corner ceiling:** cards top out at 18px (the sync hero). Never round a card past that.

### Inputs & Fields

- **Style:** Surface fill, 1px Line border, 8px radius, min-height 34px, mono uppercase label above.
- **Focus:** `outline: 2px solid Terracotta`. Inputs use `:focus` (1px offset); a shared `a / button / summary:focus-visible` rule in `base.css` gives every button, link, and segmented control the accent ring at a 2px offset. Focus is always the accent, always visible.

### Navigation (sidebar)

- 248px fixed sidebar on Surface-Raised with a hairline right border; collapses to static at ≤720px.
- Nav items: 13.5px sans, Ink-2, 8px radius. Hover is a 6% Ink tint. **Active = Ink fill, paper text, Terracotta icon** (the accent only appears on the active item's icon).

### Data Bars (signature)

- Horizontal bar rows (`.bar-row`, `.source-row`, `.project-row`): Terracotta-soft track, Terracotta fill, a 2px Terracotta-deep cap edge on the fill. Name in mono, value right-aligned in mono. This is how every distribution (models, sources, projects, explorer) is drawn.

### Charts

- SVG, drawn on the dark instrument surface. Bars use a Terracotta luminance ramp: base `#d06047`, peak `#f08a6e`, hover `#f5a890`. Grid lines are low-alpha white; axis labels mono. The chart palette is monochromatic accent, so distinctions must not rely on hue alone (see Do's and Don'ts).

## 6. Do's and Don'ts

### Do:

- **Do** set every number, unit, label, and table cell in the monospace face with tabular figures (`.num` / `.mono`, `tnum`). The Mono-Data Rule is the system's signature.
- **Do** keep data-dense readouts on the dark instrument family and let warm-vs-dark contrast create the depth. No shadow needed.
- **Do** hold Terracotta to ≤15% of a surface: eyebrows, bar fills, links, active states, one featured KPI. When a color isn't carrying meaning, make it neutral.
- **Do** separate light surfaces with 1px `--line` hairlines and the `paper → raised → white` tonal steps.
- **Do** keep card radii in the 14–18px band; reserve 999px pills for toggles, tags, and badges.
- **Do** verify body text hits 4.5:1 in both themes. Muted is now `#736b60` (4.7:1 on Warm Paper; was `#8a8278`, ~3.4:1); keep Muted-2 (`#b8b0a4`) for decorative/inactive use only, never body copy.
- **Do** keep `--shadow-soft` and `--danger` defined in both theme blocks (added 2026-06). They were previously referenced but undefined, which silently no-opped the popover / active-toggle lift and dropped the red border on high-severity findings.
- **Do** keep the `prefers-reduced-motion: reduce` block in `base.css` (added 2026-06): it stills the endpoint `pulse`, collapses the 0.15–0.18s transitions to near-instant, and forces `scroll-behavior: auto` (the JS `scrollIntoView` in `app.js` checks the same query). Extend it whenever new motion is added.

### Don't:

- **Don't** build hype-y SaaS dashboards: no gradient hero-metric template, no "supercharge your workflow" copy, no vanity-metric theater.
- **Don't** drift toward a dense neon-on-black ops cockpit (Datadog / Grafana). Every panel must answer a question a user actually asks; density is earned, not default.
- **Don't** ship generic admin-template identical card grids (icon + heading + text, repeated).
- **Don't** adopt playful or gamified consumer-fintech treatments. This is a precise developer tool, not a money app.
- **Don't** introduce a second brand hue. Terracotta is the only accent; Sage / Ochre / Brick are status semantics, not palette expansion.
- **Don't** set a metric in the sans face, or a sentence in mono.
- **Don't** pair a 1px border with a wide drop shadow (the ghost-card pattern), and don't round any card past 18px.
- **Don't** distinguish chart series by hue alone; the chart ramp is one accent. Use labels, position, or luminance.
- **Don't** scatter section eyebrows. As of 2026-06 the uppercase-mono eyebrow is used exactly once, on the SYNC command center, as a single deliberate instrument label; every other section is named by its `<h2>` / `<h3>` title (the trends title is now an `<h2>`). Don't reintroduce a per-section eyebrow.
- **Don't** encode severity or tone with a thick `border-left` stripe. Finding cards (`.finding-card[data-severity]`) and insight rows (`.insight-row[data-tone]`) use a full 1px border in the matching token (`--danger` / `--warn` / `--good` / `--accent`) and carry the label as text (`.tag` / `.insight-label`); the colour is reinforcement, never the sole signal, and never a side-stripe (see the absolute ban).

## 7. Dashboard Layout Refresh Contract

### Source of truth

- Status: Draft
- Last refreshed: 2026-06-06
- Primary product surfaces: live dashboard, snapshot dashboard, docs dashboard screenshot.
- Evidence reviewed: `src/web/shell.rs`, `src/web/assets/base.css`, `src/web/assets/layout.css`, `src/web/assets/components.css`, `src/web/assets/render/models.js`, `src/web/assets/render/behavior.js`, `src/web/assets/render/costs.js`, `docs/dashboard/index.md`, `docs/public/screenshots/web-dashboard-overview.png`, Catppuccin official palette.

### Brand

- Personality: precise local instrument, calm developer console, readable data workbench, Catppuccin-native softness.
- Trust signals: local-only chip, endpoint, explicit degraded states, estimated-cost wording, visible filters.
- Avoid: fixed-width SaaS canvas on wide monitors, decorative empty expanses, forced card stretching, non-Catppuccin palette drift, large rounded marketing sections.

### Product goals

- Goals: make the dashboard feel complete at 1440px through 2560px widths; remove large unowned blank regions; migrate light/dark themes to Catppuccin Latte/Mocha; preserve dense scanning for local usage, behavior, and cost investigation.
- Non-goals: no new analytics feature, no data-shape change, no external UI dependency, no visual-reference pixel matching.
- Success signals: wide screens use available space intentionally; sparse data renders compact empty/degraded states; section rhythm stays steady in ZH/EN and light/dark themes.

### Personas and jobs

- Primary personas: local CLI users, maintainers inspecting sync health, developers reviewing model/source/project/cost distribution.
- User jobs: scan current usage, identify concentration, inspect behavior/tool mix, estimate cost, confirm sync/source health.
- Key contexts of use: local browser launched by `llmusage serve`, static exported HTML, docs screenshot fixture.

### Information architecture

- Primary navigation: overview, trends, distributions, behavior, explorer, cost/status.
- Core routes/screens: single-page dashboard generated by `src/web/shell.rs`; live and snapshot modes share the same shell.
- Content hierarchy: overview stays first; distribution, behavior, and cost sections should become balanced workbench bands rather than isolated two-column islands.

### Design principles

- Principle 1: every visible background region must be owned by a layout role: page margin, gutter, panel interior, or intentional rest space.
- Principle 2: wide layouts should grow by adding useful columns or rebalancing panels, not by pinning the main canvas to the left.
- Principle 3: sparse data gets compact explanatory empty states, not tall vacant panels.
- Principle 4: data density is earned; keep the existing instrument language, typography split, color discipline, and flat hairline panels.
- Tradeoffs: masonry-like balance is preferred over equal-height rows when panel content lengths differ; max line length remains constrained inside prose even when the dashboard canvas widens.

### Visual language

- Color: migrate the dashboard token layer to Catppuccin Latte for light mode and Catppuccin Mocha for dark mode. This refresh supersedes the earlier warm-paper/terracotta color target for dashboard implementation while preserving the same semantic token names.
- Typography: retain system sans for titles/prose and mono for values, labels, bars, tables, IDs, and chips.
- Spacing/layout rhythm: introduce explicit responsive content rails and dashboard grid utilities; keep section gaps near the existing 56px rhythm, with no large blank holes inside sections.
- Shape/radius/elevation: preserve 8px controls, 14px panels, 16-18px instrument cards, hairline borders, and shadow only for approved overlays/hero surfaces.
- Motion: no new motion required; preserve reduced-motion behavior.
- Imagery/iconography: reuse existing inline icon language; no decorative illustration.

### Components

- Existing components to reuse: `.app`, `main`, `.section-head`, `.grid-2`, `.panel`, `.kpi-grid`, `.bar-row`, `.source-row`, `.project-row`, `.empty-state`, `.cost-grid`, `.mini-stat-grid`, `data-theme` theme switching.
- New/changed components: responsive dashboard rail, balanced section grids for distribution/behavior/cost, compact empty/degraded state variant, and shell classes that replace inline layout styles.
- Variants and states: light/dark, ZH/EN, live/snapshot, default/expanded ranking lists, degraded/no-data/insufficient-model states, narrow/mobile collapse.
- Token/component ownership: CSS stays in `base.css`, `layout.css`, and `components.css`; render modules should only add semantic classes and content.

### Accessibility

- Target standard: preserve keyboard navigation and WCAG AA text contrast already documented in this design system.
- Keyboard/focus behavior: no change to existing focus ring rules.
- Contrast/readability: blank-space fixes must not lower muted text contrast or place body copy on `--muted-2`.
- Screen-reader semantics: keep section headings and table structures; empty states should explain why content is unavailable.
- Reduced motion and sensory considerations: no new continuous animation; honor existing `prefers-reduced-motion` block.

### Responsive behavior

- Supported breakpoints/devices: mobile at <=720px, tablet/narrow desktop at <=1100px, standard desktop around 1440px, wide desktop at >=1600px and >=1920px.
- Layout adaptations: main rail expands and centers on wide screens; section grids use `auto-fit`/named variants or CSS columns where appropriate; cards collapse to one column on mobile.
- Touch/hover differences: preserve 44px mobile targets for toggles and controls; hover remains decorative only.

### Interaction states

- Loading: keep current loading copy but avoid tall skeleton voids.
- Empty: render compact, bordered explanations sized to content.
- Error: keep degraded states visible and specific.
- Success: keep health and no-failure states concise.
- Disabled: preserve opacity/cursor conventions.
- Offline/slow network: snapshot mode and secondary-refresh notices remain explicit.

### Content voice

- Tone: concise, precise, local-first, no hype.
- Terminology: use "估算", "本地", "degraded", "normalized", "snapshot", and existing section names consistently.
- Microcopy rules: explain missing data as capability/filter state, not as zero; avoid telling users to trust a blank area.

### Implementation constraints

- Framework/styling system: vanilla Rust-generated HTML, ES modules, CSS assets; no new frontend build step.
- Design-token constraints: keep existing CSS variable names for implementation compatibility, but remap their values to Catppuccin Latte/Mocha tokens.
- Performance constraints: CSS/layout-only where possible; keep render-time DOM generation simple and bounded.
- Compatibility constraints: live and snapshot dashboards share shell; docs screenshot remains generated via the testing example.
- Test/screenshot expectations: verify with formatting/lint/tests plus browser screenshots at 1440, 1920, and a mobile viewport.

### Open questions

- [ ] Whether future iterations should split the dashboard into tabs for very dense data / owner: product / impact: could change navigation depth.
- [ ] Whether docs screenshot baseline should move from 1440x1100 to include a wide-desktop artifact / owner: maintainers / impact: catches the original wide-blank regression earlier.

### Catppuccin theme mapping

- Light flavor: Catppuccin Latte.
- Dark flavor: Catppuccin Mocha.
- Neutral mapping: page/background tokens come from `base`/`mantle`/`crust`; cards, rails, and dividers come from `surface0`/`surface1`/`surface2`; text comes from `text`/`subtext1`/`subtext0`/`overlay*`.
- Accent mapping: use `mauve` or `blue` for primary interactive accent, `peach` for bar fills and warm data emphasis, `green` for success, `yellow`/`peach` for warnings, and `red`/`maroon` for danger. Do not keep `terracotta` as a hidden brand color after migration.
- Instrument surfaces: dark readout cards should stop being theme-independent custom browns. In Latte they may use a Mocha-like readout surface only if it is explicitly mapped from Mocha `base`/`surface0`; in Mocha they should integrate with Mocha `mantle`/`surface0` without becoming a second unrelated dark palette.
