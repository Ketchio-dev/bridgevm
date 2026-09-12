function Get-BvCoherenceFixture($Forms, [string]$Id) {
    $windows = @($Forms | ForEach-Object {
        if (-not $_.Visible -or $_.Handle -eq [IntPtr]::Zero) { throw 'fixture window not visible' }
        @{ id = $_.Handle.ToInt64().ToString(); title = $_.Text }
    })
    $candidate = @{ completed = $false; handles = @(); failure = 'EnumerationFailed' }
    try {
        Add-Type -Path (Join-Path $PSScriptRoot 'bv-window-inventory.cs') -ErrorAction Stop
        $rows = [BridgeVM.NativeWindowInventory]::Enumerate()
        $observed = @()
        foreach ($row in $rows) {
            foreach ($window in $windows) {
                if ($row.ProcessId -eq $PID -and $row.Handle -ceq $window.id -and $row.Title -ceq $window.title) {
                    $observed += $row.Handle
                }
            }
        }
        $candidate = @{ completed = $true; handles = @($observed); failure = 'none' }
    } catch { }
    return @{ schema = 'bridgevm.coherence-fixture.v1'; nonce = $Id; pid = $PID; windows = $windows;
        candidate = $candidate; powershell_version = $PSVersionTable.PSVersion.ToString();
        process_architecture = [string]$env:PROCESSOR_ARCHITECTURE }
}
