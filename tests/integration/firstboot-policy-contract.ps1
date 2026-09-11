$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$batch = Join-Path $root 'scripts\win-assets\bv-firstboot-policy.cmd'
$work = Join-Path ([IO.Path]::GetTempPath()) ('firstboot-policy-' + [Guid]::NewGuid().ToString('N'))
$source = Join-Path $work 'incoming assets'
$target = Join-Path $work 'guest image'
$sourceMarker = Join-Path $source 'bridgevm-display-only-firstboot.txt'
$targetMarker = Join-Path $target 'BridgeVM\bridgevm-display-only-firstboot.txt'
$script:cases = 0
function Invoke-Policy([string]$From, [string]$To, [int]$Expected) {
    $output = @(& cmd.exe /d /c "call `"$batch`" `"$From`" `"$To`"")
    $code = $LASTEXITCODE
    if ($code -ne $Expected) { throw "Policy exit=$code expected=$Expected output=$output" }
    $script:cases++
}
try {
    [IO.Directory]::CreateDirectory($source) | Out-Null
    [IO.Directory]::CreateDirectory((Join-Path $target 'BridgeVM')) | Out-Null
    Invoke-Policy $source $target 0
    if (Test-Path -LiteralPath $targetMarker) { throw 'Absent source enabled display-only' }
    [IO.File]::WriteAllText($targetMarker, 'inherited B4')
    [IO.File]::SetAttributes($targetMarker, [IO.FileAttributes]::ReadOnly)
    Invoke-Policy $source $target 0
    if (Test-Path -LiteralPath $targetMarker) { throw 'Inherited display-only policy survived' }
    [IO.File]::WriteAllText($sourceMarker, 'explicit B4')
    Invoke-Policy $source $target 0
    if ([IO.File]::ReadAllText($targetMarker) -ne 'explicit B4') { throw 'Explicit policy not copied' }
    [IO.File]::SetAttributes($targetMarker, [IO.FileAttributes]::ReadOnly)
    [IO.File]::WriteAllText($sourceMarker, 'replacement B4')
    Invoke-Policy $source $target 1
    if ([IO.File]::ReadAllText($targetMarker) -ne 'explicit B4') { throw 'Failed copy changed policy' }
    [IO.File]::SetAttributes($targetMarker, [IO.FileAttributes]::Normal)
    Remove-Item -LiteralPath $targetMarker
    [IO.Directory]::CreateDirectory($targetMarker) | Out-Null
    $sentinel = Join-Path $targetMarker 'keep.txt'
    [IO.File]::WriteAllText($sentinel, 'keep')
    Invoke-Policy $source $target 2
    if ([IO.File]::ReadAllText($sentinel) -ne 'keep') { throw 'Malformed target was modified' }
    Remove-Item -LiteralPath $targetMarker -Recurse
    Remove-Item -LiteralPath $sourceMarker
    [IO.Directory]::CreateDirectory($sourceMarker) | Out-Null
    Invoke-Policy $source $target 2
    Invoke-Policy (Join-Path $work 'missing source') $target 2
    Invoke-Policy $source (Join-Path $work 'missing target') 2
    Write-Output "PASS: $script:cases Windows firstboot policy cases"
    $global:LASTEXITCODE = 0
} finally {
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
}
