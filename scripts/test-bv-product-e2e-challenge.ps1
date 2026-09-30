# Contract: the T17 input challenge in bv-product-e2e.ps1 parses, its user32 bindings
# compile and bind, and its form, label and timers build under Windows PowerShell.
$ErrorActionPreference = 'Stop'
$Path = Join-Path $PSScriptRoot 'win-assets\bv-product-e2e.ps1'
$Tokens = $null; $Errors = $null
$Ast = [System.Management.Automation.Language.Parser]::ParseFile($Path, [ref]$Tokens, [ref]$Errors)
if ($Errors.Count -ne 0) { throw ('guest script does not parse: ' + ($Errors | Out-String)) }
$Text = [IO.File]::ReadAllText($Path)
$Match = [regex]::Match($Text, "-MemberDefinition @'\r?\n(?<body>.*?)\r?\n'@", 'Singleline')
if (-not $Match.Success) { throw 'input desktop member definition not found' }
Add-Type -Namespace BridgeVM -Name InputDesktop -MemberDefinition $Match.Groups['body'].Value
$Accepted = [BridgeVM.InputDesktop]::Accepts([IntPtr]::Zero)
if ($Accepted -isnot [bool] -or $Accepted) { throw 'a null window must never be accepted as the input target' }
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
$Assignments = $Ast.FindAll({ param($Node) $Node -is [System.Management.Automation.Language.AssignmentStatementAst] }, $true) |
    Where-Object { $_.Left.Extent.Text -in @('$Form', '$Label', '$Timer', '$ReadyTimer') }
if (@($Assignments).Count -ne 4) { throw 'challenge form, label and timers were not found exactly once' }
foreach ($Assignment in $Assignments) { Invoke-Expression $Assignment.Extent.Text }
if (-not $Form.TopMost -or $Form.WindowState -ne 'Maximized' -or $Form.FormBorderStyle -ne 'None' -or -not $Form.KeyPreview) { throw 'challenge form properties changed' }
if ($Label.Dock -ne 'Fill' -or $Timer.Interval -ne 100 -or $ReadyTimer.Interval -ne 250) { throw 'challenge label or timers changed' }
$Form.Dispose()
Write-Output 'T17 guest input challenge contract: PASS'
exit 0
