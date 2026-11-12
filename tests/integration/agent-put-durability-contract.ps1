# Source contract only: cache bypass/durability is tested by the registered crash matrix.
$ErrorActionPreference = 'Stop'
$repo = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$s = [IO.File]::ReadAllText((Join-Path $repo 'scripts/win-assets/bvagent.ps1'))
$stage = [regex]::Match($s, '(?s)function New-PutStage\b.*?\r?\n\}').Value
if ($stage -notmatch 'FileOptions\]::WriteThrough') { throw 'FAIL: staging stream must use FileOptions.WriteThrough' }
foreach ($verb in @('PUT', 'PUTBEG')) {
    $body = [regex]::Match($s, "(?s)\n                '$verb' \{(?<body>.*?)\r?\n                \}\r?\n").Groups['body'].Value
    if ($body -notmatch 'New-PutStage \$path \$true') { throw "FAIL: $verb must open the write-through stage" }
}
'agent PUT durability ordering: PASS (2 paths; source contract, not crash evidence)'
