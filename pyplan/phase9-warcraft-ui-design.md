# Phase 9 Design — Warcraft III & World of Warcraft UI Theme for Yu'lon

> **Audience:** engineers and designers maintaining the Yu'lon desktop launcher.
> **Scope:** UI/UX overhaul replicating the classic aesthetic and tactile feel of Warcraft III and World of Warcraft client interfaces using PySide6 (Qt) styling, custom widgets, and palettes.

---

## 1. Aesthetic Vision & Design Language

The Yu'lon launcher manages private offline WoW and classic MMO servers. The UI should immediately evoke the look and feel of Warcraft III and World of Warcraft:

1. **Stone, Dark Iron & Parchment Foundations**:
   - Deep obsidian/charcoal backdrops (`#0B0D12` / `#12161F` / `#161A24`).
   - Weathered dark parchment / runic stone panels (`#1A1612` / `#241E18` / `#2D251E`).
   - Authentic inner and outer drop shadows and subtle textures.

2. **Ornate Gold & Brass Filigree**:
   - Iconic WoW gold border trim (`#C89B3C`, `#F0C050`, `#785A28`, `#3C2D14`).
   - Radiant gold text highlights (`#FFD100`, `#FFF1A8`) for active states, headers, and primary actions.
   - Quality rarity color accents:
     - Common / Parchment: `#F0E6D2` / `#E6E6E6`
     - Uncommon / Fel / Health Green: `#1EFF00` / `#2ECC71`
     - Rare / Arcane / Mana Blue: `#0070FF` / `#00CCFF`
     - Epic / Nether Purple: `#A335EE`
     - Legendary / Sunwell Orange: `#FF8000`
     - Artifact / Titan Gold: `#E6CC80`
     - Danger / Horde Crimson: `#C41E3A` / `#D63031`

3. **Typography & Hierarchies**:
   - Display & Headers: Serif / Classic Fantasy font stack fallback: `'Cinzel', 'Beaufort for LOL', 'Friz Quadrata', 'FrizQuadrata BT', 'Palatino Linotype', 'Book Antiqua', 'Georgia', serif`.
   - Body & Controls: Crisp, highly legible UI typography: `'Segoe UI', 'Ubuntu', 'Helvetica Neue', 'Arial', sans-serif`.
   - Console & Logs: High-contrast monospace with WoW chat channel coloring: `'Consolas', 'Fira Code', 'JetBrains Mono', 'DejaVu Sans Mono', monospace`.

---

## 2. Component Specifications

### 2.1 Buttons (`QPushButton`)
- **Action Buttons**: Heavy beveled gold/brass borders with dual-tone lighting (highlight on top/left, shadow on bottom/right).
- **Backgrounds**: Deep amber/charcoal linear gradient (`#3A2A1A` -> `#221810` -> `#150E0A`).
- **Hover**: Brilliant gold border glow (`#FFD100`), radiant text luminescence, enhanced surface lighting.
- **Pressed**: Sunken inner border (`#3C2D14`), dark inset amber background.
- **Primary Action (Install / Start / Apply)**: Radiant golden border with subtle warm pulse or accent trim.
- **Disabled**: Carved stone grey (`#4A4A4A`), dark charcoal background (`#1A1A1A`), muted text (`#666666`).

### 2.2 Tabs (`QTabWidget` & `QTabBar`)
- Styled like the classic WoW Spellbook & Character sheet tabs.
- **Inactive Tabs**: Dark iron/stone background with subtle brass border, muted amber/gold text.
- **Hover**: Warm gold border illumination.
- **Active Tab**: Raised brass border with top gold accent bar, rich ember/obsidian background, bright gold text (`#FFD100`).

### 2.3 Group Boxes & Panels (`QGroupBox`, `QFrame`)
- Ornate brass engraved border with subtle corner rounding.
- Centered golden headers with fantasy serif typography.
- Translucent dark parchment fill (`rgba(22, 18, 14, 0.85)`).

### 2.4 Form Inputs (`QLineEdit`, `QSpinBox`, `QComboBox`)
- Inset dark runic stone background (`#0D0F14`).
- Brass border (`#6B532E`) transitioning to radiant gold (`#FFD100`) on focus.
- Golden dropdown arrow icons for `QComboBox`.

### 2.5 Tables & Lists (`QListWidget`, `QTableWidget`, `QTreeWidget`)
- Dark velvet / stone item backgrounds.
- Alternating subtle rows for maximum readability.
- Gold-highlighted item selection with gold left border indicator bar.

### 2.6 Progress Bars & Health/Mana Gauges (`QProgressBar`)
- Heavy brass border with carved bevels.
- Vibrant emerald green / arcane blue / nether purple / sunwell amber fills.
- Crisp centered percentage/status text.

### 2.7 Tooltips (`QToolTip`)
- Exact replication of the iconic World of Warcraft item tooltip:
  - Deep obsidian translucent background (`rgba(10, 12, 18, 0.96)`).
  - Dual-line gold border (`#785A28` outer, `#FFD100` inner).
  - Radiant gold title, bright white stats, muted grey flavor text.

### 2.8 Header Banner & Realm Status Badges
- Ornate top banner in `main.py` displaying the Yu'lon emblem, title, and realm status badge (Emerald Green = Online, Charcoal = Offline, Arcane Blue = Connecting, Amber = Working).

### 2.9 Left-Side Navigation Rail & Responsive Window Resizing
- **Main Realm / Catalog Navigation Rail**: `QTabWidget.TabPosition.West` transforms the primary top tab strip into a classic left-hand realm navigation rail (evoking Warcraft 3 battlements & modern Battle.net realm selectors).
- **Sub-Tabs**: Kept as top-positioned spellbook tabs inside individual `ControllerView` server managers.
- **Window Resizing**: Minimum window geometry established at `850x550` with flexible splitter proportions and strict `_CATALOG_MIN_WIDTH` floor preservation (no clipped tiles on resize).

### 2.10 Rich Interactive Context Menus
- **Catalog Tiles (`CatalogView`)**: Right-click menu for direct actions ("Install Server…", "Use Existing Server…", "Find in WSL…", "Copy Server Details").
- **Controller Views (`ControllerView`)**:
  - Accounts List: Right-click to copy username, change password, or adjust GM level.
  - Characters List: Right-click to copy name, revive, teleport, adjust level, send gold, send worn gear, or queue rename.
  - Bot List: Right-click to copy bot name.
  - Modules List: Right-click to install, remove, or copy module identifiers.
  - Backup List: Right-click to preview restore plan, trigger database restore, or copy backup filename.
  - Main Window Tab Bar: Right-click realm tabs to open server directory in file manager, copy server path, start server, or stop server.

---

## 3. Architecture & Separation of Concerns

Following `style-guide.md` §1–§5:
- `yulon/ui/theme.py`: Owns all QSS rules, color constants, font stacks, and `apply_warcraft_theme()`.
- Theme is applied globally at application launch, with modular widget-specific classes for reusable ornaments.
- Preserves all existing geometry floors (`_CATALOG_MIN_WIDTH`, `DEFAULT_WINDOW_SIZE`, splitter proportions, object names).
- Tested with dedicated unit tests in `tests/test_theme.py`.
