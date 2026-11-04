# Contract for the guest agent's clipboard image verbs (CLIPCAPS, CLIPX, CLIPSETIMG).
# PowerShell 7: compiles BridgeVM.BvClipImage as C# 5 (/langversion:5) against an
# in-memory Windows Forms stand-in and drives the three verbs through it.
# Windows PowerShell 5.1 (Windows): the agent's own Add-Type compile and the real
# clipboard, read back through Windows Forms in this STA session.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$agentPath = Join-Path $root 'scripts/win-assets/bvagent.ps1'
$source = [IO.File]::ReadAllText($agentPath)
$tokens = $null; $errors = $null
[void][System.Management.Automation.Language.Parser]::ParseFile($agentPath, [ref]$tokens, [ref]$errors)
if ($errors.Count -ne 0) { throw "bvagent.ps1 does not parse: $($errors[0])" }

$block = [regex]::Match($source, '(?s)# >>> clipboard images.*?# <<< clipboard images')
if (-not $block.Success) { throw 'clipboard image block not found' }
$cases = @{}
foreach ($verb in @('CLIPX', 'CLIPCAPS', 'CLIPSETIMG')) {
    $found = [regex]::Matches($source, "(?s)\n                '$verb' \{(?<body>.*?)\r?\n                \}\r?\n")
    if ($found.Count -ne 1) { throw "expected exactly one $verb implementation" }
    $cases[$verb] = [scriptblock]::Create($found[0].Groups['body'].Value)
}
. ([scriptblock]::Create($block.Value))
$script:BvClipDir = Join-Path ([IO.Path]::GetTempPath()) ('bv-clip-test-' + [Guid]::NewGuid().ToString('N'))
$script:logged = New-Object System.Collections.Generic.List[string]
function Log([string]$m) { $script:logged.Add($m) }
function Write-Line($handle, [string]$line, [string]$label) { $line }
function Get-Clipboard { param([switch]$Raw) $script:fakeText }
$h = [IntPtr]::Zero
$script:BvClipSeq = [uint32]4294967295
$script:BvClipCache = ''
$desktop = $PSVersionTable.PSEdition -ne 'Core'

function Assert($condition, [string]$what) { if (-not $condition) { throw "FAIL: $what" } }
function B64([string]$text) { [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($text)) }
function Run([string]$verb, [string]$argument) { $arg = $argument; return ,@(& $cases[$verb]) }

if ($desktop) {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $K = Add-Type -MemberDefinition '[DllImport("user32.dll")] public static extern uint GetClipboardSequenceNumber();' -Name 'BvClipTestUser32' -Namespace 'BridgeVM' -PassThru
    Assert (Initialize-ClipImage) 'the agent compiles its clipboard image helper'
    function New-Png([int]$width, [int]$height) {
        $bitmap = New-Object System.Drawing.Bitmap $width, $height
        for ($x = 0; $x -lt $width; $x++) { for ($y = 0; $y -lt $height; $y++) { $bitmap.SetPixel($x, $y, [Drawing.Color]::FromArgb(255, (40 * $x) % 256, (60 * $y) % 256, 128)) } }
        $stream = New-Object IO.MemoryStream
        $bitmap.Save($stream, [Drawing.Imaging.ImageFormat]::Png)
        $bitmap.Dispose()
        return ,$stream.ToArray()
    }
    function Copy-Bitmap([int]$width, [int]$height) { [Windows.Forms.Clipboard]::SetImage((New-Object Drawing.Bitmap $width, $height)) }
    function Copy-Text([string]$text) { [Windows.Forms.Clipboard]::SetText($text); $script:fakeText = $text }
} else {
    # C# 5 is what Windows PowerShell 5.1 compiles; prove the option bites first.
    $rejected = $false
    try { Add-Type -TypeDefinition 'public static class BvLangCheck { public static string N() { return nameof(N); } }' -CompilerOptions '/langversion:5' -ErrorAction Stop } catch { $rejected = $true }
    Assert $rejected '/langversion:5 rejects nameof'
    $helper = [regex]::Match($script:BvClipImageSource, '(?s)^(?<usings>(using [^\n]*\n)+)(?<rest>.*)$')
    $stub = @'
namespace System.Windows.Forms {
    public interface IDataObject { bool GetDataPresent(string format, bool autoConvert); object GetData(string format, bool autoConvert); }
    public static class DataFormats { public const string UnicodeText = "UnicodeText"; public const string Bitmap = "Bitmap"; }
    public class DataObject : IDataObject {
        readonly System.Collections.Generic.Dictionary<string, object> items = new System.Collections.Generic.Dictionary<string, object>();
        public void SetData(string format, bool autoConvert, object data) { items[format] = data; }
        public bool GetDataPresent(string format, bool autoConvert) { return items.ContainsKey(format); }
        public object GetData(string format, bool autoConvert) { object found; return items.TryGetValue(format, out found) ? found : null; }
    }
    public static class Clipboard {
        public static IDataObject Current;
        public static int Reads;
        public static void SetDataObject(object data, bool copy, int retryTimes, int retryDelay) { Current = (IDataObject)data; BvTestSeq.Seq++; }
        public static IDataObject GetDataObject() { Reads++; return Current; }
    }
}
namespace System.Drawing.Imaging { public sealed class ImageFormat { public static readonly ImageFormat Png = new ImageFormat(); } }
namespace System.Drawing {
    public abstract class Image : System.IDisposable {
        public int Width; public int Height; public byte[] Bytes;
        public void Save(System.IO.Stream stream, System.Drawing.Imaging.ImageFormat format) { stream.Write(Bytes, 0, Bytes.Length); }
        public void Dispose() { }
    }
    public sealed class Bitmap : Image {
        public Bitmap(System.IO.Stream stream) {
            System.IO.MemoryStream copy = new System.IO.MemoryStream(); stream.CopyTo(copy); Bytes = copy.ToArray();
            if (!BridgeVM.BvClipImage.PngSize(Bytes, out Width, out Height)) { throw new System.ArgumentException("Parameter is not valid."); }
        }
        public Bitmap(int width, int height, byte[] encoded) { Width = width; Height = height; Bytes = encoded; }
    }
}
public static class BvTestSeq { public static uint Seq = 1; public static uint GetClipboardSequenceNumber() { return Seq; } }
'@
    Add-Type -TypeDefinition ($helper.Groups['usings'].Value + $helper.Groups['rest'].Value + "`n" + $stub) -CompilerOptions '/langversion:5' -ErrorAction Stop
    [void][IO.Directory]::CreateDirectory($script:BvClipDir)
    $script:BvClipImageReady = $true
    # Resolve the stand-ins from their own assembly: PowerShell 7 also ships System.Drawing.
    $asm = [BridgeVM.BvClipImage].Assembly
    $K = $asm.GetType('BvTestSeq')
    $TClipboard = $asm.GetType('System.Windows.Forms.Clipboard')
    $TDataObject = $asm.GetType('System.Windows.Forms.DataObject')
    $TBitmap = $asm.GetType('System.Drawing.Bitmap')
    function New-Png([int]$width, [int]$height) {
        $png = [byte[]](137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82) + [BitConverter]::GetBytes([Net.IPAddress]::HostToNetworkOrder($width)) + [BitConverter]::GetBytes([Net.IPAddress]::HostToNetworkOrder($height)) + [byte[]](8, 6, 0, 0, 0, 1, 2, 3, 4)
        return ,([byte[]]($png + (New-Object byte[] (7 * $width))))
    }
    function Set-Fake([object]$data) { $TClipboard::SetDataObject($data, $true, 1, 1) }
    function Copy-Bitmap([int]$width, [int]$height) {
        $data = $TDataObject::new()
        $data.SetData('Bitmap', $true, $TBitmap::new($width, $height, [byte[]](New-Png $width $height)))
        Set-Fake $data
    }
    function Copy-Text([string]$text) { $data = $TDataObject::new(); $data.SetData('UnicodeText', $true, $text); Set-Fake $data; $script:fakeText = $text }
}

try {
    $caps = Run 'CLIPCAPS' ''
    Assert ($caps.Count -eq 1 -and $caps[0] -ceq ('CLIPCAPS IMAGE_PNG ' + (B64 $script:BvClipDir))) "CLIPCAPS marker: $caps"

    # Mac to Windows: the PUT file becomes text and image in one clipboard write.
    $png = New-Png 3 2
    $hostPath = $script:BvClipDir + '\host.png'
    $guestPath = $script:BvClipDir + '\guest.png'
    [IO.File]::WriteAllBytes($hostPath, $png)
    $reply = Run 'CLIPSETIMG' ((B64 $hostPath) + ' ' + (B64 'caption'))
    Assert ($reply.Count -eq 1 -and $reply[0] -ceq 'OK CLIPSETIMG 3 2') "CLIPSETIMG reply: $reply"
    Assert (-not (Test-Path -LiteralPath $hostPath)) 'host.png is consumed'
    Assert ([Convert]::ToBase64String([IO.File]::ReadAllBytes($guestPath)) -ceq [Convert]::ToBase64String($png)) 'the pushed PNG becomes guest.png'
    $sha = [BridgeVM.BvClipImage]::Sha256Hex($png)
    if ($desktop) {
        $image = [Windows.Forms.Clipboard]::GetImage()
        Assert ($image.Width -eq 3 -and $image.Height -eq 2 -and $image.GetPixel(2, 1).ToArgb() -eq [Drawing.Color]::FromArgb(255, 80, 60, 128).ToArgb()) 'GetImage shows the pushed pixels'
        Assert ([Windows.Forms.Clipboard]::GetText() -ceq 'caption') 'the copy keeps its text'
        $script:fakeText = 'caption'
    } else {
        $current = $TClipboard::Current
        Assert ($current.GetData('UnicodeText', $true) -ceq 'caption' -and $null -ne $current.GetData('Bitmap', $true)) 'text and bitmap formats'
        Assert ([Convert]::ToBase64String($current.GetData('PNG', $false).ToArray()) -ceq [Convert]::ToBase64String($png)) 'the PNG format holds the exact bytes'
    }
    $reads = if ($desktop) { 0 } else { $TClipboard::Reads }
    $poll = Run 'CLIPX' ''
    Assert ($poll[0] -ceq ('CLIPX 3 2 {0} {1} {2} {3}' -f $png.Length, $sha, (B64 $guestPath), (B64 'caption'))) "CLIPX after CLIPSETIMG: $poll"
    if (-not $desktop) { Assert ($TClipboard::Reads -eq $reads) 'served without reopening the clipboard' }

    # Windows to Mac: a bitmap-only copy is encoded as PNG into guest.png.
    Copy-Bitmap 4 5
    $script:fakeText = ''
    $poll = (Run 'CLIPX' '')[0].Split(' ')
    $bytes = [IO.File]::ReadAllBytes($guestPath)
    Assert ($poll[1] -eq '4' -and $poll[2] -eq '5' -and $poll[3] -eq [string]$bytes.Length -and $poll[4] -ceq [BridgeVM.BvClipImage]::Sha256Hex($bytes) -and $poll[5] -ceq (B64 $guestPath)) "CLIPX for a bitmap copy: $poll"
    $width = 0; $height = 0
    Assert ([BridgeVM.BvClipImage]::PngSize($bytes, [ref]$width, [ref]$height) -and $width -eq 4 -and $height -eq 5) 'guest.png is a 4x5 PNG'

    # Over the side limit nothing is encoded; without an image the part is empty.
    Copy-Bitmap 16385 2
    Assert ((Run 'CLIPX' '')[0] -clike 'CLIPX 16385 2 0 - - *') 'an image over the limit is described, not encoded'
    Assert (-not (Test-Path -LiteralPath $guestPath)) 'and the previous file is gone'
    Copy-Text 'plain'
    Assert ((Run 'CLIPX' '')[0] -ceq ('CLIPX 0 0 0 - - ' + (B64 'plain'))) 'a text copy has no image part'

    # A file that is no PNG is refused and changes nothing.
    [IO.File]::WriteAllBytes($hostPath, [Text.Encoding]::ASCII.GetBytes('MZ, not a PNG at all, but longer than an IHDR'))
    $before = @($script:BvClipSeq, $script:BvClipCache, $script:BvClipImgDesc)
    $reply = Run 'CLIPSETIMG' ((B64 $hostPath) + ' -')
    Assert ($reply.Count -eq 1 -and $reply[0] -clike 'ERR CLIPSETIMG *') "refusal: $reply"
    Assert ((@($script:BvClipSeq, $script:BvClipCache, $script:BvClipImgDesc) -join '|') -ceq ($before -join '|')) 'a refusal keeps the caches'
} finally {
    Remove-Item -LiteralPath $script:BvClipDir -Recurse -Force -ErrorAction SilentlyContinue
}
$mode = if ($desktop) { 'Windows PowerShell, real clipboard' } else { 'PowerShell 7, C# 5 with a stand-in clipboard' }
Write-Output "bvagent clipboard images: PASS ($mode)"
