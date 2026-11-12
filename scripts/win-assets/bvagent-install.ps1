# Install only the agent; imported Windows must already have the serial driver.
$ErrorActionPreference = 'Stop'
function Test-GuestToolsSerialDevice([scriptblock]$Query = { Get-PnpDevice -PresentOnly }) {
    return @(& $Query | Where-Object { $_.InstanceId -match '^PCI\\VEN_1AF4&DEV_(1043|1003)(&|\\)' -and $_.Status -eq 'OK' }).Count -gt 0
}
function Assert-ToolsFolder([string]$Path) {
    if ((Get-Item -LiteralPath $Path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Unsafe installation folder' }
    $acl = Get-Acl -LiteralPath $Path
    $sid = [Security.Principal.SecurityIdentifier]
    $trusted = @('S-1-5-18', 'S-1-5-32-544')
    if ($Path -eq $env:ProgramFiles) { $trusted += 'S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464' }
    if ($acl.GetOwner($sid).Value -notin $trusted) { throw 'Unsafe folder owner' }
    foreach ($rule in $acl.GetAccessRules($true, $true, $sid)) {
        if (($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly) -ne 0) { continue }
        if ($rule.AccessControlType -eq 'Allow' -and $rule.IdentityReference.Value -notin $trusted -and ($rule.FileSystemRights -band 0xD0156)) { throw 'Writable installation folder' }
    }
}
function New-ToolsFolder([string]$Path) {
    [void][IO.Directory]::CreateDirectory($Path)
    $acl = Get-Acl -LiteralPath $Path
    $acl.SetSecurityDescriptorSddlForm('O:S-1-5-32-544D:PAI(A;OICI;FA;;;S-1-5-18)(A;OICI;FA;;;S-1-5-32-544)')
    Set-Acl -LiteralPath $Path -AclObject $acl
    Assert-ToolsFolder $Path
}
try {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        $args = '-NoLogo -NoProfile -ExecutionPolicy Bypass -File "' + $PSCommandPath + '"'
        $child = Start-Process powershell.exe -ArgumentList $args -Verb RunAs -Wait -PassThru
        exit $child.ExitCode
    }
    if (-not (Test-GuestToolsSerialDevice)) {
        Write-Host '이 Windows에는 연결 드라이버가 없어 게스트 도구를 설치할 수 없습니다.'
        Write-Host 'Guest tools cannot be installed because this Windows does not have the connection driver.'
        Read-Host 'Enter'
        exit 1
    }
    $user = (Get-CimInstance Win32_ComputerSystem).UserName
    if ([string]::IsNullOrWhiteSpace($user)) { throw 'No interactive user' }
    Assert-ToolsFolder $env:ProgramFiles
    $root = Join-Path $env:ProgramFiles 'BridgeVM'
    if (-not (Test-Path -LiteralPath $root)) { New-ToolsFolder $root }
    Assert-ToolsFolder $root
    $target = Join-Path $root 'GuestTools'
    $existing = Test-Path -LiteralPath $target
    if (-not $existing) { New-ToolsFolder $target }
    Assert-ToolsFolder $target
    $files = @('bvagent.ps1', 'bvagent-input.ps1', 'bvagent-unicode-input.cs', 'bvagent-key-input.cs', 'bvagent-pointer-input.cs', 'bvagent-window-inventory.ps1', 'bv-window-inventory.cs', 'bvagent-task.ps1', 'bvagent-update.ps1')
    if ($existing -and @(Get-ChildItem -LiteralPath $target -Force).Count -ne $files.Count) { throw 'Unexpected existing tools' }
    foreach ($name in $files) {
        $source = Join-Path $PSScriptRoot $name
        $destination = Join-Path $target $name
        if ($existing) {
            Assert-ToolsFolder $destination
            if ((Get-FileHash -LiteralPath $source).Hash -cne (Get-FileHash -LiteralPath $destination).Hash) { throw 'Unexpected existing tools' }
        } else {
            $file = [IO.File]::Open($destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
            try { $bytes = [IO.File]::ReadAllBytes($source); $file.Write($bytes, 0, $bytes.Length) } finally { $file.Dispose() }
            $acl = Get-Acl -LiteralPath $destination
            $acl.SetSecurityDescriptorSddlForm('O:S-1-5-32-544', 'Owner')
            Set-Acl -LiteralPath $destination -AclObject $acl
        }
    }
    . (Join-Path $target 'bvagent-task.ps1')
    Register-ScheduledTask -TaskName 'BridgeVM Guest Agent' -Xml (Get-GuestAgentTaskXml $user (Join-Path $target 'bvagent.ps1')) -Force | Out-Null
    Start-ScheduledTask -TaskName 'BridgeVM Guest Agent'
} catch {
    Write-Host '게스트 도구를 설치하지 못했으니 이 창을 닫고 다시 실행하세요.'
    Write-Host 'Guest tools could not be installed; close this window and run install.cmd again.'
    Read-Host 'Enter'
    exit 1
}
