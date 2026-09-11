$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$query = Join-Path $root 'scripts/win-assets/bv-windows-closure-proof.ps1'
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($query, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw 'guest script parse failed' }
$function = $ast.Find({ param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Write-DisplayProof' }, $true)
if ($null -eq $function) { throw 'missing actual display producer' }
$parameterText = $ast.ParamBlock.Extent.Text
$functionText = $function.Extent.Text
$directory = Join-Path ([IO.Path]::GetTempPath()) ('b6 display ' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($directory)
$fixture = Join-Path $directory 'producer.ps1'
$engine = (Get-Command powershell.exe).Source
function Invoke-Producer([string]$Mode = '', [string]$Current = '1600x900', [int]$Count = 28, [string]$Modes = "'1280x720','1600x900','1920x1080'") {
    $stub = "function Get-DisplayProof { [pscustomobject]@{ Device='\\.\DISPLAY2'; Current='$Current'; Count=$Count; Modes=[System.Collections.Generic.List[string]]@($Modes) } }"
    $script = $parameterText + "`r`n" + $stub + "`r`n" + $functionText + "`r`nWrite-DisplayProof`r`n"
    [IO.File]::WriteAllText($fixture, $script, [Text.UTF8Encoding]::new($false))
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $engine
    $start.Arguments = '-NoProfile -ExecutionPolicy Bypass -File "' + $fixture + '"'
    if ($Mode) { $start.Arguments += ' -RequestedMode ' + $Mode }
    $start.UseShellExecute = $false; $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
    $child = [Diagnostics.Process]::Start($start)
    try {
        $null = $child.Handle
        $stdout = $child.StandardOutput.ReadToEndAsync(); $stderr = $child.StandardError.ReadToEndAsync()
        if (-not $child.WaitForExit(30000)) { $child.Kill(); throw 'producer timeout' }
        [pscustomobject]@{ Code=$child.ExitCode; Text=$stdout.Result.Trim(); Error=$stderr.Result }
    } finally { $child.Dispose() }
}
try {
    $default = Invoke-Producer
    if ($default.Code -ne 0 -or $default.Text -ne 'BVF2 device=\\.\DISPLAY2 current=1600x900 modes=28 has_1600x900=True') { throw 'legacy default changed' }
    foreach ($mode in '1280x720','1600x900','1920x1080') {
        $result = Invoke-Producer -Mode $mode -Current $mode
        if ($result.Code -ne 0 -or $result.Text -ne "BVF2 device=\\.\DISPLAY2 current=$mode modes=28 has_${mode}=True") { throw "requested mode proof failed: $mode" }
    }
    $absent = Invoke-Producer -Mode '1280x720' -Modes "'1600x900'"
    if ($absent.Code -ne 0 -or $absent.Text -notmatch 'has_1280x720=False$') { throw 'missing requested mode was hidden' }
    $single = Invoke-Producer -Count 1
    if ($single.Code -ne 11) { throw 'single-mode refusal changed' }
    $invalid = Invoke-Producer -Mode '1024x768'
    if ($invalid.Code -eq 0) { throw 'unsupported requested mode accepted' }
    $different = Invoke-Producer -Mode '1280x720' -Current '1920x1080'
    if ($different.Code -ne 0 -or $different.Text -notmatch 'current=1920x1080 modes=28 has_1280x720=True$') { throw 'actual current mode was rewritten' }
    Write-Output 'B6 display producer: 8 contracts PASS'
} finally { Remove-Item -LiteralPath $directory -Recurse -Force }
