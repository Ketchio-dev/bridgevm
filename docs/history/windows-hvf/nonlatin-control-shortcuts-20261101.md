# Non-Latin Ctrl shortcut events (2026-11-01)

The live-display event resolver previously ignored Ctrl-only key events
whose characters were a non-Latin letter, even at one of the supported
A/C/V/X/Y/Z key positions. For example, C-position keyCode0x08 withㅊ
produced no guest command. This is an event-contract result, not a retained
live observation of a particular macOS input source or Windows application.

The resolver now preserves any command recognized from the event’s logical
characters first, including ASCII/control-scalar shortcuts on remapped
layouts. Only an otherwise ignored event can use the fallback: Ctrl alone,
one non-ASCII Unicode letter, and a supported ANSI key position. Existing
guest tokens are reused. No guest protocol or device-contract change.

Cmd, Shift and Option combinations, empty or multiple scalars, symbols,
emoji and unsupported positions retain existing behavior. Ctrl+keypad Enter
remains refused before resolution, preventing ETX from becoming Copy.
Host IME composition already exists and is not newly claimed by this patch.

## Validation

Two focused AppKit-event tests cover supported Korean/Cyrillic/Greek letter
shapes plus logical remaps and unsupported combinations. Before the fix,
the positive matrix recorded8 failures while preservation cases passed.
After the fix, both tests pass. New helper/test files stay within their
actual structural budgets; the existing resolver ceiling is unchanged.

Final exact-source local/hosted/PR checks remain required at this source
checkpoint. No live VM/GPU run, release gate sample, criterion promotion or
product-state change is made.
