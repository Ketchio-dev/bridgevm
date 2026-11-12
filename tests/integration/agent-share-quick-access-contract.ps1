$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$source = [IO.File]::ReadAllText((Join-Path $repo 'scripts/win-assets/bvagent.ps1'))
$block = [regex]::Match($source, '(?s)# >>> share quick access.*?# <<< share quick access')
if (-not $block.Success) { throw 'missing Quick Access implementation' }
. ([scriptblock]::Create($block.Value))
function Assert($ok, $why) { if (-not $ok) { throw "FAIL: $why" } }
$script:pins = 0
$script:logs = @()
$script:fail = $false
function Log($m) { $script:logs += $m }
function New-Object([string]$ComObject) {
    Assert ($ComObject -eq 'Shell.Application') 'Shell COM application'
    if ($script:fail) { throw 'no interactive Shell' }
    $item = [pscustomobject]@{}
    $item | Add-Member ScriptMethod InvokeVerb { param($verb)
        Assert ($verb -eq 'pintohome') 'pin verb'
        $script:pins++
    }
    $shell = [pscustomobject]@{ Item = $item }
    $shell | Add-Member ScriptMethod Namespace { param($path)
        Assert ([IO.Directory]::Exists($path)) 'folder exists before pin'
        [pscustomobject]@{ Self = $this.Item }
    }
    return $shell
}
$old = $env:ProgramData
$temp = Join-Path ([IO.Path]::GetTempPath()) ('bv-pin-' + [guid]::NewGuid().ToString('N'))
$env:ProgramData = $temp
[void][IO.Directory]::CreateDirectory($temp)
try {
    $root = Join-Path $temp 'share'
    Pin-ShareQuickAccess $root
    Assert ($script:pins -eq 0 -and -not $script:BvSharePinAttempted) 'missing folder does nothing'
    [void][IO.Directory]::CreateDirectory($root)
    Pin-ShareQuickAccess $root
    $marker = Join-Path $temp 'BridgeVM/share-quick-access.attempted'
    Assert ($script:pins -eq 1 -and [IO.File]::Exists($marker)) 'pin and persistent marker'
    $script:BvSharePinAttempted = $false # a new agent process; user has unpinned
    Pin-ShareQuickAccess $root
    Assert ($script:pins -eq 1) 'restart respects manual unpin'
    [IO.File]::Delete($marker)
    $script:BvSharePinAttempted = $false
    $script:fail = $true
    Pin-ShareQuickAccess $root
    Assert ($script:logs.Count -eq 1 -and [IO.File]::Exists($marker)) 'failure silent except one log line'
    $script:BvSharePinAttempted = $false
    Pin-ShareQuickAccess $root
    Assert ($script:logs.Count -eq 1) 'failed attempt also never repeated'
    Assert ($source.Contains('Pin-ShareQuickAccess $rootFull')) 'called only from sharing LSR'
    'PASS: Quick Access contract (6 cases)'
} finally {
    $env:ProgramData = $old
    Remove-Item -LiteralPath $temp -Recurse -Force
}
