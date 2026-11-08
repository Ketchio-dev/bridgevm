# Contract for the guest agent's clipboard image, HTML and file-list verbs (CLIPCAPS, CLIPX,
# CLIPSETIMG, CLIPXH, CLIPSETH, CLIPXF) and the CLIPCAPS PUSH change watcher (BridgeVM.BvClipWatch).
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
$watchBlock = [regex]::Match($source, '(?s)# >>> clipboard watch.*?# <<< clipboard watch')
if (-not $watchBlock.Success) { throw 'clipboard watch block not found' }
$cases = @{}
foreach ($verb in @('CLIPX', 'CLIPCAPS', 'CLIPSETIMG', 'CLIPXH', 'CLIPSETH', 'CLIPXF')) {
    $found = [regex]::Matches($source, "(?s)\n                '$verb' \{(?<body>.*?)\r?\n                \}\r?\n")
    if ($found.Count -ne 1) { throw "expected exactly one $verb implementation" }
    $cases[$verb] = [scriptblock]::Create($found[0].Groups['body'].Value)
}
. ([scriptblock]::Create($block.Value))
$ioPath = [regex]::Match($source, "(?s)\nfunction Get-IoPath\b.*?\r?\n\}\r?\n")
if (-not $ioPath.Success) { throw 'Get-IoPath not found' }
. ([scriptblock]::Create($ioPath.Value))
. ([scriptblock]::Create($watchBlock.Value))
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
    function Copy-Files([string[]]$paths) {
        $list = New-Object System.Collections.Specialized.StringCollection
        $list.AddRange($paths)
        [Windows.Forms.Clipboard]::SetFileDropList($list)
        $script:fakeText = $null
    }
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
    public static class Application { public static int Exits; public static void ExitThread() { Exits++; } }
    public static class Clipboard {
        public static IDataObject Current;
        public static int Reads;
        public static void SetDataObject(object data, bool copy, int retryTimes, int retryDelay) { Current = (IDataObject)data; BvTestSeq.Seq++; }
        public static IDataObject GetDataObject() { Reads++; return Current; }
        public static bool ContainsFileDropList() { return Current != null && Current.GetDataPresent("FileDrop", true); }
        public static System.Collections.Specialized.StringCollection GetFileDropList() {
            System.Collections.Specialized.StringCollection list = new System.Collections.Specialized.StringCollection();
            if (ContainsFileDropList()) { list.AddRange((string[])Current.GetData("FileDrop", true)); }
            return list;
        }
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
    function Copy-Files([string[]]$paths) { $data = $TDataObject::new(); $data.SetData('FileDrop', $true, [string[]]$paths); Set-Fake $data; $script:fakeText = $null }
    function Read-ClipHtml {
        $data = $TClipboard::Current
        if ($null -eq $data -or -not $data.GetDataPresent('HTML Format', $false)) { return @{ Bytes = $null; Size = 0 } }
        $bytes = $data.GetData('HTML Format', $false).ToArray()
        $end = [Array]::IndexOf($bytes, [byte]0)
        if ($end -ge 0) { $bytes = [byte[]]$bytes[0..($end - 1)] }
        return @{ Bytes = $bytes; Size = $bytes.Length }
    }
}

try {
    $caps = Run 'CLIPCAPS' ''
    Assert ($caps.Count -eq 1 -and $caps[0] -ceq ('CLIPCAPS IMAGE_PNG ' + (B64 $script:BvClipDir) + ' CF_HTML CF_HDROP')) "CLIPCAPS marker: $caps"

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

    # HTML: CLIPSETH writes the text and the exact "HTML Format" bytes in one clipboard
    # write; CLIPXH serves them as sent, and reads them back unchanged after a change.
    $u8 = [Text.Encoding]::UTF8
    $fragment = '<b>' + [char]0xAD75 + [char]0xAC8C + '</b> and <i>' + [char]0xD55C + [char]0xAE00 + '</i> ' + [char]0xD83D + [char]0xDE42
    $doc = "<html><body>`r`n<!--StartFragment-->" + $fragment + "<!--EndFragment-->`r`n</body>`r`n</html>"
    $sf = 105 + $u8.GetByteCount("<html><body>`r`n<!--StartFragment-->")
    $cf = $u8.GetBytes(("Version:0.9`r`nStartHTML:{0:D10}`r`nEndHTML:{1:D10}`r`nStartFragment:{2:D10}`r`nEndFragment:{3:D10}`r`n" -f 105, (105 + $u8.GetByteCount($doc)), $sf, ($sf + $u8.GetByteCount($fragment))) + $doc)
    $htmlText = [char]0xAD75 + [char]0xAC8C + ' and ' + [char]0xD55C + [char]0xAE00 + "`r`nline 2"
    $cfB64 = [Convert]::ToBase64String($cf)
    $reply = Run 'CLIPSETH' ((B64 $htmlText) + ' ' + $cfB64)
    Assert ($reply.Count -eq 1 -and $reply[0] -ceq 'OK CLIPSETH') "CLIPSETH reply: $reply"
    $wantXh = 'CLIPXH 0 0 0 - - {0} {1} {2}' -f $cf.Length, $cfB64, (B64 $htmlText)
    $reads = if ($desktop) { 0 } else { $TClipboard::Reads }
    Assert ((Run 'CLIPXH' '')[0] -ceq $wantXh) 'CLIPXH serves the pushed HTML and text'
    if (-not $desktop) { Assert ($TClipboard::Reads -eq $reads) 'served without reopening the clipboard' }
    if ($desktop) {
        Assert ([Windows.Forms.Clipboard]::GetText() -ceq $htmlText) 'the copy keeps its text'
        Assert ([Windows.Forms.Clipboard]::ContainsText([Windows.Forms.TextDataFormat]::Html)) 'Windows sees HTML Format'
    } else {
        $stored = $TClipboard::Current.GetData('HTML Format', $false).ToArray()
        Assert ($stored.Length -eq $cf.Length + 1 -and $stored[$cf.Length] -eq 0 -and [Convert]::ToBase64String([byte[]]$stored[0..($cf.Length - 1)]) -ceq $cfB64) 'HTML Format holds the bytes and a NUL'
    }
    # The stand-in Get-Clipboard (top) serves the text; the HTML is read from the clipboard.
    $script:fakeText = $htmlText
    $script:BvClipSeq = [uint32]4294967295; $script:BvClipImgSeq = [uint32]4294967295; $script:BvClipHtmlSeq = [uint32]4294967295
    Assert ((Run 'CLIPXH' '')[0] -ceq $wantXh) 'read again, the HTML and text are byte-identical'
    Assert ((Run 'CLIPX' '')[0] -ceq ('CLIPX 0 0 0 - - ' + (B64 $htmlText))) 'CLIPX keeps its fields'
    Copy-Text 'plain'
    Assert ((Run 'CLIPXH' '')[0] -ceq ('CLIPXH 0 0 0 - - 0 - ' + (B64 'plain'))) 'a text copy has no HTML part'
    $before = @($script:BvClipSeq, $script:BvClipCache, $script:BvClipHtmlPart)
    foreach ($bad in @(((B64 'x') + ' ' + [Convert]::ToBase64String((New-Object byte[] 4194305))), (B64 'x'), ((B64 'x') + ' !!'))) {
        $reply = Run 'CLIPSETH' $bad
        Assert ($reply.Count -eq 1 -and $reply[0] -clike 'ERR CLIPSETH *') "CLIPSETH refusal: $reply"
    }
    Assert ((@($script:BvClipSeq, $script:BvClipCache, $script:BvClipHtmlPart) -join '|') -ceq ($before -join '|')) 'a CLIPSETH refusal keeps the caches'

    # Files: an Explorer copy is CF_HDROP. CLIPXF lists each copied item that exists (top 1) and
    # everything below a copied folder (top 0, sorted, after its folder); a folder has 0 bytes.
    $filesDir = Join-Path $script:BvClipDir ([string][char]0xD30C + [char]0xC77C + ' files')
    $folder = Join-Path $filesDir 'a folder'
    [void][IO.Directory]::CreateDirectory((Join-Path $folder 'sub'))
    [IO.File]::WriteAllBytes((Join-Path $folder 'inner.txt'), [byte[]](7, 8, 9))
    $file = Join-Path $filesDir ([string][char]0xBCF4 + [char]0xACE0 + [char]0xC11C + ' 1.txt')
    [IO.File]::WriteAllBytes($file, [byte[]](1, 2, 3, 4, 5))
    Copy-Files @($file, $folder, (Join-Path $filesDir 'gone.txt'))
    $tick = { param($p) [IO.File]::GetLastWriteTimeUtc($p).Ticks }
    $inner = Join-Path $folder 'inner.txt'
    $list = "0|1|5|$(& $tick $file)|$file`n1|1|0|0|$folder`n0|0|3|$(& $tick $inner)|$inner`n1|0|0|0|$(Join-Path $folder 'sub')`n"
    $want = 'CLIPXF 0 0 0 - - 0 - 4 {0} ' -f [Convert]::ToBase64String($u8.GetBytes($list))
    $got = (Run 'CLIPXF' '')[0]
    Assert ($got -ceq $want) "CLIPXF lists the copied files: $got"
    Assert ((Run 'CLIPXH' '')[0] -ceq 'CLIPXH 0 0 0 - - 0 - ') 'CLIPXH keeps its fields'
    # The same copy after a same-size edit lists a new mtime, so the host fetches the new bytes.
    [IO.File]::WriteAllBytes($file, [byte[]](5, 4, 3, 2, 1))
    [IO.File]::SetLastWriteTimeUtc($file, [IO.File]::GetLastWriteTimeUtc($file).AddSeconds(1))
    Copy-Files @($file)
    $again = (Run 'CLIPXF' '')[0]
    Assert ($again -ceq ('CLIPXF 0 0 0 - - 0 - 1 {0} ' -f [Convert]::ToBase64String($u8.GetBytes("0|1|5|$(& $tick $file)|$file`n")))) "a same-size edit lists its new mtime: $again"
    # Past FilesMax entries the listing stops and names the count only.
    $many = Join-Path $filesDir 'many'
    [void][IO.Directory]::CreateDirectory($many)
    for ($i = 0; $i -lt 300; $i++) { [IO.File]::WriteAllBytes((Join-Path $many "$i.txt"), [byte[]]@()) }
    Copy-Files @($many)
    $got = (Run 'CLIPXF' '')[0]
    Assert ($got -ceq 'CLIPXF 0 0 0 - - 0 - 257 - ') "past FilesMax: $got"
    Copy-Text 'plain'
    Assert ((Run 'CLIPXF' '')[0] -ceq ('CLIPXF 0 0 0 - - 0 - 0 - ' + (B64 'plain'))) 'a text copy has no files part'

    if ($desktop) {
        # Each clipboard call runs on a thread of its own. WinForms keeps a context with its own
        # thread handle for every thread that touches the clipboard until the thread disposes it;
        # no collection frees it. (Handles a collection frees are not counted.)
        $cs = [regex]::Match($source, "(?s)\`$script:BvClipTextSource = @'\r?\n(?<cs>.*?)\r?\n'@")
        Assert $cs.Success 'BvClipTextSource found'
        Add-Type -TypeDefinition $cs.Groups['cs'].Value -ReferencedAssemblies System.Windows.Forms
        $self = Get-Process -Id $PID
        function Get-LiveHandles { [GC]::Collect(); [GC]::WaitForPendingFinalizers(); [GC]::Collect(); Start-Sleep -Milliseconds 300; $self.Refresh(); $self.HandleCount }
        $handlesBefore = Get-LiveHandles
        for ($i = 0; $i -lt 200; $i++) { $w = 0; $hh = 0; [void][BridgeVM.BvClipImage]::Get([ref]$w, [ref]$hh) }
        for ($i = 0; $i -lt 200; $i++) { [BridgeVM.BvClipText]::Set("handles $i") }
        $kept = (Get-LiveHandles) - $handlesBefore
        Assert ($kept -lt 100) "400 clipboard calls kept $kept handles"
        Assert ([Windows.Forms.Clipboard]::GetText() -ceq 'handles 199') 'the last copy is on the clipboard'

        # CLIPCAPS PUSH starts the watcher. A read waiting on a synchronous handle
        # is cancelled once the clipboard changes; without a change it waits for
        # data, and bytes written after a cancel arrive whole.
        Add-Type -ReferencedAssemblies System.Windows.Forms -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Threading;
namespace BridgeVM {
    public static class BvPushFixture {
        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern bool CreatePipe(out IntPtr r, out IntPtr w, IntPtr sa, uint size);
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern bool WriteFile(IntPtr h, byte[] b, uint n, out uint w, IntPtr o);
        // A short-lived STA thread owns the copy, as the agent's CLIPSET does: an owner whose
        // thread then blocks (this session in Read) stalls every later copy for about 5 s.
        public static void CopyNow(string text) {
            Thread t = new Thread(() => System.Windows.Forms.Clipboard.SetText(text));
            t.SetApartmentState(ApartmentState.STA);
            t.Start();
            t.Join();
        }
        public static void CopyLater(string text, int ms) {
            Thread t = new Thread(() => { Thread.Sleep(ms); System.Windows.Forms.Clipboard.SetText(text); });
            t.SetApartmentState(ApartmentState.STA);
            t.Start();
        }
        public static void WriteLater(IntPtr h, byte[] data, int ms) {
            new Thread(() => { Thread.Sleep(ms); uint w; WriteFile(h, data, (uint)data.Length, out w, IntPtr.Zero); }).Start();
        }
    }
}
'@
        # Earlier cases copied from this session's thread; hand the clipboard to an ended one
        # before the watcher starts.
        [BridgeVM.BvPushFixture]::CopyNow('push-0')
        Start-Sleep -Milliseconds 1000
        $caps = Run 'CLIPCAPS' 'PUSH'
        Assert ($caps.Count -eq 1 -and $caps[0] -ceq ('CLIPCAPS IMAGE_PNG ' + (B64 $script:BvClipDir) + ' CF_HTML CF_HDROP CLIPPUSH')) "CLIPCAPS PUSH: $caps"
        Assert $script:BvClipPush 'CLIPCAPS PUSH turns notices on'
        $r = [IntPtr]::Zero; $w = [IntPtr]::Zero
        Assert ([BridgeVM.BvPushFixture]::CreatePipe([ref]$r, [ref]$w, [IntPtr]::Zero, 0)) 'CreatePipe'
        $buf = New-Object byte[] 64
        $clock = [Diagnostics.Stopwatch]::StartNew()
        [BridgeVM.BvPushFixture]::CopyLater('push-1', 300)
        $got = [BridgeVM.BvClipWatch]::Read($r, $buf)
        $ms = $clock.ElapsedMilliseconds
        Assert ($got -eq -1 -and $ms -ge 250 -and $ms -lt 3000) "a clipboard change cancels the waiting read: $got after $ms ms"
        # One copy can move the sequence number again a little later (clipboard
        # history), so let it settle; any late notice still must not lose bytes.
        Start-Sleep -Milliseconds 1000
        $clock.Restart()
        [BridgeVM.BvPushFixture]::WriteLater($w, [Text.Encoding]::ASCII.GetBytes("PING`n"), 500)
        do { $got = [BridgeVM.BvClipWatch]::Read($r, $buf) } while ($got -eq -1)
        $ms = $clock.ElapsedMilliseconds
        Assert ($got -eq 5 -and [Text.Encoding]::ASCII.GetString($buf, 0, 5) -ceq "PING`n" -and $ms -ge 450) "the read waits for data and gets it whole: $got after $ms ms"
        $caps = Run 'CLIPCAPS' ''
        Assert (-not $script:BvClipPush -and $caps[0] -cnotlike '*CLIPPUSH*') 'a bare CLIPCAPS turns notices off'
    } else {
        $rejected = $false
        try { Add-Type -TypeDefinition $script:BvClipWatchSource -CompilerOptions '/langversion:5' -ErrorAction Stop } catch { $rejected = $_ }
        Assert ($rejected -eq $false) "BvClipWatch compiles as C# 5: $rejected"
    }
} finally {
    Remove-Item -LiteralPath $script:BvClipDir -Recurse -Force -ErrorAction SilentlyContinue
}
$mode = if ($desktop) { 'Windows PowerShell, real clipboard' } else { 'PowerShell 7, C# 5 with a stand-in clipboard' }
Write-Output "bvagent clipboard images: PASS ($mode)"
