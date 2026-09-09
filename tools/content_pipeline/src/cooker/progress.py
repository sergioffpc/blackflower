"""Compact terminal presentation for cooking stage notifications."""

from collections.abc import Callable, Generator
import contextlib
import os
import sys
import time

from cooker import pipeline

_LABELS = {
    pipeline.CookStage.READING: "reading scene",
    pipeline.CookStage.ENCODING: "encoding scenes",
    pipeline.CookStage.SIGNING: "signing packs",
    pipeline.CookStage.VERIFYING: "verifying packs",
    pipeline.CookStage.PUBLISHING: "publishing packs",
}


def _render(stage: pipeline.CookStage, started: float) -> str:
    """Formats a stage bar or the successful publication summary."""
    encoding = (sys.stderr.encoding or "").lower().replace("-", "")
    unicode_output = encoding == "utf8"
    if stage == pipeline.CookStage.COMPLETE:
        mark = "✔" if unicode_output else "ok"
        return f"{mark} cooked 3 packs in {time.monotonic() - started:.1f}s"
    filled = int(stage) * 12 // int(pipeline.CookStage.COMPLETE)
    solid, empty = ("▰", "▱") if unicode_output else ("#", "-")
    bar = solid * filled + empty * (12 - filled)
    percent = int(stage) * 100 // int(pipeline.CookStage.COMPLETE)
    return f"{bar} {percent:3d}%  {_LABELS[stage]}"


@contextlib.contextmanager
def display() -> Generator[Callable[[pipeline.CookStage], None], None, None]:
    """Yields a stage observer; redraws stderr only on capable terminals.

    Redirected stderr stays silent. NO_COLOR disables color, TERM=dumb disables
    the display, and exiting always terminates an active line before errors.
    """
    enabled = sys.stderr.isatty() and os.environ.get("TERM") != "dumb"
    started = time.monotonic()
    active = False

    def report(stage: pipeline.CookStage) -> None:
        nonlocal active
        if not enabled:
            return
        text = "cooker " + _render(stage, started)
        width = 80
        with contextlib.suppress(OSError):
            width = os.get_terminal_size(sys.stderr.fileno()).columns or 80
        text = text[: max(0, width - 1)]
        if not os.environ.get("NO_COLOR"):
            color = "32" if stage == pipeline.CookStage.COMPLETE else "35"
            text = f"\033[{color}m{text[:6]}\033[0m{text[6:]}"
        with contextlib.suppress(OSError, UnicodeError):
            sys.stderr.write("\r\033[2K" + text)
            sys.stderr.flush()
            active = True

    try:
        yield report
    finally:
        if active:
            with contextlib.suppress(OSError, UnicodeError):
                sys.stderr.write("\n")
                sys.stderr.flush()
