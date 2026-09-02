param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-f]{64}$')]
    [string]$Nonce
)

$ErrorActionPreference = 'Stop'
$Root = 'C:\ProgramData\BridgeVM'
New-Item -ItemType Directory -Force -Path $Root | Out-Null
$Path = Join-Path $Root ('b7-tone-' + $Nonce.Substring(0, 12) + '.wav')
$Rate = 48000
$Frames = $Rate * 2
$DataBytes = $Frames * 4
$Stream = New-Object System.IO.MemoryStream
$Writer = New-Object System.IO.BinaryWriter($Stream)
$Ascii = [System.Text.Encoding]::ASCII
$Writer.Write($Ascii.GetBytes('RIFF'))
$Writer.Write([int](36 + $DataBytes))
$Writer.Write($Ascii.GetBytes('WAVE'))
$Writer.Write($Ascii.GetBytes('fmt '))
$Writer.Write([int]16)
$Writer.Write([int16]1)
$Writer.Write([int16]2)
$Writer.Write([int]$Rate)
$Writer.Write([int]($Rate * 4))
$Writer.Write([int16]4)
$Writer.Write([int16]16)
$Writer.Write($Ascii.GetBytes('data'))
$Writer.Write([int]$DataBytes)
for ($Index = 0; $Index -lt $Frames; $Index++) {
    $Sample = [int16](12000 * [Math]::Sin(2 * [Math]::PI * 440 * $Index / $Rate))
    $Writer.Write($Sample)
    $Writer.Write($Sample)
}
$Writer.Flush()
[System.IO.File]::WriteAllBytes($Path, $Stream.ToArray())
$Writer.Dispose()
$Stream.Dispose()
if ((Get-Item -LiteralPath $Path).Length -ne 384044) {
    throw 'generated WAV length is not the fixed 384044-byte contract'
}
$Player = New-Object System.Media.SoundPlayer $Path
$Player.PlaySync()
$Player.Dispose()
Write-Output ('B7 PLAYBACK PASS nonce=' + $Nonce + ' wav_bytes=384044')
