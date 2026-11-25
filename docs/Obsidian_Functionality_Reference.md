# Obsidian Functionality — Complete Feature Reference

> This document provides an exhaustive breakdown of the Obsidian note-taking app's functionality, covering every core feature, editing mode, plugin system, data model, and workflow capability. Information is sourced from Obsidian's official help center, developer documentation, Wikipedia, and community resources.

---

## Table of Contents

1. [Overview & Architecture](#1-overview--architecture)
2. [Data Model & Storage](#2-data-model--storage)
3. [Vaults](#3-vaults)
4. [Editing & Formatting](#4-editing--formatting)
5. [Views & Editing Modes](#5-views--editing-modes)
6. [Linking & References](#6-linking--references)
7. [Tags](#7-tags)
8. [Properties & Frontmatter](#8-properties--frontmatter)
9. [Core Plugins](#9-core-plugins)
10. [Graph View](#10-graph-view)
11. [Canvas](#11-canvas)
12. [Search](#12-search)
13. [Community Plugins](#13-community-plugins)
14. [Themes & Customization](#14-themes--customization)
15. [Bases](#15-bases)
16. [Attachments & Embedding](#16-attachments--embedding)
17. [Callouts](#17-callouts)
18. [Tasks](#18-tasks)
19. [Keyboard Shortcuts & Command Palette](#19-keyboard-shortcuts--command-palette)
20. [Obsidian Sync & Premium Services](#20-obsidian-sync--premium-services)
21. [Obsidian Publish](#21-obsidian-publish)
22. [Mobile App](#22-mobile-app)
23. [API & Plugin Development](#23-api--plugin-development)
24. [Import & Export](#24-import--export)
25. [Security & Privacy](#25-security--privacy)

---

## 1. Overview & Architecture

### 1.1 What is Obsidian?

Obsidian is a **personal knowledge base** and **note-taking application** built on top of a local folder of plain Markdown files. Developed by **Dynalist Inc.** (Shida Li and Erica Xu), it was first released on **March 30, 2020**. The latest stable release is **1.13.6** (August 2026).

**Key characteristics:**
- **Local-first**: All notes are stored as `.md` (Markdown) files in a local folder
- **Offline-first**: Full functionality without internet connection
- **Cross-platform**: Windows, macOS, Linux, iOS, Android
- **Extensible**: Rich plugin ecosystem (core + community)
- **Themeable**: 2,000+ community themes
- **Proprietary software**: Free for personal use, paid plans for sync/publish

### 1.2 Technical Stack

- **Built with**: JavaScript, HTML, CSS (Electron-based desktop app)
- **Available in**: 46 languages
- **Rendering**: Web technologies (Chromium engine via Electron)
- **Mobile**: Native wrappers around web technology stack

### 1.3 Design Principles

1. **Your data, your way**: Notes are plain Markdown files you own and control
2. **Connectivity over hierarchy**: Emphasizes linking between notes rather than folder-based organization
3. **Composability**: Features combine and extend through plugins
4. **Privacy-first**: Local storage by default; optional encrypted sync
5. **Customizability**: Every aspect of the UI and behavior can be modified

---

## 2. Data Model & Storage

### 2.1 File Format

- **Primary format**: Markdown (`.md`)
- **Extended syntax**: Obsidian Flavored Markdown (OFM) — extends standard Markdown with features like wikilinks, transclusions, and callouts
- **Attachments**: Stored as separate files in the vault (images, PDFs, audio, video, etc.)
- **Frontmatter**: YAML front matter for structured metadata (properties)

### 2.2 Storage Structure

```
My Vault/
├── .obsidian/
│   ├── app.json              # App settings
│   ├── appearance.json       # Theme and appearance settings
│   ├── community-plugins.json # Enabled community plugins
│   ├── core-plugins.json     # Enabled core plugins
│   ├── hotkeys.json          # Keyboard shortcuts
│   ├── plugins/              # Community plugin data
│   │   └── plugin-name/
│   │       └── data.json
│   ├── snippets/             # Custom CSS snippets
│   │   └── snippet.css
│   └── themes/               # Installed themes
│       └── theme-name/
│           └── theme.css
├── Attachments/              # Default attachment folder
│   └── image.png
├── Notes/
│   ├── Note 1.md
│   ├── Note 2.md
│   └── Subfolder/
│       └── Note 3.md
└── Daily Notes/
    ├── 2024-01-01.md
    └── 2024-01-02.md
```

### 2.3 The `.obsidian` Folder

The `.obsidian` folder stores all vault-specific settings, plugins, themes, and configurations:
- **app.json**: Editor preferences, tab behavior, file handling
- **appearance.json**: Theme, accent color, font size, CSS snippets
- **core-plugins.json**: Which core plugins are enabled/disabled
- **community-plugins.json**: Installed community plugins
- **hotkeys.json**: Custom keyboard shortcuts
- **plugins/**: Per-plugin data and settings
- **snippets/**: User-created CSS snippets
- **themes/**: Downloaded community themes

### 2.4 File Recovery

Obsidian automatically creates **snapshots** of your notes for recovery:
- Default: Saves snapshots every 15 minutes (configurable)
- Snapshots stored in `.obsidian/recovery/`
- Accessible via the File Recovery core plugin
- Allows restoring to previous versions of individual notes

---

## 3. Vaults

### 3.1 What is a Vault?

A **vault** is Obsidian's term for a user's knowledge base — essentially a folder on your filesystem containing Markdown files and attachments. Each vault is independent with its own settings, plugins, and themes.

### 3.2 Vault Operations

- **Create**: Create a new vault from an empty or existing folder
- **Open**: Open an existing vault (folder with `.md` files)
- **Safe mode**: Open a vault with all community plugins disabled (for troubleshooting)
- **Multiple vaults**: Switch between multiple vaults via the splash screen or quick switcher

### 3.3 Vault Settings

Each vault has independent settings for:
- Editor behavior (line breaks, tab size, auto-save)
- File and folder handling (default location for new notes, attachment folder)
- Appearance (theme, font, accent color)
- Enabled plugins (core and community)
- Keyboard shortcuts

---

## 4. Editing & Formatting

### 4.1 Basic Markdown Formatting

Obsidian supports all standard Markdown syntax:

#### Headings
```markdown
# Heading 1
## Heading 2
### Heading 3
#### Heading 4
##### Heading 5
###### Heading 6
```

#### Text Formatting
- **Bold**: `**text**` or `__text__`
- *Italic*: `*text*` or `_text_`
- ~~Strikethrough~~: `~~text~~`
- `Inline code`: `` `code` ``
- Highlight: `==text==` (Obsidian extension)

#### Paragraphs
- Separated by blank lines
- Single `Enter` = soft line break (continues paragraph)
- Double `Enter` = new paragraph
- **Strict line breaks** option: Single `Enter` creates hard line break (`<br>`)

#### Lists
```markdown
- Unordered item
  - Nested item
1. Ordered item
   1. Nested ordered item
- [ ] Task item (unchecked)
- [x] Task item (checked)
```

#### Blockquotes
```markdown
> Quoted text
> > Nested quote
```

#### Code Blocks
````markdown
```javascript
function hello() {
  console.log("Hello, World!");
}
```
````

#### Horizontal Rules
```markdown
---
***
___
```

#### Tables
```markdown
| Column 1 | Column 2 |
|----------|----------|
| Cell A   | Cell B   |
```

#### Footnotes
```markdown
Text with footnote[^1]

[^1]: Footnote content
```

Inline footnotes: `Text^[inline footnote content]`

#### Comments
```markdown
%%This is a comment — visible only in Source mode%%
```

### 4.2 Obsidian Flavored Markdown (OFM)

Obsidian extends standard Markdown with several proprietary features:

#### Wikilinks
```markdown
[[Note Name]]              # Link to note
[[Note Name#Heading]]      # Link to heading in note
[[Note Name^block-id]]     # Link to specific block
[[Note Name | Alias]]      # Link with custom display text
```

#### Transclusions (Embeds)
```markdown
![[Note Name]]             # Embed entire note
![[Note Name#Heading]]     # Embed section
![[Note Name^block-id]]    # Embed specific block
![[image.png]]             # Embed image
![[video.mp4]]             # Embed video
```

#### Aliases
```yaml
---
aliases: [Alternative Name, Another Name]
---
```
Allows a note to be linked via multiple names.

#### Tags in Content
```markdown
#tag-name
#multi/word/tag
#nested/tag/structure
```

### 4.3 Advanced Formatting

#### Callouts (Admonitions)
```markdown
> [!note] Title
> This is a note callout.

> [!tip] Pro Tip
> This is a tip callout.

> [!warning] Caution
> This is a warning callout.

> [!example] Code Example
> ```python
> print("Hello")
> ```
```

Available callout types: `note`, `abstract`, `info`, `tip`, `success`, `question`, `warning`, `failure`, `danger`, `bug`, `example`, `quote`, `tldr`, `summary`, `important`, `check`, `done`, `help`, `idea`, `cite`

#### Subscript & Superscript
```markdown
H~2~O          # Subscript
E=mc^2^        # Superscript
```

#### Task Lists with Metadata
```markdown
- [ ] Task due tomorrow #task 📅 2024-01-15
- [x] Completed task ✅
```

---

## 5. Views & Editing Modes

### 5.1 Editing Modes

Obsidian provides three editing modes for Markdown notes:

#### Source Mode (Edit Mode)
- Shows raw Markdown syntax
- Full control over Markdown formatting
- All syntax characters visible (`#`, `*`, `[[]]`)
- Best for power users who type Markdown fluently

#### Live Preview (Default)
- Hybrid mode: renders Markdown as you type
- Syntax characters appear as you type but render when you move away
- Best balance of editing speed and visual feedback
- Default mode for new installations

#### Reading View
- Fully rendered Markdown (like a published document)
- No editing capability
- Best for reviewing final output
- Interactive elements work (checkboxes, collapsible sections)

### 5.2 Note Views

Beyond the three editing modes, Obsidian supports different **view types**:

#### Default Markdown View
- Standard note editing with Source/Live Preview/Reading modes

#### Canvas View
- Infinite whiteboard for visual organization (see [Canvas section](#11-canvas))

#### PDF View
- Embedded PDF reader for PDF attachments

#### Backlink View
- Shows all notes linking to the current note

#### Graph View
- Visual graph of note connections (see [Graph View section](#10-graph-view))

---

## 6. Linking & References

### 6.1 Internal Links (Wikilinks)

The cornerstone of Obsidian's functionality:

```markdown
[[Note Name]]                    # Basic link
[[Note Name | Display Text]]     # Link with alias
[[Note Name#Heading]]            # Link to heading
[[Note Name#Heading^block]]      # Link to block under heading
[[Note Name^block-id]]           # Link to specific block
```

**Features:**
- **Autocomplete**: Start typing `[[` and Obsidian suggests matching notes
- **Auto-create**: Linking to a non-existent note creates it on click
- **Aliases**: Notes can be linked via multiple names
- **Block links**: Link to specific paragraphs, lists, or blocks
- **Heading links**: Link directly to sections within notes

### 6.2 External Links

```markdown
[Link Text](https://example.com)    # Standard Markdown link
https://example.com                 # Auto-linked URL
```

### 6.3 Backlinks

**Backlinks** show which notes link TO the current note:

- Displayed in the right sidebar (Backlinks panel)
- Shows incoming links from other notes
- Includes **unlinked mentions** (text that references the note name without a wikilink)
- Can be filtered and searched
- Available as a core plugin

### 6.4 Outgoing Links

**Outgoing links** show which notes the current note links TO:

- Displayed in the right sidebar (Outgoing Links panel)
- Lists all wikilinks from the current note
- Available as a core plugin

### 6.5 Transclusions (Embeds)

Embed content from one note into another:

```markdown
![[Note Name]]              # Embed entire note
![[Note Name#Heading]]      # Embed section
![[Note Name^block-id]]     # Embed block
```

- Embedded content stays **live-linked**: updates in the source note reflect in the embed
- Can embed notes, sections, blocks, images, PDFs, and audio files
- Embedded notes can be edited in place

### 6.6 Page Preview

Hover over any wikilink to see a **preview popup** of the target note:
- Shows the first few lines of the target note
- Configurable preview length
- Available as a core plugin (enabled by default)

---

## 7. Tags

### 7.1 Tag Syntax

```markdown
#tag
#multi-word-tag
#nested/tag/structure
#tag-with-dashes
#tag_with_underscores
```

### 7.2 Tag Features

- **Hierarchical**: Tags support nesting via `/` separator (`#project/work/active`)
- **Tag pane**: Dedicated sidebar view showing all tags in a tree structure
- **Click to filter**: Clicking a tag shows all notes containing that tag
- **Inline tags**: Tags can appear anywhere in note content
- **Frontmatter tags**: Tags can be defined in YAML frontmatter
```yaml
---
tags: [tag1, tag2, project/work]
---
```

### 7.3 Tag Management

- Tags can be renamed globally (find and replace)
- Tag pane shows count of notes per tag
- Color customization via CSS snippets or themes
- Community plugins offer advanced tag management (Tag Wrangler, Advanced Tag Widget)

---

## 8. Properties & Frontmatter

### 8.1 YAML Frontmatter

Obsidian supports YAML frontmatter at the top of notes for structured metadata:

```yaml
---
title: My Note
date: 2024-01-15
tags: [journal, personal]
aliases: [Alternative Title]
status: in-progress
priority: high
---
```

### 8.2 Properties UI

Obsidian provides a **visual editor** for frontmatter properties (no manual YAML editing required):

#### Property Types
- **Text**: Single-line text
- **Alias**: Alternative names for the note
- **Tags**: Tag list
- **Number**: Numeric values
- **Date**: Date picker
- **Date (time included)**: Date and time picker
- **Checkbox**: Boolean true/false
- **Select**: Single-choice dropdown
- **Multiselect**: Multi-choice dropdown
- **Created**: Auto-filled creation date
- **Modified**: Auto-filled last modification date
- **URL**: Link field

#### Properties View Core Plugin
- Lists all properties across the vault
- Shows properties for the active note
- Enables filtering and sorting by property values

### 8.3 Use Cases

- **Note classification**: Type, status, category
- **Task management**: Due dates, priority, assignee
- **Literature notes**: Author, year, DOI, rating
- **Zettelkasten**: Permanent note metadata
- **Bases data source**: Properties power the Bases feature (see [Bases section](#15-bases))

---

## 9. Core Plugins

Obsidian ships with built-in **core plugins** that provide essential functionality. Each can be enabled/disabled independently.

### 9.1 Navigation & Organization

| Plugin | Description |
|---|---|
| **File Explorer** | Browse files and folders in the vault tree view |
| **Quick Switcher** | Fast note switching via fuzzy search (Ctrl/Cmd+O) |
| **Command Palette** | Access all commands via keyboard (Ctrl/Cmd+P) |
| **Search** | Full-text search across the vault |
| **Bookmarks** | Save quick links to notes, headings, searches |
| **Backlinks** | Show incoming links to the current note |
| **Outgoing Links** | Show all links from the current note |
| **Graph View** | Visualize note relationships as a graph |
| **Outline** | Table of contents for the active note |
| **Page Preview** | Hover preview of linked notes |
| **Footnotes View** | List of footnotes in the current note |

### 9.2 Content Creation

| Plugin | Description |
|---|---|
| **Daily Notes** | Create and open date-based notes automatically |
| **Templates** | Insert predefined note templates |
| **Note Composer** | Split one note into two or merge two notes |
| **Audio Recorder** | Record audio directly into notes |

### 9.3 Organization & Data

| Plugin | Description |
|---|---|
| **Canvas** | Infinite whiteboard for visual note organization |
| **Bases** | Create custom views with filtered, sorted note collections |
| **Properties View** | Manage and view note properties (frontmatter) |

### 9.4 Utilities

| Plugin | Description |
|---|---|
| **File Recovery** | Automatic snapshots and recovery |
| **Format Converter** | Convert files to Obsidian Flavored Markdown |
| **Publish** | Publish notes to Obsidian Publish (premium) |
| **Global Search** | Search across all open vaults |
| **Zen Mode** | Distraction-free writing mode (F11) |
| **Workspaces** | Save and restore layout configurations |

### 9.5 Detailed Core Plugin Descriptions

#### File Explorer
- Tree view of all files and folders in the vault
- Create, rename, delete, and move files and folders
- Drag-and-drop reorganization
- Search within file names
- Sort by name, creation date, or modification date
- Pin frequently used folders

#### Quick Switcher
- Open notes by typing their name (fuzzy matching)
- Keyboard shortcut: `Ctrl/Cmd + O`
- Shows recent notes at top
- Can create new notes from the switcher

#### Command Palette
- Access any command in the app
- Keyboard shortcut: `Ctrl/Cmd + P`
- Includes commands from core and community plugins
- Searchable command list
- Commands can be assigned custom hotkeys

#### Daily Notes
- Automatically creates a note for the current date
- Configurable date format (e.g., `YYYY-MM-DD`, `YYYY/MM/DD`)
- Configurable folder location
- Open previous/next daily notes
- Template support for daily notes
- Keyboard shortcut for creating today's note

#### Templates
- Define reusable note templates
- Insert templates via command or hotkey
- Support for dynamic variables:
  - `{{date}}`: Current date
  - `{{time}}`: Current time
  - `{{title}}`: Note title
  - `{{transcluded}}`: Transcluded content
  - `{{tag}}`: Current tags
- Template folder configuration
- Template hotkeys for quick insertion

#### Note Composer
- **Split**: Divide a note at cursor position into two notes
- **Merge**: Combine two notes into one
- Preserves links and backlinks where possible

#### Audio Recorder
- Record audio directly from the app
- Audio files saved as attachments in the vault
- Embedded player in notes
- Useful for voice notes, meeting recordings, lectures

---

## 10. Graph View

### 10.1 Overview

The **Graph View** is Obsidian's signature feature — an interactive, force-directed graph visualization of all notes and their connections in the vault.

### 10.2 Graph Types

#### Global Graph
- Shows ALL notes and links in the vault
- Provides a "big picture" view of the knowledge base
- Densely connected clusters indicate well-developed topics
- Isolated nodes indicate orphaned notes

#### Local Graph
- Shows only the current note and its direct connections
- Focuses on the neighborhood of one note
- Helps understand a note's context

#### Filtered Graph
- Custom filters to show specific subsets:
  - By tag
  - By folder
  - By file extension
  - By backlink count
  - By filter expressions

### 10.3 Graph Elements

| Element | Representation |
|---|---|
| **Notes** | Circular nodes |
| **Links** | Lines connecting nodes |
| **Tags** | Special-colored nodes |
| **Attachments** | Special-colored nodes |
| **Unresolved links** | Faded/dimmed nodes |
| **Focused note** | Highlighted node (accent color) |
| **Hovered note** | Enlarged node with label |

### 10.4 Graph Interactions

- **Zoom**: Mouse wheel or pinch gesture
- **Pan**: Click and drag on empty space
- **Drag nodes**: Reposition individual nodes
- **Click node**: Open the linked note
- **Hover node**: See note title and connection count
- **Filter**: Use filter controls to show/hide categories

### 10.5 Graph Settings

Configurable options:
- **Display**: Show/hide arrows, text labels, arrows on hover
- **Filters**: Include/exclude by tag, folder, file extension
- **Depth**: Maximum link depth to display
- **Node sizes**: Scale by connection count
- **Line sizes**: Scale by connection strength
- **Colors**: Custom colors for different node types

### 10.6 Graph View Use Cases

- **Knowledge discovery**: Find unexpected connections between notes
- **Orphan detection**: Identify notes with no links
- **Topic clustering**: Visualize related note groups
- **Vault health**: Assess overall connectivity
- **Presentation**: Share vault structure visually

---

## 11. Canvas

### 11.1 Overview

**Canvas** is an infinite whiteboard for visual organization of ideas, notes, and connections. It provides a spatial, freeform workspace where you can arrange cards, text, images, and connectors.

### 11.2 Canvas Elements

| Element | Description |
|---|---|
| **Note Card** | Embeds a note or section from a note |
| **Text Card** | Freeform text block (Markdown supported) |
| **Image** | Embedded image file |
| **File** | Embedded file (PDF, audio, video) |
| **Group** | Container for organizing related cards |
| **Connector** | Lines connecting cards (arrows, lines) |
| **Shape** | Basic shapes (rectangles, circles) |

### 11.3 Canvas Features

- **Infinite canvas**: No boundaries, zoom and pan freely
- **Drag and drop**: Move cards freely on the canvas
- **Connectors**: Draw connections between cards with labeled arrows
- **Zoom levels**: From overview (zoomed out) to detail (zoomed in)
- **Groups**: Organize cards into colored groups
- **Markdown support**: Text cards support full Markdown formatting
- **Live links**: Note cards stay synced with source notes
- **Export**: Export canvas as image or Markdown
- **Mini-map**: Navigation overview in corner

### 11.4 Canvas Use Cases

- **Mind mapping**: Visual brainstorming and idea organization
- **Project planning**: Arrange tasks, milestones, and dependencies
- **Literature review**: Map relationships between sources
- **System design**: Visual architecture diagrams
- **Study notes**: Connect concepts visually
- **Meeting notes**: Organize discussion points spatially
- **Storyboarding**: Sequence ideas and narratives

---

## 12. Search

### 12.1 Full-Text Search

Obsidian provides powerful full-text search across the entire vault:

#### Search Interface
- Located in the left sidebar (Search panel)
- Real-time search results as you type
- Search across all notes or within specific folders

#### Search Operators

| Operator | Syntax | Example | Description |
|---|---|---|---|
| **Exact phrase** | `"phrase"` | `"exact match"` | Match exact phrase |
| **File name** | `file:term` | `file:meeting` | Search in file names |
| **Tag filter** | `tag:tag` | `tag:project` | Filter by tag |
| **Has tag** | `has:tag` | `has:tag` | Show only tagged notes |
| **Path filter** | `path:folder` | `path:Notes/Work` | Search in folder |
| **Case sensitive** | `case:true` | `case:true term` | Case-sensitive search |
| **Regex** | `regex:pattern` | `regex:\d{4}` | Regular expression |
| **Exclude** | `-term` | `-draft` | Exclude results |
| **Content only** | `content:true` | `content:true term` | Search content only |
| **Title only** | `title:true` | `title:true term` | Search titles only |

#### Advanced Features
- **Regex search**: Full regular expression support
- **Case sensitivity**: Toggle case-sensitive search
- **Context snippets**: Shows matching text with surrounding context
- **Result count**: Shows number of matches per note
- **Click to navigate**: Click a result to jump to the matching line

---

## 13. Community Plugins

### 13.1 Plugin Ecosystem

Obsidian has a large, active community plugin ecosystem:
- **1,000+ community plugins** available
- Open-source, community-developed
- Installed via Settings → Community Plugins → Browse
- Each plugin can be enabled/disabled independently

### 13.2 Installing Community Plugins

1. Go to **Settings → Community Plugins**
2. Disable **Safe Mode** (if enabled)
3. Click **Browse** to open the community plugin browser
4. Search or browse plugins
5. Click **Install** on the desired plugin
6. Enable the plugin from the Community Plugins list

### 13.3 Popular Community Plugins

#### Productivity & Organization
| Plugin | Author | Description |
|---|---|---|
| **Dataview** | Blackite | Query and display notes as dynamic tables, lists, and cards using DQL or JavaScript |
| **Templater** | Silas Magalhães | Advanced template engine with JavaScript support |
| **Calendar** | Liam Cain | Calendar view for daily notes |
| **Kanban** | Martin Haegy | Trello-like Kanban boards in Markdown |
| **Todoist Sync** | Varol Van | Sync Obsidian tasks with Todoist |

#### Writing & Editing
| Plugin | Author | Description |
|---|---|---|
| **Editing Toolbar** | Ravan | Floating toolbar with formatting buttons |
| **Word Count** | Bagerbach | Real-time word count display |
| **Copy as Markdown** | Ravan | Copy formatted content as Markdown |
| **Table Formatter** | Zyrosophia | Enhanced table creation and editing |

#### Visualization & Display
| Plugin | Author | Description |
|---|---|---|
| **Minimal Theme** | Keith Naruke | Popular minimal theme with extensive settings |
| **Advanced Tables** | Troy | Enhanced table features (formatting, navigation) |
| **Image Toolkit** | Ravan | Image editing and manipulation |
| **Excalidraw** | Ravan | Hand-drawn style diagrams in notes |

#### Integration & Sync
| Plugin | Author | Description |
|---|---|---|
| **Git Integration** | Vinzent03 | Git version control integration |
| **Obsidian to HTML** | Bagerbach | Export notes as HTML |
| **Quick Add** | Christian B. B. | Create notes from templates with quick commands |
| **Periodic Notes** | Nils Meier | Advanced periodic note management (daily, weekly, monthly) |

#### AI & Advanced
| Plugin | Author | Description |
|---|---|---|
| **Text Generator** | Harry Wood | AI-powered text generation (OpenAI, local models) |
| **Copilot** | Ravan | AI assistant integration |
| **Smart Connections** | Satsuki Yaegashi | AI-powered semantic search and connections |

### 13.4 Plugin Development

Community plugins are built using Obsidian's **Plugin API**:
- TypeScript/JavaScript SDK
- Access to editor, vault, and UI APIs
- Hot-reload development mode
- Published via GitHub repositories
- Distributed through Obsidian's community plugin browser

---

## 14. Themes & Customization

### 14.1 Theme System

Obsidian's theme system is built on CSS custom properties:

#### Default Theme
- Ships with Obsidian
- Dark and light mode support
- Minimal, functional design

#### Community Themes
- 2,000+ community-created themes
- Installed via Settings → Appearance → Themes
- Themes are CSS files that override CSS variables
- Popular themes: Minimal, Primary, AnuPpuccin, Things, Nord, Catppuccin

### 14.2 CSS Customization

#### CSS Snippets
- User-created CSS files placed in `.obsidian/snippets/`
- Enabled via Settings → Appearance → CSS Snippets
- Allow fine-grained UI customization
- Can override any CSS variable

#### CSS Variables Architecture
- Hundreds of exposed CSS variables
- Organized by category: colors, typography, spacing, borders
- Consistent naming conventions
- Theme creators override variables rather than writing custom selectors
- Ensures compatibility across Obsidian versions

### 14.3 Style Settings Plugin

The **Style Settings** community plugin provides:
- A dynamic UI for adjusting theme CSS variables
- No manual CSS editing required
- Theme creators can expose configuration panels
- Settings defined in CSS comments
- Per-theme customization profiles

### 14.4 Appearance Settings

Configurable via Settings → Appearance:
- **Base theme**: Dark / Light / System
- **Theme**: Default or community theme
- **Accent color**: Custom color picker
- **Interface font**: Font for UI elements
- **Text font**: Font for editor content
- **Font size**: Editor font size (default: 16px)
- **Line height**: Editor line height
- **Native menus**: Use OS-native menus
- **Reduce transparency**: Disable translucent effects
- **CSS snippets**: Enable/disable custom CSS

---

## 15. Bases

### 15.1 Overview

**Bases** is a feature that creates custom views of notes filtered and sorted by their properties (frontmatter). It transforms the vault into a dynamic, database-like interface.

### 15.2 Base Views

| View Type | Description |
|---|---|
| **Table** | Notes displayed as a table with property columns |
| **Card** | Notes displayed as cards with property previews |
| **Gallery** | Notes displayed as a visual gallery |
| **Map** | Notes displayed on an interactive map (if location data exists) |

### 15.3 Base Features

- **Filtering**: Filter notes by property values (text, number, date, checkbox, select)
- **Sorting**: Sort by any property (ascending/descending)
- **Grouping**: Group notes by property values
- **Editing**: Edit properties directly from the base view
- **Multiple bases**: Create multiple bases with different filter/sort configurations
- **Dynamic**: Bases update in real-time as note properties change

### 15.4 Use Cases

- **Task management**: Filter tasks by status, priority, due date
- **Literature database**: Organize research papers by author, year, rating
- **Project tracking**: View projects by status, team, deadline
- **Contact management**: Organize contacts by category, last contact
- **Content calendar**: Plan and track content by date, status, type

---

## 16. Attachments & Embedding

### 16.1 Attachment Handling

Obsidian supports various attachment types:

| Type | Supported Formats | Behavior |
|---|---|---|
| **Images** | PNG, JPG, GIF, SVG, WebP, BMP | Inline preview, embeddable |
| **Audio** | MP3, WAV, OGG, M4A | Embedded audio player |
| **Video** | MP4, WebM, OGG | Embedded video player |
| **PDF** | PDF | Embedded PDF viewer |
| **Other** | Any file type | File link with download icon |

### 16.2 Attachment Storage

- Default attachment folder: Configurable in Settings → Files & Links
- Attachments can be stored in:
  - A dedicated attachments folder
  - Relative to the current note (same folder)
  - Custom path templates

### 16.3 Embedding

```markdown
![[image.png]]                    # Embed image
![[video.mp4]]                    # Embed video
![[audio.mp3]]                    # Embed audio
![[document.pdf]]                 # Embed PDF
![[Note Name]]                    # Embed note content
![[Note Name#Heading]]            # Embed note section
```

### 16.4 Drag and Drop

- Drag files into the editor to attach them
- Drag images to resize them
- Drag notes into Canvas to create cards

---

## 17. Callouts

### 17.1 Callout Syntax

```markdown
> [!type] Title
> Content here
```

### 17.2 Callout Types

| Type | Icon | Color | Aliases |
|---|---|---|---|
| `note` | ℹ️ | Blue | — |
| `abstract` | 📋 | Cyan | `summary`, `tldr` |
| `info` | ℹ️ | Cyan | `tip`, `important` |
| `todo` | ✅ | Cyan | `check`, `done` |
| `success` | ✅ | Green | — |
| `question` | ❓ | Yellow | `help`, `faq` |
| `warning` | ⚠️ | Yellow | `caution`, `attention` |
| `failure` | ❌ | Orange | `missing` |
| `danger` | 🚫 | Red | `error` |
| `bug` | 🐛 | Red | — |
| `example` | 📝 | Pink | — |
| `quote` | 💬 | Gray | `cite` |

### 17.3 Callout Features

- **Collapsible**: Click title to collapse/expand
- **Colored**: Each type has a distinct color
- **Icon**: Each type has a distinct icon
- **Nested**: Callouts can be nested within other callouts
- **Content**: Support Markdown, code blocks, and other formatting
- **Custom types**: Create custom callout types via CSS

### 17.4 Collapsible Callouts

```markdown
> [!note]- Collapsed by default
> This content is hidden until expanded.

> [!note+] Expanded by default
> This content is visible by default.
```

---

## 18. Tasks

### 18.1 Task Lists

Basic task management via Markdown checkboxes:

```markdown
- [ ] Unchecked task
- [x] Checked task
```

### 18.2 Task Features

- **Toggle**: Click checkbox to toggle completion
- **Strikethrough**: Completed tasks are automatically struck through
- **Filtering**: Filter tasks by status in search
- **Global task list**: Community plugins provide aggregated task views

### 18.3 Advanced Task Management

Community plugins extend task functionality:

| Plugin | Features |
|---|---|
| **Tasks** | Advanced task querying, filtering, and management |
| **Periodic Notes** | Daily/weekly/monthly task templates |
| **Kanban** | Visual task boards |
| **Calendar** | Task date visualization |
| **Dataview** | Query tasks with custom logic |

### 18.4 Task Metadata

Tasks can include metadata via conventions:

```markdown
- [ ] Task 📅 2024-01-15 ⏰ 10:00 #task/high-priority
- [x] Completed task ✅ 2024-01-14
```

---

## 19. Keyboard Shortcuts & Command Palette

### 19.1 Default Keyboard Shortcuts

#### Navigation
| Shortcut | Action |
|---|---|
| `Ctrl/Cmd + O` | Quick Switcher (open note) |
| `Ctrl/Cmd + P` | Command Palette |
| `Ctrl/Cmd + F` | Find and Replace |
| `Ctrl/Cmd + G` | Find Next |
| `Ctrl/Cmd + Shift + F` | Global Search |
| `Ctrl/Cmd + T` | Open backlinks tab |
| `Ctrl/Cmd + E` | Toggle edit/reading view |

#### Editing
| Shortcut | Action |
|---|---|
| `Ctrl/Cmd + B` | Toggle Bold |
| `Ctrl/Cmd + I` | Toggle Italic |
| `Ctrl/Cmd + K` | Insert Link |
| `Ctrl/Cmd + Shift + S` | Insert Code Block |
| `Ctrl/Cmd + H` | Replace |
| `Tab` | Increase indent |
| `Shift + Tab` | Decrease indent |
| `Enter` | New line/paragraph |
| `Ctrl/Cmd + Enter` | Force new paragraph |

#### View
| Shortcut | Action |
|---|---|
| `F11` | Zen Mode |
| `Ctrl/Cmd + +` | Zoom In |
| `Ctrl/Cmd + -` | Zoom Out |
| `Ctrl/Cmd + 0` | Reset Zoom |
| `Ctrl/Cmd + Shift + S` | Split view |

### 19.2 Command Palette

The Command Palette (`Ctrl/Cmd + P`) provides access to:
- All core plugin commands
- All community plugin commands
- Editor commands
- View commands
- File operations
- Custom commands from plugins

Commands are searchable and can be assigned custom hotkeys.

### 19.3 Custom Hotkeys

- Settings → Hotkeys
- Assign custom shortcuts to any command
- Resolve conflicts between overlapping shortcuts
- Reset to defaults

---

## 20. Obsidian Sync & Premium Services

### 20.1 Obsidian Sync

**Obsidian Sync** is an official paid service for syncing vaults across devices:

#### Features
- **End-to-end encryption**: All data encrypted before leaving your device
- **Cross-device sync**: Sync between desktop and mobile
- **Version history**: Access previous versions of your notes
- **Conflict resolution**: Automatic and manual conflict handling
- **Real-time sync**: Changes sync immediately
- **Selective sync**: Choose which vaults to sync

#### Pricing
- Paid subscription model
- Per-user pricing (unlimited vaults)

### 20.2 Alternative Sync Methods

Since notes are plain files, users can sync via:
- **iCloud**: Automatic sync on Apple devices
- **Google Drive / OneDrive / Dropbox**: Cloud folder sync
- **Syncthing**: Peer-to-peer sync
- **Git**: Version control-based sync
- **Remotely Save plugin**: Community plugin for cloud storage sync

### 20.3 Obsidian Live

**Obsidian Live** enables real-time collaboration:
- Share a vault with others
- See collaborators' cursors in real-time
- Simultaneous editing
- Available as a premium feature

---

## 21. Obsidian Publish

### 21.1 Overview

**Obsidian Publish** is a premium service for publishing notes as a public website:

### 21.2 Features

- **Website hosting**: Host notes as a website, wiki, or documentation site
- **Preserves links**: Internal wikilinks work on the published site
- **Search**: Full-text search on published site
- **Custom domain**: Use your own domain name
- **Graph view**: Published graph view on the website
- **Selective publishing**: Choose which notes to publish
- **Automatic updates**: Changes in vault reflect on the published site
- **Responsive design**: Mobile-friendly published sites

### 21.3 Publishing Workflow

1. Enable the **Publish** core plugin
2. Connect to an Obsidian Publish instance
3. Select notes/folders to publish
4. Publish changes (manual or automatic)
5. Manage published content from the Publish panel

---

## 22. Mobile App

### 22.1 Platform Availability

- **iOS**: Available on the App Store
- **Android**: Available on Google Play Store

### 22.2 Feature Parity

The mobile app includes near feature-parity with desktop:

| Feature | Mobile Support |
|---|---|
| Full Markdown editing | ✅ |
| Live Preview mode | ✅ |
| Reading view | ✅ |
| Source mode | ✅ |
| Wikilinks | ✅ |
| Backlinks | ✅ |
| Graph View | ✅ |
| Canvas | ✅ |
| Search | ✅ |
| Tags | ✅ |
| Properties | ✅ |
| Community plugins | ✅ (most) |
| Themes | ✅ |
| Daily notes | ✅ |
| Templates | ✅ |
| Attachments | ✅ |
| Audio recording | ✅ |
| Callouts | ✅ |
| Bases | ✅ |

### 22.3 Mobile-Specific Features

- **Swipe gestures**: Swipe to open sidebars, switch notes
- **Touch-optimized UI**: Larger touch targets
- **Bottom navigation**: Ribbon moves to bottom bar
- **Camera integration**: Take photos and attach to notes
- **Voice input**: Dictation support
- **Offline access**: Full functionality without internet

### 22.4 Mobile Limitations

- Some community plugins may not be mobile-compatible
- PDF export may be limited
- Certain web embeds may not work
- CSS snippets may render differently on mobile

---

## 23. API & Plugin Development

### 23.1 Plugin API

Obsidian provides a comprehensive **Plugin API** for developers:

#### Key API Modules
- **`Plugin`**: Base class for plugins
- **`Workspace`**: Access to workspace, leaves, and views
- **`Vault`**: File system operations, metadata, caching
- **`Editor`**: Editor instance, cursor, selection
- **`MarkdownView`**: Markdown view operations
- **`Notice`**: Display notifications
- **`SettingTab`**: Create settings UI
- **`Modal`**: Create modal dialogs
- **`ItemView`**: Create custom view types
- **`RibbonIcon`**: Add ribbon icons
- **`Menu`**: Context menu items
- **`EventRef`**: Event listeners

### 23.2 Development Setup

1. Install Obsidian and create a development vault
2. Run `npx degit obsidianmd/obsidian-sample-plugin`
3. Install dependencies: `npm install`
4. Run development mode: `npm run dev`
5. Enable the plugin in Obsidian's community plugins
6. Changes hot-reload automatically

### 23.3 SDK & Tools

- **TypeScript SDK**: Full type definitions
- **Hot-reload**: Instant feedback during development
- **DevTools**: Browser DevTools accessible in Electron
- **Documentation**: Official plugin API documentation
- **Community**: Active developer community on Discord and forums

### 23.4 Publishing Plugins

- Host plugin code on GitHub
- Create `manifest.json` with plugin metadata
- Create `versions.json` with version history
- Submit to Obsidian's community plugin repository
- Users install via Settings → Community Plugins → Browse

---

## 24. Import & Export

### 24.1 Import Sources

Obsidian can import from:

| Source | Method |
|---|---|
| **Notion** | Import via built-in importer (HTML export) |
| **Roam Research** | Import via built-in importer |
| **Logseq** | Import Markdown files directly |
| **Evernote** | Import via ENEX export |
| **OneNote** | Import via HTML export |
| **Apple Notes** | Import via Markdown export |
| **Google Docs** | Import via Markdown export |
| **Standard Markdown** | Open folder containing `.md` files |
| **Obsidian vault** | Open existing vault directly |

### 24.2 Export Options

| Format | Method |
|---|---|
| **Markdown** | Notes are already `.md` files |
| **PDF** | Print to PDF or community plugin |
| **HTML** | Community plugin (Obsidian to HTML) |
| **Obsidian Publish** | Publish to web |
| **Git** | Version control integration |

### 24.3 Format Converter

The **Format Converter** core plugin:
- Converts files to Obsidian Flavored Markdown
- Converts standard Markdown links to wikilinks
- Converts HTML to Markdown
- Batch conversion support

---

## 25. Security & Privacy

### 25.1 Local-First Architecture

- **All data stored locally**: Notes are plain files on your device
- **No required account**: Use Obsidian without creating an account
- **No telemetry by default**: Minimal data collection
- **No cloud dependency**: Full functionality offline

### 25.2 Encryption

- **Obsidian Sync**: End-to-end encryption for synced data
- **Local encryption**: Community plugins offer local file encryption
- **Password protection**: OS-level folder encryption available

### 25.3 Privacy Features

- **No tracking**: No analytics or tracking by default
- **No ads**: No advertising in the app
- **Open format**: Markdown files are readable without Obsidian
- **Data portability**: Easy to export and migrate data
- **No lock-in**: Your data is always accessible as plain files

### 25.4 Community Plugin Security

- Community plugins are open-source
- Code can be reviewed before installation
- Safe Mode disables all community plugins
- Plugin permissions are transparent

---

## Appendix A: Obsidian Flavored Markdown (OFM) — Complete Syntax Reference

```markdown
# Standard Markdown
# Heading
**Bold**
*Italic*
~~Strikethrough~~
`Inline code`
[Link](url)
![Image](url)
> Blockquote
- List item
1. Ordered item
- [ ] Task

# OFM Extensions
[[Wikilink]]
[[Wikilink | Alias]]
[[Note#Heading]]
[[Note^block-id]]
![[Embed]]
==Highlight==
H~2~O (subscript)
E=mc^2^ (superscript)
%%Comment%%

# Callouts
> [!note] Title
> Content

# Properties (YAML Frontmatter)
---
type: note
tags: [tag1, tag2]
aliases: [Alias]
---
```

---

## Appendix B: Comparison with Similar Apps

| Feature | Obsidian | Notion | Roam Research | Logseq |
|---|---|---|---|---|
| **Data format** | Local Markdown | Proprietary cloud | Proprietary graph | Local Markdown |
| **Offline** | ✅ Full | ❌ Limited | ❌ Limited | ✅ Full |
| **Graph view** | ✅ Built-in | ❌ | ✅ Built-in | ✅ Built-in |
| **Plugins** | ✅ 1000+ | ✅ Limited | ✅ Extensions | ✅ Plugins |
| **Canvas** | ✅ Built-in | ✅ Built-in | ❌ | ❌ |
| **Pricing** | Free (sync paid) | Freemium | Paid subscription | Free (open source) |
| **Mobile** | ✅ iOS/Android | ✅ iOS/Android | ✅ iOS/Android | ✅ iOS/Android |
| **Publishing** | ✅ Paid service | ✅ Built-in | ❌ | ❌ |
| **Encryption** | ✅ (Sync) | ✅ | ❌ | ❌ |
| **Themes** | ✅ 2000+ | ❌ | ❌ | ✅ |

---

## Appendix C: Glossary

| Term | Definition |
|---|---|
| **Vault** | A folder containing your notes, attachments, and settings |
| **Wikilink** | `[[Note]]` — Obsidian's internal link syntax |
| **Backlink** | A link from another note TO the current note |
| **Transclusion** | Embedding content from one note into another (`![[Note]]`) |
| **Canvas** | Infinite whiteboard for visual organization |
| **Graph View** | Visual network diagram of note connections |
| **Callout** | Styled admonition box (`> [!note]`) |
| **Properties** | Structured metadata in YAML frontmatter |
| **Bases** | Database-like views filtered by properties |
| **Core Plugin** | Built-in plugin shipped with Obsidian |
| **Community Plugin** | Third-party plugin developed by the community |
| **OFM** | Obsidian Flavored Markdown — extended Markdown syntax |
| **Live Preview** | Hybrid editing mode that renders Markdown in real-time |
| **Zen Mode** | Distraction-free full-screen editing mode |
| **Daily Note** | Auto-generated date-based note |
| **Template** | Reusable note structure |
| **CSS Snippet** | Custom CSS file for UI customization |
| **Block Link** | Link to a specific block/paragraph (`[[Note^id]]`) |
| **Alias** | Alternative name for a note |
| **Tag** | `#tag` — Categorization label |

---

*This document was compiled from Obsidian's official help center (obsidian.md/help), developer documentation, Wikipedia, community plugin documentation, and user guides. Information reflects Obsidian version 1.x as of 2024-2026.*
