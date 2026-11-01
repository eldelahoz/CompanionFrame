---
name: CompanionFrame
description: A calm, dark-first companion for Warframe. Soft greys, one Lotus-teal accent, the game's own art for game data.
colors:
  shell: "#0e0f11"
  ground: "#131417"
  surface: "#18191d"
  raised: "#1f2126"
  hover: "#25282e"
  text: "#e7e8eb"
  text-secondary: "#a3a7b0"
  text-muted: "#80858f"
  lotus-teal: "#3fb8af"
  lotus-teal-bright: "#5ccac1"
  lotus-teal-ink: "#04201d"
  success: "#4cc38a"
  warning: "#e2b54a"
  danger: "#ef6f6c"
  info: "#74a9ee"
  light-shell: "#f1f2f4"
  light-ground: "#f8f9fa"
  light-surface: "#ffffff"
  light-text: "#16181d"
  light-text-secondary: "#4b505a"
  light-lotus-teal: "#0f7f78"
typography:
  page-title:
    fontFamily: "Geist, ui-sans-serif, system-ui, Segoe UI, sans-serif"
    fontSize: "1.4286rem"
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: "-0.011em"
  card-title:
    fontFamily: "Geist, ui-sans-serif, system-ui, Segoe UI, sans-serif"
    fontSize: "1rem"
    fontWeight: 600
    lineHeight: 1.4
  body:
    fontFamily: "Geist, ui-sans-serif, system-ui, Segoe UI, sans-serif"
    fontSize: "1rem"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "Geist, ui-sans-serif, system-ui, Segoe UI, sans-serif"
    fontSize: "0.9286rem"
    fontWeight: 500
    lineHeight: 1.4
  caption:
    fontFamily: "Geist, ui-sans-serif, system-ui, Segoe UI, sans-serif"
    fontSize: "0.8125rem"
    fontWeight: 400
    lineHeight: 1.4
  stat:
    fontFamily: "Geist, ui-sans-serif, system-ui, Segoe UI, sans-serif"
    fontSize: "1.8571rem"
    fontWeight: 600
    lineHeight: 1.15
    letterSpacing: "-0.02em"
rounded:
  sm: "0.25rem"
  md: "0.4375rem"
  lg: "0.625rem"
  xl: "0.875rem"
  full: "999px"
spacing:
  "1": "0.25rem"
  "2": "0.5rem"
  "3": "0.75rem"
  "4": "1rem"
  "5": "1.25rem"
  "6": "1.5rem"
  "8": "2rem"
  "10": "2.5rem"
components:
  button-primary:
    backgroundColor: "{colors.lotus-teal}"
    textColor: "{colors.lotus-teal-ink}"
    rounded: "{rounded.md}"
    height: "2.25rem"
    padding: "0 0.875rem"
  button-primary-hover:
    backgroundColor: "{colors.lotus-teal-bright}"
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
    height: "2.25rem"
    padding: "0 0.875rem"
  input:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
    height: "1.875rem"
    padding: "0 0.65rem"
  card:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.lg}"
  chip:
    textColor: "{colors.text-secondary}"
    rounded: "{rounded.full}"
    height: "1.75rem"
    padding: "0 0.65rem"
  readout-chip:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    typography: "{typography.label}"
  nav-item:
    textColor: "{colors.text-secondary}"
    rounded: "{rounded.md}"
    height: "2.25rem"
  nav-item-active:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.text}"
---

# Design System: CompanionFrame

## Overview

CompanionFrame sits on a second monitor or behind an alt-tab during long Warframe sessions, often in a dim room. The player glances at it for seconds at a time: is this relic worth cracking, did my order get undercut, how long until night on the Plains. The interface is a working tool in the Linear/Raycast family: dark first, soft cool greys, generous spacing, crisp type and one accent. It gets out of the way and lets the numbers read.

The world it borrows from the game is limited to three things. The game's own art marks game data (relics by tier and refinement, platinum, ducats, polarities, rarity). Our own angular, Tenno-flavoured line glyphs mark the sidebar. Every currency value is a **Readout**: tabular numerals behind the currency's art, so prices scan like an instrument column across lists, tables and overlays. That is the signature; everything else is a clean, standard product UI.

The single source of truth is `src/styles/tokens.css` (CSS variables, dark on `:root`, light on `[data-theme="light"]`), bridged to Tailwind in `src/index.css` `@theme` and written at runtime by the theme editor (`src/lib/theme/applyTheme.ts`) from the active preset. Components live in `src/components/ui` (base) and `src/components/icons` (glyphs and game art).

## Colors

Restrained: tinted neutrals plus one accent. State colours appear only where they mean something.

### Primary

- **Lotus teal** (`--accent`, #3fb8af dark / #0f7f78 light): primary actions, the current selection, focus rings, the active nav glyph core, the tab underline. Never decoration, never a large fill. Text on it uses the dark ink (`--text-on-accent` #04201d) because white fails contrast on a mid-tone teal; the theme engine picks the ink by contrast for any custom accent.
- `--accent-bright` is the hover; `--accent-glow` (14% teal) is the selected wash; `--text-link` is the accent tuned for text.

### Neutral

- Shell (`--bg-deep` #0e0f11): titlebar, sidebar, status bar. One step darker than the content so the frame recedes.
- Ground (`--bg-base` #131417), Surface (`--bg-surface` #18191d) for cards, Raised (`--bg-raised` #1f2126) for controls, Hover (`--bg-hover`).
- Borders are white at 7.5% / 13% (`--border`, `--border-strong`), ink at 8.5% / 15% in light.
- Text: primary #e7e8eb, secondary #a3a7b0, muted #80858f (4.6:1 on the ground; AA). Light: #16181d / #4b505a / #686d77.

### Semantic and game data

- Success #4cc38a, warning #e2b54a, danger #ef6f6c, info #74a9ee, each with a 13% wash (`--*-bg`) for badges.
- Game data keeps its own conventional hues as data, not decoration: relic tiers (`--relic-*`), rarity (`--rarity-*`), riven grades, world-cycle states. Shape or text always accompanies the colour.

### Named Rules

**The one accent rule.** If a second hue starts doing teal's job (a blue button, a gold highlight), it is wrong. Status colours inform; teal acts.

**The game-art rule.** Game data uses the game's own art, loaded at run time from public sources and cached locally; it is never committed to the repo. UI controls use Lucide. Sidebar views use our own glyphs. Never mix these roles.

## Typography

One family: **Geist** (variable, self-hosted woff2, SIL OFL), with `font-variant-numeric: tabular-nums` on every number that changes. `--font-display` survives only as an alias of the same face.

### Hierarchy

- Page title: 20px (`--text-xl`) / 600 / -0.011em.
- Card and modal titles: 14–16px / 600.
- Body: 14px (`--text-md`) / 400 / 1.5.
- Labels, table text, buttons: 13px (`--text-sm`) / 500.
- Captions and meta: 11.4px (`--text-xs`).
- Stat values: 26px (`--text-2xl`) / 600 / -0.02em, tabular.

The scale is fixed rem (ratio about 1.15) on a 14px root. The theme font scale (0.75–1.5) changes the root size; the app's UI scale setting (`ui_scale`, 0.75–1.5) zooms the webview on top, and Windows display scaling (100–200%) is applied by WebView2. Everything is rem, so all three compose.

### Named Rules

**No tracked caps.** No uppercase eyebrows or letter-spaced labels; the old Orokin flavour is gone. Hierarchy comes from size and weight.

## Layout

- 4px spacing grid (`--space-1` … `--space-10`); more space above a heading than below it.
- Content is capped at 1680px and padded by `--content-gutter` (fluid 16–40px).
- Breakpoints (window width): **≤ 640px** the sidebar becomes a drawer behind the titlebar menu button and modals become bottom sheets; **≤ 1024px** the sidebar becomes a 60px icon rail with tooltips (users can also choose the rail at any width); **wide** screens get more grid columns. Short windows (≤ 640px tall) also get sheets.
- Components respond to their own width with container queries (`.ui-table-wrap`, `.relics-view`, `.ui-cq`), so pop-out windows (`OpenInWindowButton`) behave like a narrow main window.
- The layout engine stacks sections when its grid is narrower than 720px (`LAYOUT_NARROW_MAX_PX`) and runs two columns above it.
- Tables either scroll sideways with the first column pinned (`Table mode="scroll"`) or turn rows into labelled cards in narrow containers (`mode="stack"`).
- Touch screens (`pointer: coarse`) raise hit targets to 44px.

## Elevation & Depth

Depth is quiet: borders do most of the separation, shadows only lift what floats.

### Shadow Vocabulary

- `--shadow-1` (0 1px 2px): resting cards and buttons.
- `--shadow-2` (0 6px 20px -4px): popovers, menus, toasts.
- `--shadow-3` (0 24px 60px -12px): modals and the drawer.
- `--scrim`: 60% near-black behind modals and the drawer (32% ink in light).

### Overlays

Overlays sit on the game, not in the app, so they have their own tokens: `--ov-surface` (12,14,17 at 78%) with `--ov-blur` 10px backdrop blur, `--ov-border` white 14%, `--ov-text` near-white regardless of the app theme. Type is sized from the game height: `--ov-font` = 1.48vh (16px at 1080p, 21px at 1440p, 32px at 4K) and every overlay dimension is in em, so layouts scale across 16:9, 16:10 and 21:9.

## Shapes

Soft corners: 4 / 7 / 10 / 14px (`--radius-sm` … `--radius-xl`), full pills for chips and status pills. The theme editor can switch to sharp or round corners; components only ever reference the radius tokens.

## Components

All in `src/components/ui`; recipes in `src/styles/ui.css`.

### Buttons

`Button` variants: primary (teal, dark ink), default (surface + strong border), ghost, danger (red text, red wash on hover); sizes md (36px) and sm (30px); `loading` shows a spinner and blocks clicks. `IconButton` is a 30px square ghost button whose `label` is both its accessible name and its tooltip. Every interactive element shows the same focus ring: 2px teal outside a 2px ground-coloured gap.

### Chips

`Chip` is a toggleable pill (`aria-pressed`); on, it takes the teal wash. Legacy `.filter-tab` buttons use the same recipe. `Badge` is a static 21px label in neutral, accent, success, warning, danger or info.

### Cards / Containers

`Card` (surface, 1px border, radius-lg, shadow-1) with `CardHeader` (title, muted meta, right-aligned tool buttons). `StatStrip` is one card divided by hairlines, not a row of KPI cards. `PageHeader` holds the page title, a muted subtitle and right-aligned actions.

### Inputs / Fields

`Input`, `SearchInput`, `Select` (custom chevron), `Field` (label above control), `Switch` (immediate on/off) and `Checkbox` (selection). 30px tall, raised background, strong border, teal border plus a 3px teal wash on focus, red border when `aria-invalid`.

### Navigation

Sidebar groups (Overview, Live, Trade, Data) with Settings and the collapse toggle in the footer. Each item is our own 24px line glyph (`NavIcon`); the active item gets a raised background and its glyph's small core fills with teal. Titlebar: brand mark, Ctrl+K view search, helper and warframe.market status pills, theme toggle, window controls. `Tabs`/`HeaderTabs` are underline tabs with a teal marker and optional icon and count; `Segmented` is for 2–4 exclusive options.

### Readout (signature)

`Readout` renders a currency value: the game art for platinum, ducats, endo or credits, then tabular numerals; `chip` adds the raised pill, `label` adds a muted qualifier ("EV"), `sign` tints gains. Use it for every currency value so columns align.

### Feedback

`Table` (responsive, see Layout), `Modal` (centred dialog, bottom sheet when narrow; focus trap and Escape from `ModalShell`), `Toast` (bottom-right, icon by tone), `TooltipHost` (one fixed tooltip for any `[data-tip]`, shown on hover and keyboard focus), `EmptyState` (icon, title, one teaching sentence, an action) and `Skeleton` (shaped placeholders, never a centred spinner).

## Do's and Don'ts

- Do build screens from `src/components/ui` and the tokens; never hard-code a colour, radius or shadow.
- Do use `Readout` for every platinum, ducat, endo and credit value.
- Do give icon-only buttons a `label`; it becomes the tooltip and the accessible name.
- Do put new strings through `tr()` with keys in en, de, zh and fr.
- Don't use teal for decoration, large fills or inactive states.
- Don't add uppercase tracked labels, gradient text, glows or coloured side stripes.
- Don't use a modal when an inline control would do.
- Don't commit game art (Digital Extremes assets); load it at run time.
