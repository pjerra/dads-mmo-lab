"""World of Warcraft & Warcraft III inspired UI Theme for Yu'lon (PySide6).

Provides the visual theme, color palette, custom font hierarchies, and Qt Style
Sheets (QSS) for replicating the classic Warcraft aesthetic:
- Deep obsidian / dark iron stone backdrops with parchment textured panels
- Beveled brass and radiant gold filigree borders
- Spellbook / character sheet style navigation tabs
- Heavy action buttons with dual-tone lighting and glowing gold hover states
- Ornate group boxes, runic input boxes, and classic WoW-styled item tooltips
"""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from PySide6.QtGui import QPalette
    from PySide6.QtWidgets import QApplication, QWidget

# --- Warcraft & WoW Color Palette Constants ---
COLOR_BG_DARK = "#0B0D12"
COLOR_BG_CONTAINER = "#12161F"
COLOR_BG_PANEL = "#161A24"
COLOR_BG_PARCHMENT = "#1C1712"
COLOR_BG_PARCHMENT_LIGHT = "#241E17"
COLOR_BG_INPUT = "#0D0F14"

# Gold & Brass Accent Tiers
COLOR_GOLD_BRIGHT = "#FFD100"
COLOR_GOLD_LIGHT = "#FFF1A8"
COLOR_GOLD_BORDER = "#D4AF37"
COLOR_GOLD_BRASS = "#C89B3C"
COLOR_BRASS_DARK = "#785A28"
COLOR_BRASS_DEEP = "#3C2D14"

# Quality & State Accents
COLOR_COMMON = "#FFFFFF"
COLOR_UNCOMMON = "#1EFF00"  # Fel / Green
COLOR_RARE = "#0070FF"      # Arcane / Blue
COLOR_EPIC = "#A335EE"      # Nether / Purple
COLOR_LEGENDARY = "#FF8000" # Sunwell / Orange
COLOR_ARTIFACT = "#E6CC80"  # Gold
COLOR_DANGER = "#C41E3A"    # Crimson Red
COLOR_TEXT_PRIMARY = "#F0E6D2"  # Parchment White
COLOR_TEXT_MUTED = "#8A8275"    # Aged Text
COLOR_TEXT_GOLD = "#F0C050"     # Warm Gold

# Font Hierarchies
FONT_FAMILY_TITLE = (
    "'Cinzel', 'Beaufort for LOL', 'Friz Quadrata', 'FrizQuadrata BT', "
    "'Palatino Linotype', 'Book Antiqua', 'Georgia', serif"
)
FONT_FAMILY_BODY = (
    "'Segoe UI', 'Ubuntu', 'Helvetica Neue', 'Arial', sans-serif"
)
FONT_FAMILY_MONO = (
    "'Consolas', 'Fira Code', 'JetBrains Mono', 'DejaVu Sans Mono', monospace"
)


WARCRAFT_THEME_QSS = f"""
/* ==========================================================================
   Yu'lon Warcraft & World of Warcraft Desktop Theme
   ========================================================================== */

/* --- Global Base Window & Central Widget --- */
QMainWindow, QDialog, QWidget#centralWidget {{
    background-color: {COLOR_BG_DARK};
    color: {COLOR_TEXT_PRIMARY};
    font-family: {FONT_FAMILY_BODY};
    font-size: 13px;
}}

QWidget {{
    color: {COLOR_TEXT_PRIMARY};
    font-family: {FONT_FAMILY_BODY};
}}

/* --- Tab Widget & Spellbook Style Tab Bar --- */
QTabWidget::pane {{
    border: 2px solid {COLOR_BRASS_DARK};
    background-color: {COLOR_BG_CONTAINER};
    border-radius: 4px;
    top: -2px;
}}

QTabBar::tab {{
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #25201A, stop:0.5 #1A1612, stop:1 #100D0A
    );
    color: {COLOR_TEXT_MUTED};
    border: 1.5px solid {COLOR_BRASS_DEEP};
    border-bottom: 2px solid {COLOR_BRASS_DARK};
    border-top-left-radius: 5px;
    border-top-right-radius: 5px;
    padding: 7px 16px;
    margin-right: 3px;
    font-family: {FONT_FAMILY_TITLE};
    font-size: 13px;
    font-weight: bold;
    min-width: 80px;
}}

QTabBar::tab:hover {{
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #3A3026, stop:0.5 #282018, stop:1 #1A1410
    );
    color: {COLOR_GOLD_LIGHT};
    border-color: {COLOR_GOLD_BRASS};
}}

QTabBar::tab:selected {{
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #42321D, stop:0.4 #2A1F13, stop:1 #17100A
    );
    color: {COLOR_GOLD_BRIGHT};
    border: 2px solid {COLOR_GOLD_BRASS};
    border-top: 3px solid {COLOR_GOLD_BRIGHT};
    border-bottom: 2px solid {COLOR_BG_CONTAINER};
    padding-bottom: 8px;
}}

/* West (Left Sidebar) Tabs */
QTabBar::tab:west {{
    border-top-left-radius: 5px;
    border-bottom-left-radius: 5px;
    border-top-right-radius: 0px;
    border-bottom-right-radius: 0px;
    padding: 10px 16px;
    margin-bottom: 3px;
    margin-right: 0px;
    border: 1.5px solid {COLOR_BRASS_DEEP};
    border-right: 2px solid {COLOR_BRASS_DARK};
    min-height: 24px;
}}

QTabBar::tab:west:selected {{
    border: 2px solid {COLOR_GOLD_BRASS};
    border-left: 3px solid {COLOR_GOLD_BRIGHT};
    border-right: 2px solid {COLOR_BG_CONTAINER};
    padding-right: 18px;
}}

QTabBar::tab:disabled {{
    background-color: #12100E;
    color: #4A443C;
    border-color: #24201A;
}}

/* --- Classic WoW Action Buttons --- */
QPushButton {{
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #3A2B18, stop:0.45 #261A0C, stop:0.55 #1C1208, stop:1 #100A04
    );
    color: {COLOR_TEXT_GOLD};
    border: 2px solid {COLOR_BRASS_DARK};
    border-top: 2px solid {COLOR_GOLD_BORDER};
    border-left: 2px solid {COLOR_GOLD_BORDER};
    border-right: 2px solid {COLOR_BRASS_DEEP};
    border-bottom: 2px solid {COLOR_BRASS_DEEP};
    border-radius: 4px;
    padding: 6px 14px;
    font-family: {FONT_FAMILY_TITLE};
    font-size: 12px;
    font-weight: bold;
    min-height: 20px;
}}

QPushButton:hover {{
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #543E23, stop:0.45 #382613, stop:0.55 #2B1C0D, stop:1 #191007
    );
    color: {COLOR_GOLD_LIGHT};
    border: 2px solid {COLOR_GOLD_BRIGHT};
    border-top: 2px solid #FFF8D0;
    border-left: 2px solid #FFF8D0;
    border-right: 2px solid {COLOR_GOLD_BRASS};
    border-bottom: 2px solid {COLOR_GOLD_BRASS};
}}

QPushButton:pressed {{
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #120A04, stop:0.5 #1C1208, stop:1 #2E1F10
    );
    color: {COLOR_GOLD_BRASS};
    border: 2px solid {COLOR_BRASS_DEEP};
    border-top: 2px solid #1A1208;
    border-left: 2px solid #1A1208;
    border-right: 2px solid {COLOR_BRASS_DARK};
    border-bottom: 2px solid {COLOR_BRASS_DARK};
    padding-top: 7px;
    padding-left: 15px;
}}

QPushButton:disabled {{
    background-color: #17181C;
    color: #555555;
    border: 1.5px solid #2C2C32;
}}

/* Special Primary / Prominent Buttons (e.g. Install, Start) */
QPushButton[primary="true"], QPushButton#start-server, QPushButton#install-btn {{
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #5C4119, stop:0.45 #3D2B11, stop:0.55 #2B1E0C, stop:1 #171006
    );
    color: {COLOR_GOLD_LIGHT};
    border: 2px solid {COLOR_GOLD_BRIGHT};
    border-top: 2px solid #FFF1A8;
    border-left: 2px solid #FFF1A8;
}}

QPushButton[danger="true"], QPushButton#stop-server, QPushButton#purge-btn {{
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #4A1212, stop:0.5 #2E0B0B, stop:1 #1A0606
    );
    color: #FFB8B8;
    border: 2px solid #962828;
    border-top: 2px solid #C43A3A;
    border-left: 2px solid #C43A3A;
}}

QPushButton[danger="true"]:hover, QPushButton#stop-server:hover, QPushButton#purge-btn:hover {{
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #661A1A, stop:0.5 #421010, stop:1 #260909
    );
    color: #FFFFFF;
    border-color: #E84141;
}}

/* --- Group Boxes & Frames --- */
QGroupBox {{
    background-color: {COLOR_BG_PARCHMENT};
    border: 1.5px solid {COLOR_BRASS_DARK};
    border-radius: 6px;
    margin-top: 20px;
    padding: 14px 10px 10px 10px;
}}

QGroupBox::title {{
    subcontrol-origin: margin;
    subcontrol-position: top center;
    padding: 2px 14px;
    background-color: {COLOR_BG_DARK};
    color: {COLOR_GOLD_BRIGHT};
    font-family: {FONT_FAMILY_TITLE};
    font-size: 13px;
    font-weight: bold;
    border: 1.5px solid {COLOR_BRASS_DARK};
    border-radius: 4px;
}}

QFrame[frameShape="5"], QFrame[frameShape="StyledPanel"] {{
    background-color: {COLOR_BG_PARCHMENT};
    border: 1.5px solid {COLOR_BRASS_DARK};
    border-radius: 5px;
}}

/* --- Input Fields & Spinners --- */
QLineEdit, QSpinBox, QComboBox, QTextEdit, QPlainTextEdit {{
    background-color: {COLOR_BG_INPUT};
    color: {COLOR_TEXT_PRIMARY};
    border: 1.5px solid {COLOR_BRASS_DARK};
    border-radius: 3px;
    padding: 5px 8px;
    selection-background-color: #523E1E;
    selection-color: {COLOR_GOLD_LIGHT};
}}

QLineEdit:focus, QSpinBox:focus, QComboBox:focus, QTextEdit:focus, QPlainTextEdit:focus {{
    border: 1.5px solid {COLOR_GOLD_BRIGHT};
    background-color: #0E121A;
}}

QComboBox::drop-down {{
    subcontrol-origin: padding;
    subcontrol-position: top right;
    width: 22px;
    border-left: 1px solid {COLOR_BRASS_DARK};
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #322514, stop:1 #181109
    );
}}

QComboBox QAbstractItemView {{
    background-color: {COLOR_BG_CONTAINER};
    border: 1.5px solid {COLOR_GOLD_BRASS};
    color: {COLOR_TEXT_PRIMARY};
    selection-background-color: #3D2D16;
    selection-color: {COLOR_GOLD_BRIGHT};
    padding: 4px;
}}

/* --- Tables, Lists, and Tree Views --- */
QListWidget, QTableWidget, QTreeWidget, QTreeView, QTableView {{
    background-color: {COLOR_BG_INPUT};
    border: 1.5px solid {COLOR_BRASS_DARK};
    border-radius: 4px;
    gridline-color: #26211A;
    color: {COLOR_TEXT_PRIMARY};
    alternate-background-color: #11141B;
}}

QListWidget::item, QTableWidget::item, QTreeWidget::item {{
    padding: 4px 6px;
    border-bottom: 1px solid #1E1A14;
}}

QListWidget::item:hover, QTableWidget::item:hover, QTreeWidget::item:hover {{
    background-color: #241D14;
    color: {COLOR_GOLD_LIGHT};
}}

QListWidget::item:selected, QTableWidget::item:selected, QTreeWidget::item:selected {{
    background: qlineargradient(
        x1:0, y1:0, x2:1, y2:0,
        stop:0 #4A3519, stop:1 #241A0C
    );
    color: {COLOR_GOLD_BRIGHT};
    border-left: 3px solid {COLOR_GOLD_BRIGHT};
}}

QHeaderView::section {{
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #2C2318, stop:1 #17120B
    );
    color: {COLOR_GOLD_BRASS};
    font-family: {FONT_FAMILY_TITLE};
    font-weight: bold;
    padding: 5px;
    border: 1px solid {COLOR_BRASS_DEEP};
    border-bottom: 2px solid {COLOR_BRASS_DARK};
}}

/* --- Scroll Bars (Carved Stone & Brass Trough) --- */
QScrollBar:vertical {{
    background-color: {COLOR_BG_DARK};
    width: 14px;
    margin: 14px 0 14px 0;
    border: 1px solid #1E1914;
}}

QScrollBar::handle:vertical {{
    background: qlineargradient(
        x1:0, y1:0, x2:1, y2:0,
        stop:0 #3D2F1D, stop:0.5 #574328, stop:1 #3D2F1D
    );
    min-height: 24px;
    border: 1px solid {COLOR_BRASS_DARK};
    border-radius: 2px;
}}

QScrollBar::handle:vertical:hover {{
    background: qlineargradient(
        x1:0, y1:0, x2:1, y2:0,
        stop:0 #574328, stop:0.5 #785C36, stop:1 #574328
    );
    border-color: {COLOR_GOLD_BRASS};
}}

QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{
    background: {COLOR_BG_PARCHMENT};
    height: 14px;
    subcontrol-origin: margin;
    border: 1px solid {COLOR_BRASS_DEEP};
}}

QScrollBar:horizontal {{
    background-color: {COLOR_BG_DARK};
    height: 14px;
    margin: 0 14px 0 14px;
    border: 1px solid #1E1914;
}}

QScrollBar::handle:horizontal {{
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #3D2F1D, stop:0.5 #574328, stop:1 #3D2F1D
    );
    min-width: 24px;
    border: 1px solid {COLOR_BRASS_DARK};
    border-radius: 2px;
}}

/* --- Splitter --- */
QSplitter::handle {{
    background-color: #1A1612;
    border: 1px solid #2B2218;
}}

QSplitter::handle:horizontal {{
    width: 6px;
}}

QSplitter::handle:vertical {{
    height: 6px;
}}

QSplitter::handle:hover {{
    background-color: {COLOR_GOLD_BRASS};
}}

/* --- Progress Bars (XP & Health/Mana Style) --- */
QProgressBar {{
    border: 1.5px solid {COLOR_BRASS_DARK};
    border-radius: 4px;
    background-color: #0A0D12;
    text-align: center;
    color: {COLOR_TEXT_PRIMARY};
    font-weight: bold;
    font-size: 11px;
}}

QProgressBar::chunk {{
    background: qlineargradient(
        x1:0, y1:0, x2:0, y2:1,
        stop:0 #27AE60, stop:0.5 #1E824C, stop:1 #145A32
    );
    border-radius: 2px;
}}

/* --- Classic World of Warcraft Item Tooltip Style --- */
QToolTip {{
    background-color: rgba(11, 13, 18, 0.96);
    color: {COLOR_TEXT_PRIMARY};
    border: 1.5px solid {COLOR_GOLD_BORDER};
    padding: 8px 12px;
    border-radius: 4px;
    font-family: {FONT_FAMILY_BODY};
    font-size: 12px;
}}

/* --- Checkboxes & Radio Buttons --- */
QCheckBox, QRadioButton {{
    spacing: 7px;
    color: {COLOR_TEXT_PRIMARY};
}}

QCheckBox::indicator, QRadioButton::indicator {{
    width: 16px;
    height: 16px;
    border: 1.5px solid {COLOR_BRASS_DARK};
    background-color: {COLOR_BG_INPUT};
    border-radius: 3px;
}}

QRadioButton::indicator {{
    border-radius: 8px;
}}

QCheckBox::indicator:hover, QRadioButton::indicator:hover {{
    border-color: {COLOR_GOLD_BRIGHT};
}}

QCheckBox::indicator:checked {{
    background-color: {COLOR_GOLD_BRASS};
    border-color: {COLOR_GOLD_LIGHT};
}}

QRadioButton::indicator:checked {{
    background-color: {COLOR_GOLD_BRASS};
    border-color: {COLOR_GOLD_LIGHT};
}}

/* --- Menus & Context Menus --- */
QMenuBar {{
    background-color: {COLOR_BG_DARK};
    border-bottom: 1.5px solid {COLOR_BRASS_DARK};
    color: {COLOR_TEXT_PRIMARY};
}}

QMenuBar::item:selected {{
    background-color: #2C2216;
    color: {COLOR_GOLD_BRIGHT};
}}

QMenu {{
    background-color: {COLOR_BG_CONTAINER};
    border: 1.5px solid {COLOR_GOLD_BRASS};
    border-radius: 4px;
    padding: 4px;
    color: {COLOR_TEXT_PRIMARY};
}}

QMenu::item {{
    padding: 6px 18px 6px 12px;
    border-radius: 2px;
    font-family: {FONT_FAMILY_BODY};
    font-size: 12px;
}}

QMenu::item:selected {{
    background: qlineargradient(
        x1:0, y1:0, x2:1, y2:0,
        stop:0 #4A3519, stop:1 #241A0C
    );
    color: {COLOR_GOLD_BRIGHT};
}}

QMenu::item:disabled {{
    color: #666666;
}}

QMenu::separator {{
    height: 1px;
    background: {COLOR_BRASS_DARK};
    margin: 4px 6px;
}}

/* --- Labels & Status Badges --- */
QLabel#updateBanner {{
    background: qlineargradient(
        x1:0, y1:0, x2:1, y2:0,
        stop:0 #4A3311, stop:0.5 #6E4D19, stop:1 #4A3311
    );
    color: {COLOR_GOLD_LIGHT};
    border: 1.5px solid {COLOR_GOLD_BRIGHT};
    border-radius: 4px;
    padding: 8px 14px;
    font-weight: bold;
}}
"""


def build_warcraft_palette() -> QPalette:
    """Build a QPalette matching the dark Warcraft obsidian and gold color scheme."""
    from PySide6.QtGui import QColor, QPalette

    palette = QPalette()
    palette.setColor(QPalette.ColorRole.Window, QColor(COLOR_BG_DARK))
    palette.setColor(QPalette.ColorRole.WindowText, QColor(COLOR_TEXT_PRIMARY))
    palette.setColor(QPalette.ColorRole.Base, QColor(COLOR_BG_INPUT))
    palette.setColor(QPalette.ColorRole.AlternateBase, QColor(COLOR_BG_CONTAINER))
    palette.setColor(QPalette.ColorRole.ToolTipBase, QColor(COLOR_BG_DARK))
    palette.setColor(QPalette.ColorRole.ToolTipText, QColor(COLOR_TEXT_PRIMARY))
    palette.setColor(QPalette.ColorRole.Text, QColor(COLOR_TEXT_PRIMARY))
    palette.setColor(QPalette.ColorRole.Button, QColor(COLOR_BG_PARCHMENT))
    palette.setColor(QPalette.ColorRole.ButtonText, QColor(COLOR_TEXT_GOLD))
    palette.setColor(QPalette.ColorRole.BrightText, QColor(COLOR_GOLD_BRIGHT))
    palette.setColor(QPalette.ColorRole.Link, QColor(COLOR_RARE))
    palette.setColor(QPalette.ColorRole.Highlight, QColor(COLOR_GOLD_BRASS))
    palette.setColor(QPalette.ColorRole.HighlightedText, QColor(COLOR_BG_DARK))
    return palette


def apply_warcraft_theme(target: QApplication | QWidget) -> None:
    """Apply the Warcraft theme stylesheet and palette to a QApplication or QWidget."""
    from PySide6.QtWidgets import QApplication

    if isinstance(target, QApplication):
        target.setPalette(build_warcraft_palette())
        target.setStyleSheet(WARCRAFT_THEME_QSS)
    else:
        target.setStyleSheet(WARCRAFT_THEME_QSS)
