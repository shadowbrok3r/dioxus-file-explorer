# UI Style Guide

This file documents the unified styling system built around `crimson.css` tokens and the semantic + attribute driven classes now used across the app.

## Core Principles

1. Surfaces stay within the dark neutral range: `--color-surface-800` .. `--color-surface-950`.
2. No ad-hoc color hex values in component markup. Use semantic tokens or theme variables.
3. Spacing, border radius, and typography scale come from the existing tailwind utilities or token-derived classes—do not introduce random pixel sizes unless scoped and necessary.
4. Styling logic prefers attribute selectors (`data-style`, `data-selected`, `data-accent`) plus a small set of semantic classes instead of one-off class clusters.

## Semantic Structural Classes

| Class | Purpose |
|-------|---------|
| `panel` | Generic surface container (cards, sidebars, overlays). Pair with `data-style` for variant. |
| `file-card` | Grid card representation of a file (icon/thumbnail + meta). |
| `file-row` | Row representation used in details/list views. |
| `file-chip`, `file-chip-cat`, `file-chip-tag` | Inline metadata chips for category & tags. |
| `similarity-badge` | Highlighted badge for similarity score or other metrics. |

## Attribute Variants

`data-style` applies a visual treatment:
- `glass` (default accent-tinted translucent surface; primary interactive look)
- `outline` (neutral surface with subtle border)
- `destructive` (error/destructive emphasis — used sparingly)
- future: `ghost` (text emphasis minimal chrome)

Any element can use `data-style`; if you omit it on interactive elements that carry the `button` base class, they receive the `glass` default.

### Selection State
Use either:
- `data-selected="true"` on the structural element (preferred), OR
- Add/remove the helper class `is-selected` (legacy fallback)

CSS applies the same highlight semantics for both.

## Buttons

Use `class="button"` plus optional `data-style` variant. Remove legacy `.btn` usages; a compatibility alias exists temporarily but will be purged.

Examples:
```rsx
button { class: "button", "data-style": "outline", "Open" }
button { class: "button", "Delete", "data-style": "destructive" }
```

## Panels / Containers
Wrap logical UI sections (`aside`, modals, sidebars, inspector blocks) with `class="panel"` and the appropriate `data-style`.

Guidelines:
- Use `data-style="outline"` for most structural frames (sidebars, preview pane root, dialog bodies if not already thematically handled).
- Use `data-style="glass"` for elevated or interactive focal surfaces (thumbnail area, floating overlays).

## File Representations
- Cards: `div.file-card[data-selected]` contain thumbnail, filename, metadata chips, buttons.
- Rows: `div.file-row[data-selected]` for list/detail view.
- Maintain layout spacing; do not collapse existing margins without UX review.

## Chips & Badges
Use dedicated classes (`file-chip-*`, `similarity-badge`) instead of ad-hoc background/text utilities. They automatically adapt to selection/hover states.

## Typography & Density
Global normalization sets base font sizing and smoothing. Adjust density via future `data-density` (planned) if compact modes are added.

## Deprecated / To Remove
The following legacy classes should not appear in new code:
- `bg-panel`
- `bg-muted`
- `border-stroke`
- `selected-item` / `non-selected-item`
- `.btn`

Temporary allowances remain for table row borders & thin separators; borders are now provided by panel + outline styles or specific border utilities.

## Migration Checklist (Component Authors)
- Replace container wrappers with `panel` + `data-style`.
- Standardize all buttons: `button` class, remove stray padding/background classes.
- Convert stateful highlight logic to `data-selected`.
- Eliminate raw color utilities unless they map to tokens (accent usage acceptable for emphasis text).
- Use chips/badges for metadata inline items (tags, category, similarity, status).

## Do / Don’t Examples

| Do | Don’t |
|----|-------|
| `div { class: "panel", "data-style": "outline" }` | `div { class: "bg-panel border border-stroke" }` |
| `button { class: "button", "data-style": "outline" }` | `button { class: "px-2 py-1 bg-panel" }` |
| `div { class: "file-row", "data-selected": "true" }` | `div { class: "selected-item" }` |

## Adding New Variants
If you need a new visual variant:
1. Prefer an attribute on existing semantic class (e.g., `data-state="warning"`).
2. Add minimal CSS in `tailwind.css` grouped near related rules.
3. Avoid introducing broad new color tokens without palette alignment.

## Future Enhancements (Backlog)
- `data-density` compact/comfortable modes.
- `ghost` button & panel style.
- Centralized motion/transition tokens.
- Light theme mapping (token inversion) once needed.

## Questions / Updates
Document any new semantic class or attribute usage here during PRs to keep design language consistent.
