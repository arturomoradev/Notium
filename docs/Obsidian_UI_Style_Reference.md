# Obsidian UI Style — Complete Design Reference

> ⚠️ **Superseded — do not use these values.** A check against the real Obsidian 1.15.0
> stylesheet (2026-11-16) showed this document is inaccurate: the accent is purple `#8a5cf5`
> (not blue `#76a7f5`), the gray ramp, radii, heading sizes, ribbon/tab dimensions, icon set
> (Lucide, not Feather) and the mobile layout are all different. The verified values live in
> [`THEME_DATA.md`](./THEME_DATA.md) §2. This file is kept only for history.

> This document provides an exhaustive breakdown of the Obsidian note-taking app's visual design system, covering every aspect of its UI: colors, typography, icons, spacing, layout, components, and interaction patterns. Values are sourced from Obsidian's official `app.css`, developer documentation, and community theme analysis.

---

## Table of Contents

1. [Design Philosophy](#1-design-philosophy)
2. [Color System](#2-color-system)
3. [Typography](#3-typography)
4. [Icons](#4-icons)
5. [Spacing & Sizing](#5-spacing--sizing)
6. [Layout & Structure](#6-layout--structure)
7. [UI Components](#7-ui-components)
8. [Editor & Content Styling](#8-editor--content-styling)
9. [Dark Mode & Light Mode](#9-dark-mode--light-mode)
10. [Interactive States](#10-interactive-states)
11. [Graph View Styling](#11-graph-view-styling)
12. [Mobile UI](#12-mobile-ui)
13. [Accessibility](#13-accessibility)

---

## 1. Design Philosophy

Obsidian's UI follows a **minimalist, content-first** design philosophy:

- **Dark by default**: The default theme is dark, reducing visual noise so that linked notes, graph views, and Markdown content become the focal point.
- **System fonts**: Zero custom web fonts are loaded. The app defers entirely to the operating system's native typeface, prioritizing instant rendering over typographic branding.
- **Customizability as a first-class feature**: Hundreds of CSS custom properties (CSS variables) are exposed, organized by category, making every pixel of the app user-customizable. The default theme is intentionally just one opinion among thousands.
- **Local-first simplicity**: No loading states for cloud sync, no collaboration cursors, no presence indicators. The interface is simpler because the data model is simpler.
- **Sharp, crisp geometry**: Components average ~4px corner radius, favoring crisp rectangles over rounded, soft shapes.

---

## 2. Color System

Obsidian's color system is built entirely on CSS custom properties. The palette is organized into functional categories: backgrounds, text, interactive elements, modifiers, and accents.

### 2.1 Background Colors

| CSS Variable | Dark Mode Value | Light Mode Value | Purpose |
|---|---|---|---|
| `--background-primary` | `#1e1e1e` | `#ffffff` | Main editor and workspace background |
| `--background-primary-alt` | `#262626` | `#f6f6f6` | Alternate primary surface (e.g., hover rows) |
| `--background-secondary` | `#191919` | `#f2f2f2` | Sidebars, panels, and secondary areas |
| `--background-secondary-alt` | `#0c1018d1` (semi-transparent) | `#eaeaead1` | Tertiary surfaces, overlays |
| `--background-accent` | `--interactive-accent` | `--interactive-accent` | Accent-colored backgrounds (active items) |

### 2.2 Text Colors

| CSS Variable | Dark Mode Value | Light Mode Value | Purpose |
|---|---|---|---|
| `--text-normal` | `#dcddde` | `#1e1e1e` | Primary body text |
| `--text-muted` | `#999999` | `#888888` | Secondary text, metadata, timestamps |
| `--text-faint` | `#666666` | `#999999` | Tertiary text, placeholders, faint labels |
| `--text-accent` | `#76a7f5` | `#0050bf` | Accent-colored text (links, highlights) |
| `--text-accent-hover` | `#8bb8f7` | `#0060df` | Accent text on hover |
| `--text-error` | `#e95555` | `#c82828` | Error messages |
| `--text-error-hover` | `#f06565` | `#d43838` | Error text on hover |
| `--text-on-accent` | `#ffffff` | `#ffffff` | Text rendered on top of accent-colored backgrounds |
| `--text-selection` | `rgba(15, 75, 210, 0.33)` | `rgba(15, 75, 210, 0.2)` | Text selection highlight |
| `--bold-color` | `--text-normal` | `--text-normal` | Bold text color |
| `--italic-color` | `--text-normal` | `--text-normal` | Italic text color |

### 2.3 Interactive Element Colors

| CSS Variable | Dark Mode Value | Light Mode Value | Purpose |
|---|---|---|---|
| `--interactive-normal` | `#1e1e1e` | `#ffffff` | Default button, input, and interactive surface |
| `--interactive-hover` | `#262626` | `#f2f2f2` | Hover state for interactive elements |
| `--interactive-accent` | `#76a7f5` | `#0050bf` | Primary accent (buttons, active tabs, checkboxes) |
| `--interactive-accent-rgb` | `118, 167, 245` | `0, 80, 191` | RGB values of accent for use in rgba() |
| `--interactive-accent-hover` | `#8bb8f7` | `#0060df` | Accent hover state |
| `--interactive-before` | `#3e3e3e` | `#e0e0e0` | "Before" pseudo-element color (e.g., tree connectors) |

### 2.4 Background Modifiers

| CSS Variable | Dark Mode Value | Light Mode Value | Purpose |
|---|---|---|---|
| `--background-modifier-border` | `#333333` | `#e0e0e0` | Borders, dividers, hairlines |
| `--background-modifier-form-field` | `#191919` | `#ffffff` | Input fields, textareas |
| `--background-modifier-form-field-highlighted` | `#191919` | `#ffffff` | Focused input fields |
| `--background-modifier-box-shadow` | `rgba(0, 0, 0, 0.3)` | `rgba(0, 0, 0, 0.1)` | Drop shadows |
| `--background-modifier-success` | `#1b8043` | `#0f9d58` | Success indicators (green) |
| `--background-modifier-error` | `#883935` | `#c82828` | Error backgrounds (red) |
| `--background-modifier-error-hover` | `#944242` | `#d43838` | Error hover (red) |
| `--background-modifier-cover` | `rgba(0, 0, 0, 0.5)` | `rgba(0, 0, 0, 0.2)` | Overlay covers |

### 2.5 Highlight & Mark Colors

| CSS Variable | Dark Mode Value | Light Mode Value | Purpose |
|---|---|---|---|
| `--text-highlight-bg` | `rgba(255, 208, 0, 0.2)` | `rgba(245, 200, 0, 0.4)` | ==Highlight== background (yellow) |
| `--text-highlight-bg-active` | `rgba(255, 208, 0, 0.4)` | `rgba(245, 200, 0, 0.6)` | Active highlight background |
| `--highlight-mix-blend-mode` | `lighten` | `darken` | Blend mode for highlights |

### 2.6 Scrollbar Colors

| CSS Variable | Dark Mode Value | Light Mode Value | Purpose |
|---|---|---|---|
| `--scrollbar-bg` | `rgba(255, 255, 255, 0.05)` | `rgba(0, 0, 0, 0.05)` | Scrollbar track background |
| `--scrollbar-thumb-bg` | `rgba(255, 255, 255, 0.1)` | `rgba(0, 0, 0, 0.1)` | Scrollbar thumb |
| `--scrollbar-active-thumb-bg` | `rgba(255, 255, 255, 0.2)` | `rgba(0, 0, 0, 0.2)` | Scrollbar thumb on hover/active |

### 2.7 Code & Inline Code Colors

| CSS Variable | Dark Mode Value | Light Mode Value | Purpose |
|---|---|---|---|
| `--code-background` | `#262626` | `#f2f2f2` | Code block background |
| `--code-normal` | `#dcddde` | `#1e1e1e` | Code text color |

### 2.8 Tag Colors

Tags use a muted palette with subtle backgrounds:

| CSS Variable | Dark Mode Value | Light Mode Value | Purpose |
|---|---|---|---|
| `--tag-color` | (inherits `--text-accent`) | (inherits `--text-accent`) | Tag text color |
| `--tag-background` | `rgba(118, 167, 245, 0.15)` | `rgba(0, 80, 191, 0.1)` | Tag background pill |
| `--tag-hover` | `rgba(118, 167, 245, 0.25)` | `rgba(0, 80, 191, 0.15)` | Tag on hover |

### 2.9 Link Colors

| CSS Variable | Dark Mode Value | Light Mode Value | Purpose |
|---|---|---|---|
| `--link-color` | (inherits `--text-accent`) | (inherits `--text-accent`) | Unvisited internal links |
| `--link-color-active` | `--interactive-accent-hover` | `--interactive-accent-hover` | Active/open link |
| `--link-decoration` | `none` | `none` | Link underline (default: none) |
| `--link-decoration-hover` | `underline` | `underline` | Link underline on hover |
| `--link-external-decoration` | `underline` | `underline` | External link underline |

### 2.10 Callout Colors

Obsidian's callouts (admonitions) use an 8-color harmony system:

| Callout Type | Color Variable | Hex Value |
|---|---|---|
| Note | `--color-blue` | `#027aff` |
| Abstract/Summary/Tldr | `--color-cyan` | `#53dfdd` |
| Tip/Important/Info | `--color-cyan` | `#53dfdd` |
| Success/Check/Done | `--color-green` | `#44cf6e` |
| Question/Help/Help/Question | `--color-yellow` | `#e0de71` |
| Failure/Fail/Missing | `--color-orange` | `#e9973f` |
| Danger/Error | `--color-red` | `#fb464c` |
| Bug | `--color-red` | `#fb464c` |
| Example | `--color-pink` | `#fa99cd` |
| Quote/Cite | `--text-muted` | `#999999` |

Each callout uses its color for the left border accent, the icon, and a subtle background tint (via blend modes).

### 2.11 Brand & Marketing Colors

Obsidian's marketing/brand identity uses a distinct palette:

| Role | Hex Value | Usage |
|---|---|---|
| Primary (Brand Violet) | `#a78bfa` | CTAs, primary buttons, brand accent |
| On Primary | `#ffffff` | Text on brand violet |
| Background | `#1f1f1f` | Page background |
| Surface | `#262626` | Card and component surfaces |

---

## 3. Typography

### 3.1 Font Families

Obsidian uses **system fonts exclusively** — no custom web fonts, no Google Fonts requests, no web-font payload:

```css
--font-interface-8: "ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, Inter, "Helvetica Neue", Arial, "Noto Sans", sans-serif";
--font-text-8: "ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, Inter, "Helvetica Neue", Arial, "Noto Sans", sans-serif";
--font-monospace-8: "ui-monospace, "Cascadia Code", "JetBrains Mono", "SF Mono", "Fira Code", "Fira Mono", "Roboto Mono", Consolas, "Courier New", monospace";
```

**Platform-specific rendering:**
- **macOS**: San Francisco (system-ui → -apple-system)
- **Windows**: Segoe UI (system-ui → "Segoe UI")
- **Android**: Roboto
- **Linux**: Inter / Helvetica Neue / Noto Sans (fallback chain)

Three font roles are defined:
- `--font-interface-theme`: UI elements (menus, settings, sidebars)
- `--font-text-theme`: Editor text content
- `--font-monospace-theme`: Code blocks and inline code

### 3.2 Font Sizes

Obsidian uses both **relative** (editor) and **fixed** (UI) font sizes:

#### Editor Font Sizes (Relative)

| CSS Variable | Default Value | Usage |
|---|---|---|
| `--font-text-size` | `16px` | Base editor font size (user-configurable in Appearance settings) |
| `--font-smallest` | `0.8em` | Smallest text (footnotes, captions) |
| `--font-smaller` | `0.875em` | Smaller auxiliary text |
| `--font-small` | `0.933em` | Small body text |

#### UI Font Sizes (Fixed)

| CSS Variable | Default Value | Usage |
|---|---|---|
| `--font-ui-smaller` | `12px` | Tiny labels, timestamps |
| `--font-ui-small` | `13px` | Secondary UI text |
| `--font-ui-medium` | `15px` | Standard UI text |
| `--font-ui-large` | `20px` | UI headings |

#### Heading Sizes

| Element | Size (relative to `--font-text-size`) |
|---|---|
| H1 | `2em` (~32px) |
| H2 | `1.6em` (~25.6px) |
| H3 | `1.37em` (~21.9px) |
| H4 | `1.25em` (~20px) |
| H5 | `1.12em` (~17.9px) |
| H6 | `1.12em` (~17.9px) |

### 3.3 Font Weights

| CSS Variable | Value |
|---|---|
| `--font-thin` | `100` |
| `--font-extralight` | `200` |
| `--font-light` | `300` |
| `--font-normal` | `400` |
| `--font-medium` | `500` |
| `--font-semibold` | `600` |
| `--font-bold` | `700` |
| `--font-extrabold` | `800` |
| `--font-black` | `900` |

- `--font-weight`: Regular text weight (default: `400`)
- `--bold-modifier`: Added weight for bolded text (recommended: `100-300`)
- `--bold-weight`: Computed bold text weight (`--font-weight` + `--bold-modifier`)

### 3.4 Line Heights

| CSS Variable | Default Value | Usage |
|---|---|---|
| `--line-height-normal` | `1.5` | Default line height for body text |
| `--line-height-tight` | `1.3` | Search results, tree items, tooltips, compact spaces |

### 3.5 Paragraph & Heading Spacing

| CSS Variable | Purpose |
|---|---|
| `--heading-spacing` | Spacing above headings (varies by heading level) |
| `--p-spacing` | Spacing between paragraphs |

### 3.6 Text Formatting

- **Bold**: Uses `--bold-weight` (font-weight stacking via `--bold-modifier`)
- **Italic**: Uses `--italic-color` for color, standard `font-style: italic`
- **Strikethrough**: Standard `text-decoration: line-through`
- **Inline code**: Monospace font, `--code-background` background, `--code-normal` text color
- **Links**: Accent color, no underline by default, underline on hover

---

## 4. Icons

### 4.1 Icon Library

Obsidian uses **Feather Icons** as its primary icon library. Feather Icons is a community-driven icon set featuring:
- Simple, 2px stroke outlines
- Consistent 24×24 viewBox grid
- Minimalist, geometric style
- ~280+ icons covering common UI actions

Additionally, Obsidian's internal icon set extends Feather with custom icons specific to the app's features (e.g., graph view, backlinks, canvas).

### 4.2 Icon Usage in the UI

| Location | Icon Style | Examples |
|---|---|---|
| **Ribbon (left bar)** | Feather-style, 20×20px, single-color | File explorer, search, calendar, graph view, command palette, settings |
| **Tab headers** | Feather-style, 16×16px | Note icon, search icon, canvas icon, graph icon |
| **Toolbar buttons** | Feather-style, 16×16px | Bold, italic, link, list, code block, heading |
| **File tree** | Feather-style, 14×14px | Folder (open/closed), file type icons, chevron (expand/collapse) |
| **Settings menu** | Feather-style, 16×16px | Toggle switches, section icons |
| **Context menus** | Feather-style, 16×16px | Edit, delete, rename, duplicate, link |
| **Callout icons** | Feather-style, 16×16px | Info, warning, check, x, alert triangle |

### 4.3 Icon Colors

Icons inherit the text color of their container:

| Context | Color Variable |
|---|---|
| Ribbon icons (default) | `--text-muted` |
| Ribbon icons (active) | `--text-accent` |
| Ribbon icons (hover) | `--text-normal` |
| Tab icons | `--text-muted` |
| Button icons | `--text-normal` |
| Tree item icons | `--text-muted` |

### 4.4 Icon Customization

The community provides extensive icon customization through plugins:
- **Custom Sidebar Icons**: Assign unique icons from Lucide/Iconify catalog to files, folders, and ribbon items
- **Icon Shortcodes**: Embed icons directly in notes using shortcode syntax
- Built-in support for SVG tags and Base64-encoded images

---

## 5. Spacing & Sizing

### 5.1 Spacing Scale

Obsidian uses a consistent spacing scale based on 4px increments:

| Token | Value | Usage |
|---|---|---|
| `--size-2` | `2px` | Micro-spacing (icon gaps) |
| `--size-4` | `4px` | Tight spacing (padding within small elements) |
| `--size-8` | `8px` | Standard padding (buttons, small gaps) |
| `--size-16` | `16px` | Section padding, standard margins |
| `--size-24` | `24px` | Large section gaps |
| `--size-32` | `32px` | Major layout divisions |

### 5.2 Border Radius

Obsidian favors **sharp, minimal border radius**:

| Element | Border Radius |
|---|---|
| Buttons | `4px` |
| Input fields | `4px` |
| Modal dialogs | `8px` |
| Tags | `0.5em` (pill-like) |
| Cards/panels | `4-6px` |
| Code blocks | `4px` |
| Callouts | `4px` |
| Default base radius | `4px` |

### 5.3 Component Sizing

| Component | Width | Height |
|---|---|---|
| Ribbon (icon bar) | `56px` | Full viewport height |
| Left sidebar (collapsed) | `56px` (ribbon only) | Full viewport height |
| Left sidebar (expanded) | `280px` (default, resizable) | Full viewport height |
| Right sidebar (expanded) | `280px` (default, resizable) | Full viewport height |
| Tab bar | Full width | `36px` |
| Status bar | Full width | `24px` |
| Modal dialog | `600px` (max) | Auto (max ~80vh) |
| Settings modal | `700px` (max) | Auto (max ~90vh) |

### 5.4 Content Width

| Variable | Default | Purpose |
|---|---|---|
| `--file-line-width` | `700px` | Max width of editor content line |
| `--file-line-width-padding` | `4px` | Padding around content width |
| `--file-margins` | `0px` | Editor margins |

---

## 6. Layout & Structure

### 6.1 Workspace Layout

Obsidian's workspace follows a **three-column layout**:

```
┌──────────────────────────────────────────────────────┐
│                    Title Bar                         │
├──────┬───────────────────────────────┬───────────────┤
│      │       Tab Bar                 │               │
│Ribbon│  [Note 1] [Note 2] [Canvas]   │  Right Bar    │
│      ├───────────────────────────────┤               │
│      │                               │               │
│ 56px │     Editor / Content Area     │  280px        │
│      │                               │  (resizable)  │
│      │                               │               │
│      │                               │               │
├──────┴───────────────────────────────┴───────────────┤
│                    Status Bar                        │
└──────────────────────────────────────────────────────┘
```

### 6.2 Layout Components

1. **Title Bar** (top)
   - Window controls (minimize, maximize, close)
   - Vault name
   - File name of active note
   - Height: ~32px

2. **Ribbon** (leftmost vertical bar)
   - Width: 56px
   - Contains icon buttons for core plugins
   - Default icons: File Explorer, Search, Today (calendar), Random note, Graph View, Command Palette, Settings
   - Background: `--background-secondary`
   - Icons spaced vertically with ~8px gaps

3. **Left Sidebar** (expandable)
   - Width: 280px (default, user-resizable via drag)
   - Contains: File Explorer tree, Search results, Backlinks, Outgoing Links, Bookmarks, Tags, Outline
   - Background: `--background-secondary`
   - Tab-based navigation within sidebar

4. **Center Content Area** (main workspace)
   - Flexible width (fills remaining space)
   - Contains: Editor tabs, Graph View, Canvas, Settings
   - Background: `--background-primary`
   - Supports split views (horizontal and vertical)

5. **Right Sidebar** (expandable)
   - Width: 280px (default, user-resizable)
   - Contains: Backlinks, Outgoing Links, Outline, Bookmarks
   - Background: `--background-secondary`

6. **Status Bar** (bottom)
   - Height: 24px
   - Shows: Word count, cursor position, zoom level, file name
   - Background: `--background-secondary`
   - Text: `--text-muted`, font-size: `--font-ui-smaller` (12px)

### 6.3 Split Views

The center area supports **split panes**:
- Vertical splits (side-by-side notes)
- Horizontal splits (stacked notes)
- Each split maintains its own tab bar
- Split divider: 4px wide, `--background-modifier-border` color

### 6.4 Sidebar Dividers

- Sidebar-to-content divider: 2px wide drag handle
- Color: `--background-modifier-border`
- Hover state: `--interactive-accent`

---

## 7. UI Components

### 7.1 Buttons

| Style | Background | Text Color | Border | Radius |
|---|---|---|---|---|
| **Primary** | `--interactive-accent` | `--text-on-accent` | None | `4px` |
| **Secondary** | `--interactive-normal` | `--text-normal` | `--background-modifier-border` | `4px` |
| **Danger** | `--background-modifier-error` | `--text-on-accent` | None | `4px` |
| **Ghost/Text** | Transparent | `--text-accent` | None | `0px` |

Button hover states use `--interactive-hover` for background and `--interactive-accent-hover` for accent buttons.

### 7.2 Input Fields

- Background: `--background-modifier-form-field`
- Border: `1px solid --background-modifier-border`
- Border (focused): `1px solid --interactive-accent`
- Text color: `--text-normal`
- Placeholder: `--text-faint`
- Padding: `8px`
- Border radius: `4px`
- Font size: `--font-ui-medium` (15px)

### 7.3 Toggle Switches

- Off state: `--background-modifier-border` (track), `--text-muted` (thumb)
- On state: `--interactive-accent` (track), `--text-on-accent` (thumb)
- Size: 36px × 20px (track), 16px (thumb)
- Border radius: 10px (pill shape)

### 7.4 Modals & Dialogs

- Background: `--background-primary`
- Border: `1px solid --background-modifier-border`
- Border radius: `8px`
- Box shadow: `--background-modifier-box-shadow`
- Overlay: `--background-modifier-cover`
- Max width: `600-700px`
- Padding: `24px`

### 7.5 Tabs

| State | Background | Text Color | Border |
|---|---|---|---|
| **Active** | `--background-primary` | `--text-normal` | Bottom: `2px --interactive-accent` |
| **Inactive** | `--background-secondary` | `--text-muted` | None |
| **Hover** | `--background-primary-alt` | `--text-normal` | None |

- Tab height: `36px`
- Tab close button: appears on hover, `--text-muted` color
- Tab bar background: `--background-secondary`

### 7.6 Dropdowns & Select Menus

- Background: `--background-primary`
- Border: `1px solid --background-modifier-border`
- Border radius: `4px`
- Item padding: `8px 12px`
- Hover item: `--interactive-hover` background
- Selected item: `--interactive-accent` left border accent

### 7.7 Tooltips

- Background: `--background-primary`
- Border: `1px solid --background-modifier-border`
- Text: `--text-normal`, `--font-ui-small` (13px)
- Padding: `4px 8px`
- Border radius: `4px`
- Box shadow: `--background-modifier-box-shadow`
- Line height: `--line-height-tight` (1.3)

### 7.8 Notices & Toasts

- Background: `--interactive-accent`
- Text: `--text-on-accent`
- Border radius: `4px`
- Padding: `8px 16px`
- Position: Top-right of workspace
- Auto-dismiss: 5 seconds

### 7.9 Tree Views (File Explorer)

- Item height: `24px`
- Padding: `4px 8px`
- Indentation: `16px` per level
- Chevron icon: 14×14px, `--text-muted`
- Folder icon: 14×14px, `--text-muted`
- File icon: 14×14px, `--text-muted`
- Active item: `--background-modifier-border` background
- Hover item: `--interactive-hover` background
- Connector lines: `--interactive-before`

### 7.10 Search Results

- Result item padding: `8px 12px`
- Match highlight: `--text-highlight-bg`
- Title font: `--font-ui-medium` (15px), `--text-normal`
- Snippet font: `--font-ui-small` (13px), `--text-muted`
- Divider between results: `--background-modifier-border`

---

## 8. Editor & Content Styling

### 8.1 Editor Area

- Background: `--background-primary`
- Font: `--font-text-theme`
- Font size: `--font-text-size` (16px, user-configurable)
- Line height: `--line-height-normal` (1.5)
- Max content width: `--file-line-width` (700px)
- Cursor color: `--text-normal`
- Selection color: `--text-selection`

### 8.2 Headings in Editor

| Level | Font Size | Font Weight | Color Variable | Spacing |
|---|---|---|---|---|
| H1 | `2em` | `600` (semibold) | `--h1-color` | `--heading-spacing` (largest) |
| H2 | `1.6em` | `600` | `--h2-color` | Large |
| H3 | `1.37em` | `600` | `--h3-color` | Medium |
| H4 | `1.25em` | `600` | `--h4-color` | Small |
| H5 | `1.12em` | `600` | `--h5-color` | Small |
| H6 | `1.12em` | `600` | `--h6-color` | Small |

Default heading colors inherit `--text-normal` but can be overridden by themes.

### 8.3 Blockquotes

- Left border: `4px solid --blockquote-border`
- `--blockquote-border`: `rgba(77, 60, 166, 0.6)` (default)
- Background: Slight tint of border color
- Padding: `4px 8px` (left)
- Text color: `--text-muted`

### 8.4 Code Blocks

- Background: `--code-background` (`#262626` dark / `#f2f2f2` light)
- Text color: `--code-normal`
- Font: `--font-monospace-theme`
- Font size: `--font-smaller` (0.875em)
- Padding: `16px`
- Border radius: `4px`
- Line height: `1.5`
- Overflow: Horizontal scroll

### 8.5 Inline Code

- Background: `--code-background` (subtle)
- Text color: `--code-normal`
- Font: `--font-monospace-theme`
- Padding: `2px 4px`
- Border radius: `3px`

### 8.6 Lists

- Bullet style: Disc (unordered), decimal (ordered)
- Bullet color: `--text-muted`
- List item padding: `2px 0`
- Indentation: `2em`
- Nested list marker: varies (disc → circle → square)

### 8.7 Task Lists (Checkboxes)

- Unchecked: Empty square, border: `--background-modifier-border`
- Checked: Filled with `--interactive-accent`, checkmark in `--text-on-accent`
- Size: `14px × 14px`
- Border radius: `3px`
- Text after checkbox: `--text-normal` (unchecked) / `--text-muted` with `text-decoration: line-through` (checked)

### 8.8 Tables

- Border: `1px solid --background-modifier-border`
- Header background: `--background-primary-alt`
- Cell padding: `6px 12px`
- Font size: `--font-small` (0.933em)
- Alternating row backgrounds: subtle `--background-primary-alt`

### 8.9 Horizontal Rules

- Color: `--background-modifier-border`
- Thickness: `2px`
- Margin: `24px 0`

### 8.10 Footnotes

- Reference marker: `--text-accent`, superscript
- Footnote text: `--font-smaller`, `--text-muted`
- Separator line: `--background-modifier-border`

### 8.11 Callouts (Admonitions)

Callouts are styled boxes with icons, titles, and content:

- Background: Subtle tint of the callout color (via blend mode)
- Left border: `4px solid` of callout color
- Icon: 16×16px, callout color
- Title: `--font-ui-medium` (15px), bold, callout color
- Content: `--font-text-size`, `--text-normal`
- Padding: `12px 16px`
- Border radius: `4px`
- Margin: `8px 0`
- Collapsible: Yes (chevron icon on title)

---

## 9. Dark Mode & Light Mode

### 9.1 Theme Switching

Obsidian supports two themes via CSS class toggling on the `<body>` element:
- `.theme-dark`: Dark mode (default)
- `.theme-light`: Light mode

Switching is done via Settings → Appearance → Base Theme.

### 9.2 Dark Mode Palette Summary

```
Backgrounds:  #1e1e1e (primary), #262626 (alt), #191919 (secondary)
Text:         #dcddde (normal), #999999 (muted), #666666 (faint)
Accent:       #76a7f5 (blue)
Interactive:  #1e1e1e (normal), #262626 (hover), #76a7f5 (accent)
Borders:      #333333
Success:      #1b8043
Error:        #883935
Code:         #262626 background, #dcddde text
```

### 9.3 Light Mode Palette Summary

```
Backgrounds:  #ffffff (primary), #f6f6f6 (alt), #f2f2f2 (secondary)
Text:         #1e1e1e (normal), #888888 (muted), #999999 (faint)
Accent:       #0050bf (blue)
Interactive:  #ffffff (normal), #f2f2f2 (hover), #0050bf (accent)
Borders:      #e0e0e0
Success:      #0f9d58
Error:        #c82828
Code:         #f2f2f2 background, #1e1e1e text
```

### 9.4 Obsidian.md Website Design Tokens

The marketing site uses a distinct design system:

| Token | Value |
|---|---|
| Primary color | `#a78bfa` (violet) |
| Typeface | `ui-sans-serif` (system font stack) |
| Shape language | Sharp |
| Base radius | `6px` |
| Colors | 19 total tokens |
| Type tokens | 12 tokens (11px to 60px, weights 400–600) |
| Spacing values | 7 values |

Display headlines: `60px/600` weight with `-1.2px` letter-spacing and `1.0` line-height.

---

## 10. Interactive States

### 10.1 Hover States

| Element | Hover Effect |
|---|---|
| Buttons | Background → `--interactive-hover` or `--interactive-accent-hover` |
| Links | Underline, color → `--text-accent-hover` |
| Tree items | Background → `--interactive-hover` |
| Tabs | Background → `--background-primary-alt` |
| Table rows | Background → `--interactive-hover` |
| Sidebar items | Background → `--interactive-hover` |
| File tree items | Background → `--interactive-hover` |

### 10.2 Active/Selected States

| Element | Active Effect |
|---|---|
| Buttons | Background → `--interactive-accent`, slight scale-down |
| Tree items | Background → `--background-modifier-border` |
| Tabs | Bottom border: `2px --interactive-accent` |
| Ribbon icons | Color → `--text-accent` |
| Checkboxes | Filled with `--interactive-accent` |
| Radio buttons | Inner dot: `--interactive-accent` |

### 10.3 Focus States

| Element | Focus Effect |
|---|---|
| Input fields | Border → `--interactive-accent` |
| Buttons | Outline: `2px --interactive-accent` (offset: 2px) |
| Tabs | Same as active state |
| Selectable items | Background → `--interactive-hover` |

### 10.4 Disabled States

| Element | Disabled Effect |
|---|---|
| Buttons | Opacity: `0.4`, cursor: `not-allowed` |
| Inputs | Opacity: `0.5`, cursor: `not-allowed` |
| Links | Color: `--text-faint`, pointer-events: `none` |

### 10.5 Transitions & Animations

Obsidian uses subtle, fast transitions:
- Default transition: `150ms ease`
- Sidebar expand/collapse: `200ms ease`
- Modal open/close: `150ms ease`
- Hover effects: `100ms ease`
- Tab switching: Instant (no animation)

---

## 11. Graph View Styling

The Graph View is a signature Obsidian feature with its own color system:

| CSS Variable | Purpose |
|---|---|
| `--graph-background` | Graph canvas background (inherits `--background-primary`) |
| `--graph-line` | Connection line color (inherits `--background-modifier-border`) |
| `--graph-node` | Default node color (inherits `--text-normal`) |
| `--graph-node-unresolved` | Unresolved link node (inherits `--text-faint`) |
| `--graph-node-tag` | Tag node color (inherits `--text-accent`) |
| `--graph-node-attachment` | Attachment node color |
| `--graph-node-focus` | Focused/hovered node (inherits `--interactive-accent`) |
| `--graph-line-focus` | Focused/hovered line (inherits `--interactive-accent`) |

Graph nodes are rendered as circles (~8px radius default), with labels appearing on hover or at higher zoom levels.

---

## 12. Mobile UI

### 12.1 Mobile Layout

On mobile (iOS/Android), the layout adapts:
- **Ribbon**: Bottom navigation bar (56px height)
- **Sidebars**: Full-screen overlay drawers (slide from left/right)
- **Editor**: Full-width, full-height
- **Status bar**: Hidden or minimal
- **Tab bar**: Hidden (single-note focus)

### 12.2 Mobile-Specific Adjustments

- Touch targets: Minimum `44×44px` (iOS HIG) / `48×48px` (Material Design)
- Font sizes: Slightly larger for readability on small screens
- Sidebar overlay: Semi-transparent backdrop (`--background-modifier-cover`)
- Swipe gestures: Swipe left/right to open sidebars

### 12.3 Mobile Feature Parity

The mobile app includes near feature-parity with desktop:
- Full editing (Markdown, Live Preview, Reading view)
- Backlinks and outgoing links
- Graph view
- Canvas
- Community plugins
- Properties and frontmatter
- Search and filtering

---

## 13. Accessibility

### 13.1 Contrast Ratios

Obsidian's default themes aim for WCAG AA compliance:
- Normal text on primary background: ~12:1 (dark), ~12:1 (light) — exceeds AA (4.5:1)
- Muted text on primary background: ~5.5:1 (dark), ~4.5:1 (light) — meets AA
- Faint text on primary background: ~3.5:1 (dark) — may not meet AA for small text

### 13.2 Keyboard Navigation

- Full keyboard navigation support
- Tab key moves focus between interactive elements
- Arrow keys navigate tree views and lists
- Enter/Space activates buttons and selects items
- Escape closes modals and dropdowns

### 13.3 Screen Reader Support

- Semantic HTML elements where applicable
- ARIA labels on icon-only buttons (ribbon)
- Proper heading hierarchy in settings and modals

---

## Appendix: Complete CSS Variable Reference

The following is a consolidated list of all major Obsidian CSS custom properties for quick reference:

### Background Variables
```css
--background-primary
--background-primary-alt
--background-secondary
--background-secondary-alt
--background-accent
--background-modifier-border
--background-modifier-form-field
--background-modifier-form-field-highlighted
--background-modifier-box-shadow
--background-modifier-success
--background-modifier-error
--background-modifier-error-hover
--background-modifier-cover
```

### Text Variables
```css
--text-normal
--text-muted
--text-faint
--text-accent
--text-accent-hover
--text-error
--text-error-hover
--text-highlight-bg
--text-highlight-bg-active
--text-selection
--text-on-accent
--bold-color
--italic-color
```

### Interactive Variables
```css
--interactive-normal
--interactive-hover
--interactive-accent
--interactive-accent-rgb
--interactive-accent-hover
--interactive-before
```

### Typography Variables
```css
--font-text-size
--font-smallest
--font-smaller
--font-small
--font-ui-smaller
--font-ui-small
--font-ui-medium
--font-ui-large
--font-weight
--bold-modifier
--bold-weight
--line-height-normal
--line-height-tight
--font-interface-8
--font-text-8
--font-monospace-8
```

### Scrollbar Variables
```css
--scrollbar-bg
--scrollbar-thumb-bg
--scrollbar-active-thumb-bg
```

### Code Variables
```css
--code-background
--code-normal
```

### Layout Variables
```css
--file-line-width
--file-line-width-padding
--file-margins
--heading-spacing
--p-spacing
```

---

*This document was compiled from Obsidian's official `app.css`, developer documentation (obsidian-developer-docs GitHub repository), community theme analysis, and the Obsidian help center. Values reflect the default Obsidian theme as of version 1.x.*
