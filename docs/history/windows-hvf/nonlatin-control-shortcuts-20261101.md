# Non-Latin Ctrl shortcut events (2026-11-01)

The live-display resolver ignored Ctrl-only events containing a non-Latin
letter at supported A/C/V/X/Y/Z positions: C-position keyCode0x08 withㅊ
produced no guest command. This describes the event contract; native macOS
input-source generation and Windows delivery have not been measured.

Recognized logical characters win first, preserving ASCII/control-scalar
shortcuts on remapped layouts. Otherwise, Ctrl alone with one non-ASCII
Unicode letter can fall back to a supported ANSI position. Cmd/Shift/Option,
empty/multiple scalars, symbols, emoji and unsupported positions keep their
behavior. Ctrl+keypad Enter is refused before ETX can become Copy.
Existing guest tokens and host IME composition are unchanged.

Two focused AppKit-event tests cover Korean/Cyrillic/Greek letters, logical
remaps and refusals. The old positive matrix had8 failures; preservation
cases passed. Both tests pass with the fix. Structural ceilings stay intact.

The first PR CI attempt failed the existing daemon delayed-hello test:
its session was absent after a50ms scheduling sleep; its fake peer closed
at250ms. The log does not identify which race occurred. The test now
acknowledges the write and holds its peer open through the original
held-connection assertions. Daemon product code is unchanged. The test was
extracted under the existing ceiling; no timing criterion was relaxed.
Superseded final checks were stopped and their failure record retained.

Final exact-source local/hosted/PR checks remain required. No live VM/GPU
run, release-gate sample, criterion promotion or product-state change.
