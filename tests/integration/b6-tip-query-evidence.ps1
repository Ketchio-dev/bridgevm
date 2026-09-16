function Read-B6TipSnapshot {
    param($Run, $Record, [string]$Path, [ValidateSet('stdout','stderr')][string]$Stream)
    if (!$Record.exit_observed -or !$Record.output_drained) { throw 'Cannot read output before owned child exit and stream drain are confirmed' }
    $metadata = [ordered]@{ captured_bytes = 0; source_bytes = $null; truncated = $false; error = $null }
    $result = @{ Text = ''; Error = $null }; $file = $null
    try {
        if (([IO.File]::GetAttributes($Path) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Output is a reparse point' }
        $file = [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        $length = $file.Length; $metadata.source_bytes = $length; if ($Record.output) { $metadata.source_bytes = $Record.output[$Stream].source_bytes }
        $count = [int][Math]::Min(65536, $length); $bytes = New-Object byte[] $count; $read = 0
        while ($read -lt $count) { $part = $file.Read($bytes, $read, $count - $read); if (!$part) { throw 'Output ended during capture' }; $read += $part }
        if ($file.Length -ne $length) { throw 'Output changed during capture' }
        $metadata.captured_bytes = $read; $metadata.truncated = $metadata.source_bytes -gt 65536
        $name = "$($Record.ordinal)-$($Record.case).$Stream.txt"
        [IO.File]::WriteAllBytes((Join-Path $Run.Snapshots $name), $bytes)
        $metadata.file = $name
        $offset = 0; $encoding = [Text.UTF8Encoding]::new($false, $true)
        if ($count -ge 2 -and $bytes[0] -eq 255 -and $bytes[1] -eq 254) { $encoding = [Text.UnicodeEncoding]::new($false, $true, $true); $offset = 2 }
        elseif ($count -ge 2 -and $bytes[0] -eq 254 -and $bytes[1] -eq 255) { $encoding = [Text.UnicodeEncoding]::new($true, $true, $true); $offset = 2 }
        elseif ($count -ge 3 -and $bytes[0] -eq 239 -and $bytes[1] -eq 187 -and $bytes[2] -eq 191) { $offset = 3 }
        $result.Text = $encoding.GetString($bytes, $offset, $count - $offset)
        if ($metadata.truncated) { throw 'Output exceeds 65536-byte acceptance limit' }
    } catch { $result.Error = $_.Exception.Message; $metadata.error = $result.Error }
    finally { if ($file) { $file.Dispose() } }
    $Record.streams[$Stream] = $metadata
    return $result
}

function Save-B6TipCaseMetadata {
    param($Run, $Record)
    foreach ($key in @('failure','primary_error','cleanup_error','dispose_error')) {
        if ($Record[$key] -and $Record[$key].Length -gt 512) { $Record[$key] = $Record[$key].Substring(0,512) }
    }
    foreach ($value in $Record.streams.Values) {
        if ($value.error -and $value.error.Length -gt 256) { $value.error = $value.error.Substring(0,256) }
    }
    $json = $Record | ConvertTo-Json -Depth 6 -Compress
    $bytes = [Text.Encoding]::UTF8.GetBytes($json)
    if ($bytes.Length -gt 4096) { throw 'Case metadata exceeds 4096-byte limit' }
    [IO.File]::WriteAllBytes((Join-Path $Run.Snapshots "$($Record.ordinal)-$($Record.case).json"), $bytes)
}

function Copy-B6TipBoundedSnapshot {
    param([string]$Source, [string]$Destination, [int]$Limit)
    $file = $null
    try {
        if (([IO.File]::GetAttributes($Source) -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Snapshot is a reparse point' }
        $file = [IO.File]::Open($Source, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
        if ($file.Length -gt $Limit) { throw 'Snapshot exceeds publication limit' }
        $length = [int]$file.Length; $bytes = New-Object byte[] $length; $read = 0
        while ($read -lt $length) { $part = $file.Read($bytes, $read, $length - $read); if (!$part) { throw 'Snapshot ended during publication' }; $read += $part }
        if ($file.Length -ne $length) { throw 'Snapshot changed during publication' }
        [IO.File]::WriteAllBytes($Destination, $bytes)
        return ,$bytes
    } finally { if ($file) { $file.Dispose() } }
}

function Complete-B6TipEvidenceRun {
    param($Run, [bool]$Failed)
    try {
        if ($Failed) {
            if ($Run.Cases.Count -gt 5) { throw 'Too many cases for bounded publication' }
            New-Item -ItemType Directory $Run.PublishRoot | Out-Null
            foreach ($record in $Run.Cases) {
                $name = "$($record.ordinal)-$($record.case).json"
                $null = Copy-B6TipBoundedSnapshot (Join-Path $Run.Snapshots $name) (Join-Path $Run.PublishRoot $name) 4096
                Write-Host ('B6 case-result: ' + ($record | ConvertTo-Json -Depth 6 -Compress))
                if (!$record.exit_observed -or !$record.output_drained) { continue }
                foreach ($stream in @('stdout','stderr')) {
                    $metadata = $record.streams[$stream]
                    if (!$metadata -or !$metadata.file) { continue }
                    $source = Join-Path $Run.Snapshots $metadata.file
                    $bytes = Copy-B6TipBoundedSnapshot $source (Join-Path $Run.PublishRoot $metadata.file) 65536
                    $count = [Math]::Min(8192, $bytes.Length)
                    $text = [Text.Encoding]::UTF8.GetString($bytes, 0, $count)
                    $escaped = $text | ConvertTo-Json -Compress
                    Write-Host "B6 $($record.case) $stream prefix ($count of $($metadata.source_bytes) source bytes): $escaped"
                }
            }
            $summary = @{ failed = $true; case_count = $Run.Cases.Count; owned_cleanup_confirmed = $Run.RawCleanupSafe }
            [IO.File]::WriteAllText((Join-Path $Run.PublishRoot 'run.json'), ($summary | ConvertTo-Json -Compress), [Text.UTF8Encoding]::new($false))
        }
    } finally { Set-B6TipOutputRetention $Run
        if ($Run.RawCleanupSafe) { Remove-Item -Recurse -Force $Run.RawRoot }
        else { Write-Host 'B6 child/output cleanup unconfirmed: raw stream files retained and excluded from artifact capture' }
    }
}
