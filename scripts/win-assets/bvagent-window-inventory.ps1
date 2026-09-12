# Resident WINLIST companion: no PowerShell callback crosses the native boundary.
function ConvertTo-BvWindowInventoryLines {
    param([object[]]$Rows)
    $ErrorActionPreference = 'Stop'
    if ($null -eq $Rows -or $Rows.Count -gt 4096) { throw 'Invalid window inventory size' }
    $culture = [Globalization.CultureInfo]::InvariantCulture
    $utf8 = [Text.UTF8Encoding]::new($false, $true)
    $handles = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $lines = [Collections.Generic.List[string]]::new()
    [long]$wireBytes = 7 # WINEND plus LF, even for an empty inventory.
    foreach ($row in $Rows) {
        if ($null -eq $row) { throw 'Null window record' }
        $handle = [string]$row.Handle
        [uint64]$number = 0
        if ($handle -cnotmatch '^[1-9][0-9]{0,19}$' -or
            -not [uint64]::TryParse($handle, [ref]$number) -or
            $number.ToString($culture) -cne $handle -or -not $handles.Add($handle)) {
            throw 'Invalid or duplicate window handle'
        }
        [uint32]$wpid = 0
        if (-not [uint32]::TryParse([string]$row.ProcessId, [ref]$wpid) -or $wpid -eq 0) {
            throw 'Invalid window process ID'
        }
        $bounds = [Collections.Generic.List[string]]::new()
        foreach ($name in @('X', 'Y', 'W', 'H')) {
            [int]$value = 0
            if (-not [int]::TryParse([string]$row.$name, [Globalization.NumberStyles]::Integer,
                    $culture, [ref]$value) -or ($name -in @('W', 'H') -and $value -le 0)) {
                throw 'Invalid window bounds'
            }
            $bounds.Add($value.ToString($culture))
        }
        if ($null -eq $row.Title -or [string]$row.Title -eq '') { throw 'Empty window title' }
        $title = [Convert]::ToBase64String($utf8.GetBytes([string]$row.Title))
        $line = 'WIN ' + $handle + ' ' + $wpid.ToString($culture) + ' ' +
            [string]::Join(' ', $bounds.ToArray()) + ' ' + $title
        $wireBytes += $line.Length + 1 # The complete line is ASCII; LF is one byte.
        if ($wireBytes -gt 4194304) { throw 'Window inventory wire budget exceeded' }
        $lines.Add($line)
    }
    return $lines.ToArray()
}
function Get-BvWindowInventoryLines {
    $ErrorActionPreference = 'Stop'
    if (-not ('BridgeVM.NativeWindowInventory' -as [type])) {
        Add-Type -Path (Join-Path $PSScriptRoot 'bv-window-inventory.cs') -ErrorAction Stop
    }
    $rows = @([BridgeVM.NativeWindowInventory]::Enumerate())
    ConvertTo-BvWindowInventoryLines -Rows $rows
}
