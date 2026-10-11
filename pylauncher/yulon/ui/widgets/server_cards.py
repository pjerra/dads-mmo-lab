"""The Server tab's at-a-glance pieces (T699): the status pill, a service row, a Get playing step.

They draw what `ui.server_status` answers and hold no state of their own beyond it.
Colours are `theme.STATUS_TONES`, one place; every state also has its own shape and word.
"""

from __future__ import annotations

from PySide6.QtCore import QPointF, QRectF, QSize, Qt
from PySide6.QtGui import QBrush, QColor, QPainter, QPainterPath, QPaintEvent, QPen
from PySide6.QtWidgets import QFrame, QHBoxLayout, QLabel, QSizePolicy, QVBoxLayout, QWidget

from yulon.ui.server_status import PillLook, ServiceLine, Shape, Step, Tone, pill_for
from yulon.ui.theme import COLOR_TEXT_MUTED, STATUS_TONES

STATUS_PILL = "status-pill"
"""objectName of the Server tab's big status pill."""


def paint_shape(painter: QPainter, shape: Shape, colour: str, box: QRectF) -> None:
    """Draw `shape` filling `box` in `colour`: the drawing the pill, the rows and the rail share."""
    painter.save()
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    fill = QColor(colour)
    side = min(box.width(), box.height())
    ring = max(1.5, side * 0.16)
    centre = box.center()
    radius = side / 2
    if shape == "dot":
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QBrush(fill))
        painter.drawEllipse(centre, radius, radius)
    elif shape == "ring":
        painter.setPen(QPen(fill, ring))
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawEllipse(centre, radius - ring / 2, radius - ring / 2)
    elif shape == "half":
        inner = radius - ring / 2
        painter.setPen(QPen(fill, ring))
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawEllipse(centre, inner, inner)
        half = QPainterPath()
        half.moveTo(centre)
        half.arcTo(QRectF(centre.x() - inner, centre.y() - inner, 2 * inner, 2 * inner), 90, 180)
        half.closeSubpath()
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QBrush(fill))
        painter.drawPath(half)
    elif shape == "triangle":
        path = QPainterPath()
        path.moveTo(QPointF(centre.x(), box.top()))
        path.lineTo(QPointF(box.right(), box.bottom()))
        path.lineTo(QPointF(box.left(), box.bottom()))
        path.closeSubpath()
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QBrush(fill))
        painter.drawPath(path)
    elif shape == "diamond":
        path = QPainterPath()
        path.moveTo(QPointF(centre.x(), box.top()))
        path.lineTo(QPointF(box.right(), centre.y()))
        path.lineTo(QPointF(centre.x(), box.bottom()))
        path.lineTo(QPointF(box.left(), centre.y()))
        path.closeSubpath()
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(QBrush(fill))
        painter.drawPath(path)
    elif shape == "cross":
        painter.setPen(QPen(fill, ring * 1.3, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
        inset = side * 0.18
        painter.drawLine(
            QPointF(box.left() + inset, box.top() + inset),
            QPointF(box.right() - inset, box.bottom() - inset),
        )
        painter.drawLine(
            QPointF(box.right() - inset, box.top() + inset),
            QPointF(box.left() + inset, box.bottom() - inset),
        )
    else:  # "question": a ring with a gap, the badge's "?" drawn as a shape
        painter.setPen(QPen(fill, ring, Qt.PenStyle.DotLine))
        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.drawEllipse(centre, radius - ring / 2, radius - ring / 2)
    painter.restore()


class ShapeMark(QWidget):
    """A state's shape, `size` px, in its tone's colour."""

    def __init__(self, size: int, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self._shape: Shape = "ring"
        self._tone: Tone = "down"
        self.setFixedSize(size, size)
        self.setAttribute(Qt.WidgetAttribute.WA_TransparentForMouseEvents)

    @property
    def shape(self) -> Shape:
        return self._shape

    @property
    def tone(self) -> Tone:
        return self._tone

    def set_look(self, shape: Shape, tone: Tone) -> None:
        if (shape, tone) != (self._shape, self._tone):
            self._shape, self._tone = shape, tone
            self.update()

    def paintEvent(self, _event: QPaintEvent) -> None:
        painter = QPainter(self)
        paint_shape(
            painter,
            self._shape,
            STATUS_TONES[self._tone]["mark"],
            QRectF(self.rect()).adjusted(1, 1, -1, -1),
        )
        painter.end()


class StatusPill(QFrame):
    """ONLINE / STARTING / PARTLY UP / OFFLINE and the rest: a shape and a word in a rounded pill.

    Follows the realm badge (`set_status` with its word), so it is that one reading, bigger.
    """

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName(STATUS_PILL)
        self.mark = ShapeMark(12, self)
        self.word = QLabel("", self)
        row = QHBoxLayout(self)
        row.setContentsMargins(14, 6, 16, 6)
        row.setSpacing(8)
        row.addWidget(self.mark, 0, Qt.AlignmentFlag.AlignVCenter)
        row.addWidget(self.word, 0, Qt.AlignmentFlag.AlignVCenter)
        self.setSizePolicy(QSizePolicy.Policy.Fixed, QSizePolicy.Policy.Fixed)
        self._look: PillLook = pill_for("stopped")
        self._draw()

    @property
    def look(self) -> PillLook:
        return self._look

    def set_status(self, status: str) -> None:
        look = pill_for(status)
        if look != self._look:
            self._look = look
            self._draw()

    def _draw(self) -> None:
        look = self._look
        tone = STATUS_TONES[look.tone]
        self.mark.set_look(look.shape, look.tone)
        self.word.setText(look.word)
        self.setAccessibleName(f"Server status: {look.word.capitalize()}")
        self.setStyleSheet(
            f"QFrame#{STATUS_PILL} {{ background: {tone['back']}; "
            f"border: 1px solid {tone['edge']}; "
            f"border-radius: 15px; }}"
            f"QFrame#{STATUS_PILL} QLabel {{ color: {tone['text']}; font-weight: bold; "
            f"letter-spacing: 1px; background: transparent; border: none; }}"
        )


class ServiceRow(QFrame):
    """One service: its shape, its name over a muted detail line, and its word on the right."""

    def __init__(self, name: str, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setObjectName("service-row")
        self.mark = ShapeMark(11, self)
        self.name = QLabel(name, self)
        self.name.setStyleSheet("font-weight: bold; background: transparent;")
        self.detail = QLabel("", self)
        self.detail.setStyleSheet(f"color: {COLOR_TEXT_MUTED}; background: transparent;")
        self.word = QLabel("", self)
        texts = QVBoxLayout()
        texts.setSpacing(0)
        texts.addWidget(self.name)
        texts.addWidget(self.detail)
        row = QHBoxLayout(self)
        row.setContentsMargins(10, 6, 12, 6)
        row.setSpacing(10)
        row.addWidget(self.mark, 0, Qt.AlignmentFlag.AlignVCenter)
        row.addLayout(texts, 1)
        row.addWidget(self.word, 0, Qt.AlignmentFlag.AlignVCenter)
        self.setStyleSheet(
            "QFrame#service-row { background: rgba(0, 0, 0, 0.22); border-radius: 8px; "
            "border: none; }"
        )
        self.line: ServiceLine | None = None

    def show_line(self, line: ServiceLine) -> None:
        self.line = line
        self.mark.set_look(line.shape, line.tone)
        self.name.setText(line.name)
        self.detail.setText(line.detail)
        self.detail.setVisible(bool(line.detail))
        self.word.setText(line.word)
        self.word.setStyleSheet(
            f"color: {STATUS_TONES[line.tone]['text']}; font-weight: bold; background: transparent;"
        )
        self.setAccessibleName(f"{line.name}: {line.word}")


class StepMark(QLabel):
    """A Get playing step's round mark: ✓ when done, its number when not."""

    def __init__(self, number: int, parent: QWidget | None = None) -> None:
        super().__init__(str(number), parent)
        self.number = number
        self.setFixedSize(QSize(24, 24))
        self.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.show_step(False, False)

    def show_step(self, done: bool, current: bool) -> None:
        up, between, down = STATUS_TONES["up"], STATUS_TONES["between"], STATUS_TONES["down"]
        if done:
            self.setText("✓")
            style = f"background: {up['back']}; color: {up['text']}; border: none;"
        elif current:
            self.setText(str(self.number))
            ring = between["mark"]
            style = f"background: transparent; color: {ring}; border: 2px solid {ring};"
        else:
            self.setText(str(self.number))
            style = (
                f"background: transparent; color: {down['text']}; border: 2px solid {down['edge']};"
            )
        self.setStyleSheet(f"{style} border-radius: 12px; font-weight: bold;")


class StepRow(QWidget):
    """One Get playing step: its mark, its words, and the press that does it while it is open."""

    def __init__(self, number: int, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.mark = StepMark(number, self)
        self.text = QLabel("", self)
        self.text.setWordWrap(True)
        self.row = QHBoxLayout(self)
        self.row.setContentsMargins(0, 0, 0, 0)
        self.row.setSpacing(12)
        self.row.addWidget(self.mark, 0, Qt.AlignmentFlag.AlignVCenter)
        self.row.addWidget(self.text, 1)
        self.step: Step | None = None

    def add_press(self, press: QWidget) -> None:
        press.setParent(self)
        self.row.addWidget(press, 0, Qt.AlignmentFlag.AlignVCenter)

    def show_step(self, step: Step, current: bool) -> None:
        self.step = step
        self.mark.show_step(bool(step.done), current)
        self.text.setText(step.text)
        muted = not step.done and not current
        self.text.setStyleSheet(f"color: {COLOR_TEXT_MUTED};" if muted else "")
