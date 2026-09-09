# Diagnostic-only guest asset for the B6 present-latency measurement. Not part
# of any shipped closure gate.
#
# measure-glyph-present-latency.py times a keystroke to the next presented
# frame by watching the scanout surface's seed. On 2026-09-09 that clock was
# being started and stopped by the text caret: classic Notepad's system caret
# blinks on roughly a half-second period, each blink repaints the surface, so
# six of fifteen samples never saw the surface hold still and the nine that did
# measured the distance to the next blink as often as the distance to the
# glyph. Values spread from 6 ms to 237 ms for the same keystroke.
#
# SetCaretBlinkTime(INFINITE) is the documented way to stop the system caret
# blinking; GetCaretBlinkTime returns INFINITE for a caret that does not blink.
# It is session-scoped and immediate, and it only touches the Win32 system
# caret, which is the one classic Notepad's edit control uses. The packaged
# XAML Notepad draws its own caret and is not sampled for latency.
$ErrorActionPreference = 'Stop'
Add-Type -Namespace BvCaret -Name Native -MemberDefinition @'
[DllImport("user32.dll", SetLastError = true)] public static extern bool SetCaretBlinkTime(uint uMSeconds);
[DllImport("user32.dll")] public static extern uint GetCaretBlinkTime();
'@
$before = [BvCaret.Native]::GetCaretBlinkTime()
$ok = [BvCaret.Native]::SetCaretBlinkTime([uint32]::MaxValue)
$after = [BvCaret.Native]::GetCaretBlinkTime()
Write-Output "BVCARETSTILL ok=$ok before_ms=$before after_ms=$after still=$($after -eq [uint32]::MaxValue)"
