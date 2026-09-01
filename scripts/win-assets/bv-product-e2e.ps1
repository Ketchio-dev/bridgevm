[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('KeyboardPointer', 'Clipboard', 'Share', 'Network', 'Audio', 'MarkerA', 'MarkerB', 'MarkerRestoredA', 'AgentResult')]
    [string]$Action,
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-f]{64}$')]
    [string]$Nonce,
    [string]$JobID = '',
    [string]$Commit = '',
    [int]$Lane = 0,
    [string]$VMSlug = ''
)
$ErrorActionPreference = 'Stop'
$Share = 'C:\bridgevm-share'
$Prefix = $Nonce.Substring(0, 12)
$Utf8 = New-Object System.Text.UTF8Encoding($false)

function Write-Exact([string]$Name, [string]$Body) {
    $Path = Join-Path $Share $Name
    [IO.File]::WriteAllBytes($Path, $Utf8.GetBytes($Body))
}

function Require-Exact([string]$Path, [string]$Body) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "missing input: $Path" }
    $Actual = [IO.File]::ReadAllBytes($Path)
    $Expected = $Utf8.GetBytes($Body)
    if ($Actual.Length -ne $Expected.Length) { throw "input length mismatch: $Path" }
    for ($Index = 0; $Index -lt $Actual.Length; $Index++) {
        if ($Actual[$Index] -ne $Expected[$Index]) { throw "input bytes mismatch: $Path" }
    }
}

function Hash([string]$Name) {
    return (Get-FileHash -LiteralPath (Join-Path $Share $Name) -Algorithm SHA256).Hash.ToLowerInvariant()
}

switch ($Action) {
    'KeyboardPointer' {
        Add-Type -AssemblyName System.Windows.Forms
        Add-Type -AssemblyName System.Drawing
        $ExpectedText = 't17kbd' + $Prefix
        $script:Typed = ''
        $script:Clicked = $false
        $Form = New-Object System.Windows.Forms.Form
        $Form.Text = 'BridgeVM T17 Input Challenge'
        $Form.WindowState = [System.Windows.Forms.FormWindowState]::Maximized
        $Form.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::None
        $Form.BackColor = [Drawing.Color]::FromArgb(20, 70, 150)
        $Form.KeyPreview = $true
        $Label = New-Object System.Windows.Forms.Label
        $Label.AutoSize = $false
        $Label.Dock = [System.Windows.Forms.DockStyle]::Fill
        $Label.TextAlign = [Drawing.ContentAlignment]::MiddleCenter
        $Label.Font = New-Object Drawing.Font('Segoe UI', 28, [Drawing.FontStyle]::Bold)
        $Label.ForeColor = [Drawing.Color]::White
        $Label.Text = 'CLICK AND TYPE THE SEALED CHALLENGE'
        $Form.Controls.Add($Label)
        $Form.Add_MouseDown({ $script:Clicked = $true })
        $Label.Add_MouseDown({ $script:Clicked = $true })
        $Form.Add_KeyPress({ param($Sender, $Event) $script:Typed += $Event.KeyChar })
        $Timer = New-Object System.Windows.Forms.Timer
        $Timer.Interval = 100
        $Timer.Add_Tick({
            if ($script:Clicked -and $script:Typed.EndsWith($ExpectedText)) {
                $Timer.Stop()
                $Form.Close()
            }
        })
        $Form.Add_Shown({ $Form.Activate(); $Form.Focus(); $Timer.Start() })
        [void]$Form.ShowDialog()
        if (-not $script:Clicked -or -not $script:Typed.EndsWith($ExpectedText)) { throw 'input challenge incomplete' }
        Write-Exact "t17-keyboard-pointer-$Prefix.txt" "bridgevm-t17-keyboard-pointer-v1`n$Nonce`n"
    }
    'Clipboard' {
        $Korean = [string][char]0xBE0C + [char]0xB9AC + [char]0xC9C0 + 'VM T17 ' + [char]0xD074 + [char]0xB9BD + [char]0xBCF4 + [char]0xB4DC + ' ' + [char]0xC655 + [char]0xBCF5 + ' v1'
        $Expected = "$Korean`n$Nonce`n"
        if ((Get-Clipboard -Raw) -ne $Expected) { throw 'Windows clipboard does not contain the sealed challenge' }
        Write-Exact "t17-clipboard-guest-$Prefix.txt" $Expected
    }
    'Share' {
        Require-Exact (Join-Path $Share "t17-$Prefix.txt") "bridgevm-t17-share-v1`n$Nonce`n"
        Write-Exact "t17-guest-$Prefix.txt" "bridgevm-t17-guest-share-v1`n$Nonce`n"
    }
    'Network' {
        $Adapter = Get-NetIPConfiguration | Where-Object { $_.IPv4Address -and $_.IPv4DefaultGateway } | Select-Object -First 1
        if (-not $Adapter) { throw 'no configured IPv4 adapter' }
        [void](Resolve-DnsName -Name 'example.com' -Type A -ErrorAction Stop | Select-Object -First 1)
        $Response = Invoke-WebRequest -Uri 'http://example.com/' -UseBasicParsing -TimeoutSec 30
        if ([int]$Response.StatusCode -ne 200) { throw 'HTTP status was not 200' }
        Write-Exact "t17-network-$Prefix.txt" "bridgevm-t17-network-ok-v1`n$Nonce`n"
    }
    'Audio' {
        $Wave = 'C:\ProgramData\BridgeVM\t17-tone.wav'
        $Rate = 48000; $Samples = $Rate; $Stream = New-Object IO.MemoryStream
        $Writer = New-Object IO.BinaryWriter($Stream); $Bytes = $Samples * 4
        $Writer.Write([char[]]'RIFF'); $Writer.Write([int](36 + $Bytes)); $Writer.Write([char[]]'WAVE')
        $Writer.Write([char[]]'fmt '); $Writer.Write([int]16); $Writer.Write([int16]1); $Writer.Write([int16]2)
        $Writer.Write([int]$Rate); $Writer.Write([int]($Rate * 4)); $Writer.Write([int16]4); $Writer.Write([int16]16)
        $Writer.Write([char[]]'data'); $Writer.Write([int]$Bytes)
        for ($Index = 0; $Index -lt $Samples; $Index++) {
            $Value = [int16](12000 * [Math]::Sin(2 * [Math]::PI * 440 * $Index / $Rate))
            $Writer.Write($Value); $Writer.Write($Value)
        }
        [IO.File]::WriteAllBytes($Wave, $Stream.ToArray())
        (New-Object Media.SoundPlayer $Wave).PlaySync()
        Write-Exact "t17-audio-$Prefix.txt" "bridgevm-t17-audio-ok-v1`n$Nonce`n"
    }
    'MarkerA' {
        $Body = "bridgevm-t17-snapshot-a-v1`n$Nonce`n"
        Write-Exact "t17-snapshot-a-$Prefix.txt" $Body
        [IO.File]::WriteAllBytes('C:\ProgramData\BridgeVM\t17-snapshot-marker.txt', $Utf8.GetBytes($Body))
    }
    'MarkerB' {
        $Body = "bridgevm-t17-snapshot-b-v1`n$Nonce`n"
        Write-Exact "t17-snapshot-b-$Prefix.txt" $Body
        [IO.File]::WriteAllBytes('C:\ProgramData\BridgeVM\t17-snapshot-marker.txt', $Utf8.GetBytes($Body))
    }
    'MarkerRestoredA' {
        $Body = "bridgevm-t17-snapshot-a-v1`n$Nonce`n"
        Require-Exact 'C:\ProgramData\BridgeVM\t17-snapshot-marker.txt' $Body
        Write-Exact "t17-snapshot-restored-a-$Prefix.txt" $Body
    }
    'AgentResult' {
        if ($JobID -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$' -or
            $Commit -notmatch '^[0-9a-f]{40}$' -or $Lane -lt 1 -or
            $VMSlug -notmatch '^[a-z0-9][a-z0-9-]{0,63}$') { throw 'agent-result identity is invalid' }
        $Result = [ordered]@{
            schema_version = 'bridgevm.windows-product-e2e-agent-result.v2'
            job_id = $JobID; commit = $Commit; lane = $Lane; nonce = $Nonce; vm_slug = $VMSlug
            keyboard_pointer_challenge_sha256 = Hash "t17-keyboard-pointer-$Prefix.txt"
            clipboard_roundtrip_sha256 = Hash "t17-clipboard-guest-$Prefix.txt"
            share_host_to_guest_sha256 = Hash "t17-$Prefix.txt"
            share_guest_to_host_sha256 = Hash "t17-guest-$Prefix.txt"
            network_result_sha256 = Hash "t17-network-$Prefix.txt"
            audio_result_sha256 = Hash "t17-audio-$Prefix.txt"
            audio_playback_count = 1; audio_error_count = 0
            snapshot_marker_a_sha256 = Hash "t17-snapshot-a-$Prefix.txt"
            snapshot_marker_b_sha256 = Hash "t17-snapshot-b-$Prefix.txt"
            snapshot_marker_restored_a_sha256 = Hash "t17-snapshot-restored-a-$Prefix.txt"
        }
        Write-Exact "t17-agent-result-$Prefix.json" (($Result | ConvertTo-Json -Compress) + "`n")
    }
}

Write-Output ("T17 action=$Action nonce=$Nonce status=PASS")
exit 0
