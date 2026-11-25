# Notium Theme Data — Design Tokens & UI Style

> Single source of truth for the Notium visual design system.
>
> **Provenance.** Every Obsidian value in this file was read directly from the default theme
> stylesheet (`app.css`) shipped inside **Obsidian 1.15.0** (desktop package, which also carries
> the mobile `.is-mobile` / `.is-phone` / `.is-android` rules), and cross-checked against the
> Android screenshots in `reference/`. Verified 2026-11-16. Hex values for the accent are
> computed from Obsidian's HSL definition. Do not "correct" these from memory or from
> [`Obsidian_UI_Style_Reference.md`](./Obsidian_UI_Style_Reference.md), which predates this
> verification and contains wrong values (blue accent, Feather icons, 56px ribbon, etc.).
>
> Tokens are applied in [`src/app.css`](../src/app.css). Change a token **here first**, then in
> `app.css`; never hardcode hex values in components.

---

## 1. Design Philosophy (what Obsidian actually does)

- **Purple accent**, not blue: `hsl(258, 88%, 66%)` = `#8a5cf5`.
- **Neutral gray ramp** (`--color-base-00 … 100`), no tint. On desktop dark the **editor is the
  darkest surface** (`#1c1c1c`) and sidebars are *lighter* (`#282828`).
- **Mobile dark is a different palette**: pure-black editor (`#000`), `#1e1e1e` drawers.
  **Notium divergence (2026-11-22):** Notium uses this mobile palette on every screen size
  (desktop and tablet included), so `src/app.css` has one dark and one light palette with no
  `.is-compact` colour overrides.
- **Soft geometry**, not sharp: radius scale `4 / 8 / 12 / 24px`; on mobile most controls are
  full pills (radius = touch size).
- **Hover is an overlay**, not a solid color: 6.7% of white (dark) / black (light).
- **System fonts**, **Lucide** icons (not Feather), 16px text.
- **Borders are quiet**: 1px on desktop, 0.5px on phones; inputs on mobile have no border.

---

## 2. Obsidian Foundations (verified reference)

### 2.1 Base gray ramp

| Step | Dark | Light | Mobile dark override | Role |
|---|---|---|---|---|
| `base-00` | `#1c1c1c` | `#ffffff` | `#000000` | `--background-primary` (editor) |
| `base-05` | `#212121` | `#fcfcfc` | | light `--background-secondary-alt` |
| `base-10` | `#232323` | `#fafafa` | `#111111` | `--background-primary-alt`, code background |
| `base-20` | `#282828` | `#f6f6f6` | `#1e1e1e` | `--background-secondary` (sidebars, menus) |
| `base-25` | `#2e2e2e` | `#efefef` | | dark form fields |
| `base-30` | `#333333` | `#e4e4e4` | | `--background-modifier-border`; dark buttons |
| `base-35` | `#3f3f3f` | `#dadada` | | border hover; menu/modal borders |
| `base-40` | `#555555` | `#bdbdbd` | | border focus |
| `base-50` | `#666666` | `#ababab` | | `--text-faint` |
| `base-60` | `#999999` | `#707070` | | (unused by text tokens) |
| `base-70` | `#b3b3b3` | `#5c5c5c` | | `--text-muted` |
| `base-100` | `#dadada` | `#222222` | light mobile `#000` | `--text-normal` |

### 2.2 Accent

`--accent-h: 258; --accent-s: 88%; --accent-l: 66%`

| Obsidian variable | Dark | Light |
|---|---|---|
| `--color-accent` | `#8a5cf5` | `#8a5cf5` |
| `--color-accent-1` | `#a68af9` | `#9873f7` |
| `--color-accent-2` | `#c5b6fc` | `#a68af9` |
| `--text-accent` (links, tags) | `#a68af9` | `#8a5cf5` |
| `--text-accent-hover` | `#c5b6fc` | `#a68af9` |
| `--interactive-accent` (buttons, checkboxes, toggles) | `#8a5cf5` | `#9873f7` |
| `--interactive-accent-hover` | `#a68af9` | `#a68af9` |
| `--text-on-accent` | `#ffffff` | `#ffffff` |
| `--text-selection` | accent 33% | accent 20% |

### 2.3 Extended colors

| Color | Dark | Light | Used for |
|---|---|---|---|
| red | `#fb464c` | `#e93147` | errors; callouts bug/error/fail/danger; code tag/operator |
| orange | `#e9973f` | `#ec7500` | warnings; callouts warning/question |
| yellow | `#e0de71` | `#e0ac00` | highlight (30% opacity); code function |
| green | `#44cf6e` | `#08b94e` | success; callout success; code string |
| cyan | `#53dfdd` | `#00bfbc` | callouts tip/summary/important; code property |
| blue | `#027aff` | `#086ddd` | callouts note/info/todo (default) |
| purple | `#a882ff` | `#7852ee` | callout example; code value |
| pink | `#fa99cd` | `#d53984` | code keyword |
| — | `#9e9e9e` | `#9e9e9e` | callout quote |

### 2.4 Semantic surfaces & states

| Obsidian variable | Value |
|---|---|
| `--background-modifier-hover` | 6.7% white (dark) / black (light) overlay |
| `--nav-item-background-selected` | accent at 15% opacity |
| `--background-modifier-active-hover` | accent at 10% opacity |
| `--background-modifier-cover` (modal backdrop) | dark `rgba(10,10,10,.4)`, light `rgba(220,220,220,.4)`, mobile `rgba(0,0,0,.15)` |
| `--interactive-normal` / `-hover` | dark `#333333` / `#3f3f3f`; light `#ffffff` / `#fafafa` |
| `--menu-background` | `--background-secondary`, border `base-35`, radius 8px |
| `--tooltip-background` | 90% black, text `#fafafa`, radius 12px |
| `--scrollbar-thumb-bg` / active | 10% / 20% of white (dark) or black (light); track transparent |
| `--shadow-s` (menus, popovers) | dark `0 1px 2px rgba(0,0,0,.121), 0 3.4px 6.7px rgba(0,0,0,.179), 0 15px 30px rgba(0,0,0,.3)`; light same offsets at `.028 / .042 / .07` |

### 2.5 Typography

| Item | Value |
|---|---|
| UI / text font | `ui-sans-serif, -apple-system, BlinkMacSystemFont, system-ui, "Segoe UI", "Google Sans Flex", Roboto, "Inter Variable", "Inter", sans-serif` |
| Monospace | `ui-monospace, SFMono-Regular, "Cascadia Mono", "Roboto Mono", "DejaVu Sans Mono", "Liberation Mono", Menlo, Monaco, Consolas, "Source Code Pro", monospace` |
| Editor text | `16px`, line-height `1.5` (`1.3` tight), weight 400, bold = +200 (600) |
| Line width | `700px`; file margins `32px` desktop, `8px 24px` mobile |
| UI sizes (desktop) | smaller `12px`, small `13px`, medium `15px`, large `20px` |
| UI sizes (mobile) | relative to text size: `×0.8` (12.8px), `×0.937` (15px), `×1` (16px), `×1.2` (19.2px) |
| Paragraph spacing | `1rem`; heading spacing `2.5rem` |
| Code | `0.875em`, radius 4px, no border, background `base-10` |

| Heading | Size | Weight | Line height | Letter spacing |
|---|---|---|---|---|
| H1 / inline title | `1.618em` | 700 | 1.2 | `-0.015em` |
| H2 | `1.462em` | 680 (600 without variable font) | 1.2 | `-0.011em` |
| H3 | `1.318em` | 660 (600) | 1.3 | `-0.008em` |
| H4 | `1.188em` | 640 (600) | 1.4 | `-0.005em` |
| H5 | `1.076em` | 620 (600) | 1.5 | `-0.002em` |
| H6 | `1em` | 600 | 1.5 | `0` |

### 2.6 Shape, spacing, icons, motion

| Item | Value |
|---|---|
| Radius | `s 4px`, `m 8px`, `l 12px`, `xl 24px` (mobile `xl 18px`) |
| Touch sizes (mobile) | `xxs 24`, `xs 30`, `s 40`, `m 44`, `l 52`, `xl 60` px — also used as radii |
| Spacing | 2px grid `2 / 4 / 6`; 4px grid `4 / 8 / 12 / 16 / 20 / 24 / 32 / 36 / 40 / 48 / 64 / 72` |
| Border width | `1px` desktop, `0.5px` phone |
| Icons (Lucide) | desktop `xs 14`, `s 16`, `m 16`, `l 18`, `xl 32` px; stroke `2px` (`l` 1.75px, `xl` 1.25px); opacity .85 |
| Icons (mobile) | `xs/s 18`, `m 20`, `l 24` px; navbar 26px; opacity 1 |
| Icon color | `--text-muted`; active `--text-accent`; mobile headers/navbar `--text-normal` |
| Motion | superfast `70ms`, fast `140ms`, moderate `300ms`, slow `560ms`; mobile control transitions `150ms ease-in-out` |

### 2.7 Desktop components

| Component | Spec |
|---|---|
| Ribbon | width `44px`, background `--background-secondary` |
| Tab bar / header | height `40px`; container `--background-secondary`; active tab `--background-primary` with radius `10px 10px 0 0`; tab text 13px, inactive `--text-faint`, active `--text-normal`. **No accent underline.** |
| Title bar | height `30px`, `--background-secondary` |
| Status bar | 12px, `--text-muted`, `--background-secondary`, floating bottom-right with radius `8px 0 0 0` |
| File tree row | 13px, `--text-muted` → `--text-normal` on hover/selected; padding `4px 8px 4px 24px`; radius `8px`; margin-bottom 2px; hover = hover overlay; selected = accent 15% |
| Input | height `30px`, radius `5px`, padding `4px 8px`, 1px border, focus ring 2px |
| Button | same radius as input; dark background `#333333` → `#3f3f3f` |
| Toggle | `40×22`, thumb `18px` white, radius `18px` |
| Checkbox | size = text size, radius `4px`, border `--text-faint`, checked `--interactive-accent` |
| Modal | radius `12px`, border `base-35`, 90vw × 85vh up to 1100px; dialog `560px` |
| Command palette / prompt | width `700px`, max 80vw × 70vh, input `40px` |
| Settings groups | background `--background-primary-alt`, radius `12px`, padding `20px`, 1px dividers |
| Tag | pill radius `2em`, padding `.25em .65em`, text `--text-accent`, background accent 10% (20% hover, 20% on mobile dark), no border |
| Link | `--text-accent`, **underlined** (internal and external); unresolved at 70% opacity |
| Blockquote | `2px` left border in `--interactive-accent`, transparent background, normal text color |
| Embed | `2px` left border in accent, left padding 24px |
| Callout | no border, radius `4px`, padding `12px 12px 12px 24px`, background = callout color at ~10%, title bold in callout color |
| Horizontal rule | `2px`, border color |
| Table | 1px borders, transparent header, header weight 600, line-height 1.3 |
| List marker | `--text-faint`, bullet `0.3em` circle |

### 2.8 Mobile (phone) — verified against `reference/` screenshots

| Element | Spec |
|---|---|
| Palette (dark) | editor `#000000`; drawers, modals, settings, menus `#1e1e1e`; code `#111111`; text `#dadada` |
| Raised controls | `--interactive-normal` = drawer + 10% white ≈ `#353535`; hover/pressed + 16% ≈ `#424242` |
| Settings groups | drawer + 4% white ≈ `#272727`, dividers + 8% ≈ `#303030`, radius 30px |
| Sidebar drawer | `100vw` on phones (84vw, 300–450px on tablets), no radius |
| File row | 16px, `--text-normal`, padding `8px 8px 8px 24px`, radius `30px`, no bottom margin; folders show only a chevron; selected = soft gray pill |
| Drawer action icons | 24px, `--text-muted`, row centered above the view selector |
| View selector ("Files" / "Backlinks") | pill, radius 30px, background `--interactive-normal`, 24px icon, 16px semibold label, up/down chevron in `--text-faint`; opens upward as a list in the same surface |
| Vault footer | name `19.2px` semibold + small chevron; info line `12.8px` `--text-muted`; circular settings button on the right |
| View header | height `44px`; circular buttons (radius = height) on `--interactive-normal`; 24px icons in `--text-normal`; grouped actions share one pill |
| Floating navbar | height `52px`, fully rounded, max width `316px`, centered, bottom offset `max(safe-area, 12px)`; icons 26px `--text-normal` |
| Inputs / search | height `44px`, full pill, **no border, no shadow**, weight 500, padding `4px 16px`, background `--interactive-normal` |
| Toggle | `52×26+`, thumb `26px` |
| Modal | radius `30px`, borderless, full width; cover `rgba(0,0,0,.15)` |
| Settings | centered semibold title, circular close button, pill search, group heading semibold `--text-muted`, rows with 20px outline icon + 16px medium label + chevron |
| Borders / dividers | `0.5px` |
| Caret | `--text-accent` (purple) |
| Scrollbar (Android) | 5px, no radius |

---

## 3. Notium Tokens (as implemented in `src/app.css`)

Notium keeps its short flat token names. Each maps to an Obsidian variable.

### 3.1 Surfaces & text

| Token | Dark | Light | Mobile dark | Obsidian source | Usage |
|---|---|---|---|---|---|
| `--bg` | `#1c1c1c` | `#ffffff` | `#000000` | `--background-primary` | Editor / workspace |
| `--panel` | `#282828` | `#f6f6f6` | `#1e1e1e` | `--background-secondary` | Sidebars, drawers, panels |
| `--panel-2` | `#2e2e2e` | `#efefef` | `#303030` | `base-25` (≈ hover overlay on panel) | Hover rows, cards, inputs, selected mobile row |
| `--border` | `#333333` | `#e4e4e4` | — | `--background-modifier-border` | Borders, dividers |
| `--pill-bg` | `#282828` | `#f6f6f6` | `#353535` | `--menu-background` / mobile `--interactive-normal` | Floating toolbars, menus, mobile pills |
| `--pill-border` | `#3f3f3f` | `#dadada` | `#424242` | `--background-modifier-border-hover` | Floating surface borders |
| `--text` | `#dadada` | `#222222` | light mobile `#000000` | `--text-normal` | Body text |
| `--muted` | `#b3b3b3` | `#5c5c5c` | — | `--text-muted` | Secondary text, icons |
| `--faint` | `#666666` | `#ababab` | — | `--text-faint` | Placeholders, chevrons, list markers |
| `--active-text` | `#dadada` | `#222222` | — | `--nav-item-color-selected` | Text on `--accent-dim` |

### 3.2 Accent & semantic

| Token | Dark | Light | Obsidian source | Usage |
|---|---|---|---|---|
| `--accent` | `#8a5cf5` | `#8a5cf5` | `--color-accent` | Links, active states, checkboxes, primary buttons |
| `--accent-text` | `#a68af9` | `#8a5cf5` | `--text-accent` | Link text in the editor |
| `--accent-hover` | `#a68af9` | `#a68af9` | `--interactive-accent-hover` | Accent on hover |
| `--accent-dim` | accent 15% | accent 15% | `--nav-item-background-selected` | Selected item background |
| `--selection-bg` | `rgba(138,92,245,.33)` | `rgba(138,92,245,.2)` | `--text-selection` | Text selection |
| `--focus-ring` | `rgba(138,92,245,.5)` | `rgba(138,92,245,.4)` | — (Notium) | Focus outline |
| `--danger` | `#fb464c` | `#e93147` | `--color-red` | Errors, destructive actions |
| `--success`, `--md` | `#44cf6e` | `#08b94e` | `--color-green` | Success, Markdown marker |
| `--warning` | `#e9973f` | `#ec7500` | `--color-orange` | Warnings |
| `--callout-blue` | `#027aff` | `#086ddd` | `--color-blue` | Info / note callouts (default) |
| `--callout-cyan` | `#53dfdd` | `#00bfbc` | `--color-cyan` | Tip callouts |

`--accent` is the fill color (buttons, checkboxes, active states); `--accent-text` is the
lighter variant Obsidian uses for link text on dark backgrounds.

### 3.3 Code, highlight, scrollbars, misc

| Token | Dark | Light | Mobile dark | Obsidian source |
|---|---|---|---|---|
| `--code-bg` | `#232323` | `#fafafa` | `#111111` | `--code-background` |
| `--code-fg` | `#dadada` | `#222222` | — | `--code-normal` |
| `--highlight-bg` | `rgba(224,222,113,.3)` | `rgba(224,172,0,.3)` | — | yellow at 30% |
| `--scrollbar-thumb` | `rgba(255,255,255,.1)` | `rgba(0,0,0,.1)` | — | `--scrollbar-thumb-bg` |
| `--scrollbar-thumb-hover` | `rgba(255,255,255,.2)` | `rgba(0,0,0,.2)` | — | `--scrollbar-active-thumb-bg` |
| `--caret` | `#dadada` | `#222222` | `#a68af9` (light mobile `#8a5cf5`) | `--caret-color` |
| `--shadow` | Obsidian `--shadow-s` (see §2.4) | same, lighter | — | `--shadow-s` |

The mobile overrides live in a `@media (max-width: 767px)` block right after the light theme
block in `app.css`.

### 3.4 Typography

| Token | Value | Note |
|---|---|---|
| `--font-ui` | `"Poppins", system-ui, -apple-system, "Segoe UI", Roboto, sans-serif` | **Intentional divergence** — Obsidian uses the system stack (§2.5). |
| `--font-mono` | `"Cascadia Code", ui-monospace, "SFMono-Regular", Consolas, monospace` | Obsidian lists Cascadia Mono. |
| `--editor-font-size` | `15px` (user-configurable 10–32) | Obsidian default is `16px`. |

---

## 4. Radius Scale

Use Obsidian's scale. **Never** introduce a radius outside it.

| Class | Radius | Applies to |
|---|---|---|
| **S** | `4px` | Code, callouts, checkboxes, table selection, small chips |
| **M** | `8px` | Tree rows, menus, toolbar buttons, cards, buttons/inputs (Obsidian inputs are 5px) |
| **L** | `12px` | Modals, command palette, settings groups, tooltips |
| **XL** | `24px` (18px mobile) | Large previews / sheets |
| **Pill** | `9999px` | Tags, toggles, search fields, and on mobile: file rows, view selector, navbar, header buttons, inputs |
| **Circle** | `50%` | Floating / header icon buttons |

---

## 5. Interactive States

| State | Rule |
|---|---|
| **Hover** | Background → `--panel-2` (Obsidian: 6.7% overlay); text `--muted` → `--text`; accent → `--accent-hover` |
| **Selected** | Desktop: `--accent-dim` background, `--active-text`. Mobile file tree: `--panel-2` pill, `--text` |
| **Active tab** | Editor-colored tab with rounded top corners on the `--panel` bar; no accent underline |
| **Focus** | Inputs: 2px accent ring / border; buttons: `outline: 2px solid var(--accent)` |
| **Disabled** | `opacity: 0.4`, `cursor: not-allowed` |
| **Transition** | `70–140ms` for color/background; `150ms ease-in-out` for mobile controls; `300ms` for drawers and sheets |

---

## 6. Application Rules (for agents)

1. **Read tokens from `var(--…)` only.** No literal hex/rgba in `.svelte` components or new
   CSS — add a token here and to `app.css` instead.
2. **Every new token needs a dark and a light value**, plus a mobile override when Obsidian
   has one (§2.8).
3. **Keep the flat variable names** (`--bg`, `--panel`, `--accent`, …).
4. **Radius** must come from §4; sizes and spacing from the 4px grid in §2.6.
5. **Callouts** (`.cm-callout-*` in `app.css`) use the extended colors of §2.3: info/note blue,
   tip cyan, success green, warning orange, danger red, quote `--muted`. Set `--callout-color` on
   the variant; never hardcode a hex.
6. **When unsure how Obsidian styles something, check §2** — and if it is not there, read it
   from Obsidian's own `app.css` (inside `%APPDATA%\obsidian\obsidian-<version>.asar`), not from
   memory or third-party write-ups. Record the version when updating this file.

### Applied in components (2026-11-16)

- Radii moved to the §4 scale: rows, buttons, inputs and cards `8px`; modals and the command
  palette `12px`; code, callouts, math and small chips `4px`.
- Links use `--accent-text` and a solid underline; headings use Obsidian's sizes, weights and
  letter spacing; bold is 600; list markers are `--faint`; table headers are transparent.
- Blockquotes and note embeds use a `2px` accent left border on a transparent background.
- Callouts have no border; each variant sets `--callout-color`, used at 10% for the background
  and at full strength for the title.
- Android: the area behind the transparent system bars is painted by
  `NotiumAndroid.setSystemBarColor` (see `src/lib/androidBars.ts`): `--bg` normally, `--panel`
  while a drawer is open; it follows the in-app theme and flips the bar icon color.
- Inline `#tags` render as pills (`.cm-tag`, `--tag-bg`: accent 10%, 20% on mobile dark).
- The top bar has a `--bg` fade so scrolled text does not collide with the note title.
- Toggles are `40x22` on desktop and `52x30` on mobile, off-state `--pill-border`.

### Known gaps (not yet matching Obsidian)

- UI font (Poppins) and the 15px editor size are deliberate Notium choices.
- Desktop inputs are about 31px tall (Obsidian `30px`) with 8px radius (Obsidian `5px`).

---

## 7. Mobile Navigation & Backlinks

The mobile side panels follow §2.8 and the screenshots in `reference/`:

- Navigation and backlinks use full-width, flat `--panel` surfaces with safe-area-aware spacing.
- File rows: `16–17px`, at least `44px` tall, ellipsized titles. Folders keep only their
  disclosure chevron; notes show no type dots.
- The selected file row uses `--panel-2`, `--text` and a pill radius.
- Footer actions are quiet ghost icons (`--muted`, 24px) with touch targets of at least `44px`.
  The current view is shown in a large `--pill-bg` pill selector; alternate views open in an
  overlay menu on the same surface.
- Mobile backlinks include a local filter, sentence-case section headings, plain right-aligned
  counts and explicit muted empty states.
- Quick Settings remains a single circular action in the navigation footer. Cloud Sync,
  Attachments and Trash open as dedicated pages inside Settings.

---

## 8. Settings Navigation & Motion

- Settings opens on a searchable home screen with grouped option rows, muted outline icons,
  concise descriptions and disclosure chevrons. Its title stays centered between circular
  back/close controls.
- General, Cloud Sync, Attachments, Trash and Snippets/Guide are dedicated subpages. Attachments
  and Trash reuse their existing functionality embedded in the settings sheet.
- The settings overlay fades for `160ms`; the sheet travels `160px` from below over `320ms`
  with `cubicOut`, and reverses on close. Reduced-motion users get `0ms` transitions.
- On mobile, the sheet is capped at `88dvh` and accounts for bottom safe-area padding. Embedded
  tool pages keep independent scrolling.

---

## 9. Responsive Layout

One switch decides the layout: the `is-compact` class on `<html>`, set by
`src/lib/layout.svelte.ts`. Markup (`isDesktop` in `App.svelte`) and CSS both read it.

| Mode | When | UI |
|---|---|---|
| **Docked** | Desktop OS and window ≥ 768px | Ribbon, docked sidebar and inspector, centered dialogs, desktop palette |
| **Compact** | Phone/tablet OS at any size, or a desktop window < 768px | Overlay drawers, large touch targets, bottom-sheet settings, mobile palette (§2.8) |

Rules:

- Write compact styles as `:root.is-compact …`. Do **not** add `@media (max-width: 767px)`
  blocks or branch on the user agent for layout; `layout.mobilePlatform` is only for platform
  features (camera, native folder picker).
- Compact on wide screens (≥ 600px: tablets, landscape phones): drawers are `min(84vw, 420px)`
  side sheets and the settings sheet is capped at `640px`.
- Compact on short screens (height ≤ 500px): tighter drawer rows (44px) and paddings.
- Crossing the breakpoint resets the panels: sidebar open when docked, closed when compact.
- Preview any size in a browser with `npm run dev` (a dev-only fake backend loads; add
  `?mobile` to the URL for the phone/tablet UI).
