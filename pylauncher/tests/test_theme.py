"""Tests for the Warcraft & WoW UI theme and custom decorative widgets."""

from __future__ import annotations

from typing import Any, cast

from PySide6.QtCore import Qt
from PySide6.QtGui import QPalette
from PySide6.QtWidgets import QApplication, QFrame, QMainWindow, QTabWidget, QWidget

import main
from yulon.catalog.catalog import load_catalog
from yulon.ui.catalog_view import CatalogView
from yulon.ui.theme import (
    COLOR_BG_DARK,
    COLOR_BG_PARCHMENT,
    WARCRAFT_THEME_QSS,
    apply_warcraft_theme,
    build_warcraft_palette,
)
from yulon.ui.widgets.log_panel import LogPanel
from yulon.ui.widgets.warcraft_decorations import (
    WarcraftHeader,
    WarcraftRealmBadge,
    format_warcraft_tooltip,
)


def test_warcraft_palette_construction() -> None:
    palette = build_warcraft_palette()
    assert isinstance(palette, QPalette)
    assert palette.color(QPalette.ColorRole.Window).name().lower() == COLOR_BG_DARK.lower()
    assert palette.color(QPalette.ColorRole.Button).name().lower() == COLOR_BG_PARCHMENT.lower()


def test_warcraft_theme_qss_covers_essential_controls() -> None:
    assert "QMainWindow" in WARCRAFT_THEME_QSS
    assert "QTabWidget" in WARCRAFT_THEME_QSS
    assert "QTabBar::tab" in WARCRAFT_THEME_QSS
    assert "QTabBar::tab:west" in WARCRAFT_THEME_QSS
    assert "QMenu" in WARCRAFT_THEME_QSS
    assert "QMenu::item" in WARCRAFT_THEME_QSS
    assert "QPushButton" in WARCRAFT_THEME_QSS
    assert "QGroupBox" in WARCRAFT_THEME_QSS
    assert "QLineEdit" in WARCRAFT_THEME_QSS
    assert "QListWidget" in WARCRAFT_THEME_QSS
    assert "QProgressBar" in WARCRAFT_THEME_QSS
    assert "QToolTip" in WARCRAFT_THEME_QSS


def test_apply_warcraft_theme_on_widget(qapp: QApplication) -> None:
    widget = QWidget()
    apply_warcraft_theme(widget)
    assert widget.styleSheet() == WARCRAFT_THEME_QSS


def test_apply_warcraft_theme_on_qapp(qapp: QApplication) -> None:
    apply_warcraft_theme(qapp)
    assert qapp.styleSheet() == WARCRAFT_THEME_QSS


def test_warcraft_realm_badge(qapp: QApplication) -> None:
    badge = WarcraftRealmBadge("online")
    assert "ONLINE" in badge._label.text()
    badge.set_status("starting")
    assert "STARTING" in badge._label.text()
    badge.set_status("restarting")
    assert "RESTARTING" in badge._label.text()
    badge.set_status("stopped")
    assert "OFFLINE" in badge._label.text()


def test_warcraft_header(qapp: QApplication) -> None:
    header = WarcraftHeader("TEST REALM", "Subtitle")
    assert header is not None
    header.set_realm_status("running")
    assert "ONLINE" in header._badge._label.text()


def test_format_warcraft_tooltip() -> None:
    tooltip_html = format_warcraft_tooltip(
        "Thunderfury, Blessed Blade",
        ["Binds when picked up", "Speed 1.90"],
        quality="legendary",
        flavor_text="Did someone say...?",
        item_level=80,
    )
    assert "Thunderfury" in tooltip_html
    assert "Did someone say" in tooltip_html
    assert "Server Tier / Build: 80" in tooltip_html


def test_build_catalog_tab_sets_west_tab_position(qapp: QApplication) -> None:
    window = QMainWindow()
    catalog = load_catalog()
    panel = LogPanel()
    view = CatalogView(catalog, lambda _e: cast(Any, None), panel)
    tabs, _banner, _splitter = main.build_catalog_tab(window, view, panel)
    assert tabs.tabPosition() == QTabWidget.TabPosition.West


def test_catalog_tile_has_custom_context_menu_policy(qapp: QApplication) -> None:
    catalog = load_catalog()
    panel = LogPanel()
    view = CatalogView(catalog, lambda _e: cast(Any, None), panel)
    frames = view.findChildren(QFrame)
    custom_frames = [
        f for f in frames if f.contextMenuPolicy() == Qt.ContextMenuPolicy.CustomContextMenu
    ]
    assert len(custom_frames) >= len(catalog.games)
