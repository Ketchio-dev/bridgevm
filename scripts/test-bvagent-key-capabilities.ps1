param([ValidateSet('all', 'current', 'no-taps', 'chords', 'shift-only', 'partial-chords', 'base', 'enter-only', 'space-only', 'no-printscreen', 'no-scroll-units', 'no-pointer-modifiers', 'no-middle')][string]$Mode = 'all')
$ErrorActionPreference = 'Stop'
if ($Mode -eq 'all') {
    foreach ($variant in @('current', 'no-taps', 'chords', 'shift-only', 'partial-chords', 'base', 'enter-only', 'space-only', 'no-printscreen', 'no-scroll-units', 'no-pointer-modifiers', 'no-middle')) {
        & (Get-Process -Id $PID).Path -NoProfile -File $PSCommandPath -Mode $variant
        if ($LASTEXITCODE -ne 0) { throw "key capability variant failed: $variant" }
    }
    Write-Output 'PASS: resident base/expanded key, scroll-unit, pointer-modifier and middle-button capabilities (no SendInput)'
    return
}
$assets = Join-Path $PSScriptRoot 'win-assets'
$keySource = [IO.File]::ReadAllText((Join-Path $assets 'bvagent-key-input.cs'))
$anchor = '        public static Input[] BuildKey(string command) {'
if (-not $keySource.Contains($anchor)) { throw 'missing generic key builder' }
# Older residents built only these keys (bvagent-key-input.cs allowlists before the generic chord builder).
$navigation = @('left', 'up', 'right', 'down', 'home', 'end', 'pageup', 'pagedown')
$base = $navigation + @('backspace', 'tab', 'enter', 'esc', 'escape', 'space', 'insert', 'delete') + @(1..12 | ForEach-Object { "f$_" }) +
    @($navigation + 'tab' | ForEach-Object { "shift+$_" }) + @($navigation + @('a', 'c', 'v', 'x', 'y', 'z', 'backspace', 'delete') | ForEach-Object { "ctrl+$_" }) +
    @($navigation | ForEach-Object { "ctrl+shift+$_" }) + @('tab', 'enter', 'f4' | ForEach-Object { "alt+$_" })
$shift = @('shift+enter', 'shift+space')
$chords = @([char[]]'abcdefghijklmnopqrstuvwxyz' | ForEach-Object { "ctrl+$_" }) + @('ctrl+enter', 'ctrl+tab', 'ctrl+shift+tab', 'shift+delete')
$functions = @(1..12 | ForEach-Object { "shift+f$_" })
$resident = switch ($Mode) {
    'base' { $base }; 'enter-only' { $base + 'shift+enter' }; 'space-only' { $base + 'shift+space' }; 'shift-only' { $base + $shift }
    'partial-chords' { $base + $shift + $chords }; 'chords' { $base + $shift + $chords + $functions }; default { $null }
}
if ($Mode -eq 'no-printscreen') {
    # Every chord, but no Print Screen key (the resident before it).
    if (-not $keySource.Contains('{ "printscreen", 0x2C }, ')) { throw 'missing Print Screen key' }
    $keySource = $keySource.Replace('{ "printscreen", 0x2C }, ', '')
} elseif ($Mode -eq 'no-taps') {
    # Every chord, but not a bare modifier tap.
    $keySource = $keySource.Replace($anchor, $anchor + "`r`n            if (command.IndexOf('+') < 0 && !NamedKeys.ContainsKey(command)) { throw new ArgumentException(""unsupported-key-input""); }")
} elseif ($null -ne $resident) {
    $set = '        private static readonly HashSet<string> ResidentKeys = new HashSet<string>(new string[] { "' + ($resident -join '", "') + '" }, StringComparer.Ordinal);' + "`r`n"
    $keySource = $keySource.Replace($anchor, $set + $anchor + "`r`n            if (!ResidentKeys.Contains(command)) { throw new ArgumentException(""unsupported-key-input""); }")
}
$pointerSource = [IO.File]::ReadAllText((Join-Path $assets 'bvagent-pointer-input.cs'))
if ($Mode -eq 'no-scroll-units') {
    # An older resident pointer module: scrollunits falls through to the coordinate verbs and is refused.
    if (-not $pointerSource.Contains('fields[0] == "scrollunits"')) { throw 'missing scroll-units builder' }
    $pointerSource = $pointerSource.Replace('fields[0] == "scrollunits"', 'fields[0] == "scrollunits-unsupported"')
} elseif ($Mode -eq 'no-pointer-modifiers') {
    # An older resident pointer module: a third field is a format error.
    if (-not $pointerSource.Contains('fields.Length == 3')) { throw 'missing held-modifier builder' }
    $pointerSource = $pointerSource.Replace('fields.Length == 3', 'fields.Length == -3')
} elseif ($Mode -eq 'no-middle') {
    # An older resident pointer module: no middle verbs, and releaseall lifts left and right only.
    if (-not $pointerSource.Contains('case "middle') -or -not $pointerSource.Contains('0x8055')) { throw 'missing middle button builder' }
    $pointerSource = $pointerSource.Replace('case "middle', 'case "unsupported-middle').Replace('0x8055', '0x8015')
}
# Fresh child processes model resident compiled modules; Add-Type cannot replace a loaded type.
$temporary = Join-Path ([IO.Path]::GetTempPath()) (([guid]::NewGuid().ToString()) + '.cs')
$pointerTemporary = Join-Path ([IO.Path]::GetTempPath()) (([guid]::NewGuid().ToString()) + '.cs')
try {
    [IO.File]::WriteAllText($temporary, $keySource)
    [IO.File]::WriteAllText($pointerTemporary, $pointerSource)
    Add-Type -Path (Join-Path $assets 'bvagent-unicode-input.cs'),$temporary,$pointerTemporary
} finally { Remove-Item -LiteralPath $temporary,$pointerTemporary -Force -ErrorAction SilentlyContinue }
. (Join-Path $assets 'bvagent-input.ps1')
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $assets 'bvagent.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw 'resident parse failed' }
$definition = $ast.Find({ param($node)
    $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Get-InputCapabilities'
}, $true)
if ($null -eq $definition) { throw 'missing resident capability function' }
. ([scriptblock]::Create($definition.Extent.Text))
$id = '12345678-1234-1234-1234-123456789abc'
$expected = "BVINPUT_CAPS $id 3 TEXTINPUT KEYINPUT POINTERINPUT 65536"
if ($Mode -in @('current', 'no-taps', 'chords', 'shift-only', 'partial-chords', 'no-printscreen', 'no-scroll-units', 'no-pointer-modifiers', 'no-middle')) { $expected += "`nBVINPUT_KEYS $id SHIFT_ENTER_SPACE" }
if ($Mode -in @('current', 'no-taps', 'chords', 'no-printscreen', 'no-scroll-units', 'no-pointer-modifiers', 'no-middle')) { $expected += "`nBVINPUT_CHORDS $id CTRL_SHIFT_CHORDS" }
if ($Mode -in @('current', 'no-printscreen', 'no-scroll-units', 'no-pointer-modifiers', 'no-middle')) { $expected += "`nBVINPUT_CHORDS $id MODIFIER_CHORDS" }
if ($Mode -in @('current', 'no-taps', 'no-scroll-units', 'no-pointer-modifiers', 'no-middle')) { $expected += "`nBVINPUT_KEYS $id PRINTSCREEN" }
if ($Mode -ne 'no-scroll-units') { $expected += "`nBVINPUT_POINTER $id SCROLL_UNITS" }
if ($Mode -ne 'no-pointer-modifiers') { $expected += "`nBVINPUT_POINTER $id MODIFIERS" }
if ($Mode -ne 'no-middle') { $expected += "`nBVINPUT_POINTER $id MIDDLE" }
$result = Get-InputCapabilities $id
if ($result.Exit -ne 0 -or $result.Out -cne $expected) { throw "incorrect advertised keyset: $Mode" }
$ordinary = Invoke-UnicodeInput "$id YQ==" { param($text) [uint32]2 }
if ($ordinary.Exit -ne 0 -or $ordinary.Out -cne "BVINPUT_INSERTED $id 2") { throw 'ordinary input was disabled' }
if ([BridgeVM.BvUnicodeInput]::BuildKey('enter').Length -ne 2) { throw 'base key support lost' }
if ([BridgeVM.BvUnicodeInput]::BuildPointer('scroll:-1@0x0').Length -ne 2) { throw 'notch scroll support lost' }
Write-Output "PASS: $Mode resident module advertises only its complete keyset and pointer extensions"
