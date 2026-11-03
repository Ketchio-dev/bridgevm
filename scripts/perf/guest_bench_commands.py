"""In-guest benchmark commands and their parsers, shared by every VMM under test.

Each command is one cmd.exe line for the resident agent's RUN verb. PowerShell
gets its script through -EncodedCommand, so no quoting layer can change it.
Numbers come only from lines the guest prints; a missing or malformed line is a
missing value, never a guess.
"""

from __future__ import annotations

import base64
import re

# .NET Stopwatch timing inside the guest excludes host-side agent latency. Each
# script prints exactly one `BVBENCH <name> <value> <unit>` line.
_SHA256 = (
    "$b=New-Object byte[] (64MB); (New-Object Random 7).NextBytes($b);"
    "$h=[Security.Cryptography.SHA256]::Create(); $null=$h.ComputeHash($b);"
    "$s=[Diagnostics.Stopwatch]::StartNew(); for($i=0;$i -lt 8;$i++){$null=$h.ComputeHash($b)};"
    "$s.Stop(); 'BVBENCH cpu-sha256-1t {0:F1} MB/s' -f (512/$s.Elapsed.TotalSeconds)"
)

# One thread per guest CPU; the C# helper is compiled before the timed region.
_SHA256_MT = (
    "Add-Type -TypeDefinition 'using System; using System.Diagnostics; using System.Security.Cryptography;"
    " using System.Threading.Tasks; public static class BvMt { public static double Run(int n) {"
    " var b = new byte[n][]; for (int i = 0; i < n; i++) { b[i] = new byte[32 << 20]; }"
    " var s = Stopwatch.StartNew(); Parallel.For(0, n, new ParallelOptions { MaxDegreeOfParallelism = n }, i => {"
    " using (var h = SHA256.Create()) { for (int k = 0; k < 8; k++) { h.ComputeHash(b[i]); } } });"
    " s.Stop(); return n * 256.0 / s.Elapsed.TotalSeconds; } }';"
    "'BVBENCH cpu-sha256-mt {0:F1} MB/s' -f [BvMt]::Run([Environment]::ProcessorCount)"
)

_MEMCOPY = (
    "$a=New-Object byte[] (256MB); $c=New-Object byte[] (256MB); [Buffer]::BlockCopy($a,0,$c,0,$a.Length);"
    "$s=[Diagnostics.Stopwatch]::StartNew(); for($i=0;$i -lt 16;$i++){[Buffer]::BlockCopy($a,0,$c,0,$a.Length)};"
    "$s.Stop(); 'BVBENCH mem-copy {0:F1} MB/s' -f (4096/$s.Elapsed.TotalSeconds)"
)

# Unbuffered (FILE_FLAG_NO_BUFFERING) I/O with page-aligned buffers measures the
# virtual disk, not the guest cache. 1 GiB file; seq 1 MiB and random 4 KiB QD1.
_DISK_IO = (
    "Add-Type -TypeDefinition 'using System; using System.Diagnostics; using System.Runtime.InteropServices;"
    " using Microsoft.Win32.SafeHandles; public static class BvIo {"
    " [DllImport(\"kernel32\", SetLastError = true, CharSet = CharSet.Unicode)] static extern SafeFileHandle CreateFileW("
    "string n, uint a, uint s, IntPtr sa, uint d, uint f, IntPtr t);"
    " [DllImport(\"kernel32\", SetLastError = true)] static extern bool ReadFile(SafeFileHandle h, IntPtr b, int n, out int r, IntPtr o);"
    " [DllImport(\"kernel32\", SetLastError = true)] static extern bool WriteFile(SafeFileHandle h, IntPtr b, int n, out int w, IntPtr o);"
    " [DllImport(\"kernel32\", SetLastError = true)] static extern bool SetFilePointerEx(SafeFileHandle h, long d, IntPtr p, uint m);"
    " [DllImport(\"kernel32\")] static extern IntPtr VirtualAlloc(IntPtr a, UIntPtr s, uint t, uint p);"
    " const uint NoBuffer = 0x20000000, WriteThrough = 0x80000000;"
    " static SafeFileHandle Open(string p, bool w) { var h = CreateFileW(p, w ? 0x40000000u : 0x80000000u, 0, IntPtr.Zero,"
    " w ? 2u : 3u, NoBuffer | (w ? WriteThrough : 0), IntPtr.Zero); if (h.IsInvalid) throw new Exception(\"open \" + Marshal.GetLastWin32Error()); return h; }"
    " public static string Run(string p) { const int M = 1 << 20; var buf = VirtualAlloc(IntPtr.Zero, (UIntPtr)M, 0x3000, 4);"
    " var rnd = new Random(5); var bytes = new byte[M]; rnd.NextBytes(bytes); Marshal.Copy(bytes, 0, buf, M); int n;"
    " var sw = Stopwatch.StartNew(); using (var h = Open(p, true)) { for (int i = 0; i < 1024; i++) { if (!WriteFile(h, buf, M, out n, IntPtr.Zero)) throw new Exception(\"write\"); } } sw.Stop(); double w = 1024 / sw.Elapsed.TotalSeconds;"
    " sw.Restart(); using (var h = Open(p, false)) { for (int i = 0; i < 1024; i++) { if (!ReadFile(h, buf, M, out n, IntPtr.Zero) || n != M) throw new Exception(\"read\"); } } sw.Stop(); double r = 1024 / sw.Elapsed.TotalSeconds;"
    " int ops = 0; sw.Restart(); using (var h = Open(p, false)) { while (sw.ElapsedMilliseconds < 5000) { SetFilePointerEx(h, (long)rnd.Next(262144) * 4096, IntPtr.Zero, 0);"
    " if (!ReadFile(h, buf, 4096, out n, IntPtr.Zero)) throw new Exception(\"rread\"); ops++; } } sw.Stop();"
    " return string.Format(\"BVBENCH disk-write-seq-1m {0:F1} MB/s\\nBVBENCH disk-read-seq-1m {1:F1} MB/s\\nBVBENCH disk-read-rand-4k {2:F0} IOPS\", w, r, ops / sw.Elapsed.TotalSeconds); } }';"
    "$p=Join-Path $env:TEMP 'bvbench.bin'; [BvIo]::Run($p); Remove-Item $p"
)

# 1000 x (create, write 4 KiB, FlushFileBuffers, close): the guest flush path down
# to the host's durable flush. Typed C# avoids PowerShell overload guessing.
_SMALL_FILES = (
    "Add-Type -TypeDefinition 'using System; using System.Diagnostics; using System.IO; public static class BvFiles {"
    " public static double Run(string d) { if (Directory.Exists(d)) { Directory.Delete(d, true); } Directory.CreateDirectory(d);"
    " var buf = new byte[4096]; var s = Stopwatch.StartNew(); for (int i = 0; i < 1000; i++) {"
    " using (var f = new FileStream(Path.Combine(d, \"f\" + i), FileMode.Create, FileAccess.Write, FileShare.None, 4096,"
    " FileOptions.WriteThrough)) { f.Write(buf, 0, buf.Length); f.Flush(true); } } s.Stop(); Directory.Delete(d, true);"
    " return 1000 / s.Elapsed.TotalSeconds; } }';"
    "'BVBENCH disk-small-files-flushed {0:F1} files/s' -f [BvFiles]::Run((Join-Path $env:TEMP 'bvbench-small'))"
)

_PROCESS_SPAWN = (
    "$s=[Diagnostics.Stopwatch]::StartNew(); for($i=0;$i -lt 50;$i++){$null=cmd.exe /c exit 0};"
    "$s.Stop(); 'BVBENCH process-spawn {0:F1} proc/s' -f (50/$s.Elapsed.TotalSeconds)"
)

SUITE = [
    ("cpu-sha256-1t", _SHA256),
    ("cpu-sha256-mt", _SHA256_MT),
    ("mem-copy", _MEMCOPY),
    ("disk-io", _DISK_IO),
    ("disk-small-files-flushed", _SMALL_FILES),
    ("process-spawn", _PROCESS_SPAWN),
]

def net_download(url: str, sha256: str) -> str:
    """Guest<-host TCP through the VMM's NAT: one warm-up GET, then one timed GET whose
    size and SHA-256 are checked against the host's copy."""
    return (
        f"$u='{url}'; $c=New-Object Net.WebClient; $null=$c.DownloadData($u);"
        "$s=[Diagnostics.Stopwatch]::StartNew(); $d=$c.DownloadData($u); $s.Stop();"
        "'BVBENCH net-download {0:F1} MB/s' -f ($d.Length/1MB/$s.Elapsed.TotalSeconds);"
        "$h=-join ([Security.Cryptography.SHA256]::Create().ComputeHash($d) | % { $_.ToString('x2') });"
        f"'BVBENCH net-download-sha256-match {{0}} bool' -f [int]($h -eq '{sha256}');"
        "'BVBENCH net-download-mib {0} MiB' -f ($d.Length/1MB)"
    )


def cursor_logger(ms: int) -> str:
    """Poll GetCursorPos every ~1 ms for `ms`; report distinct positions per second while moving."""
    return (
        "Add-Type -TypeDefinition 'using System; using System.Collections.Generic; using System.Diagnostics;"
        " using System.Runtime.InteropServices; using System.Threading; public static class BvCur {"
        " [StructLayout(LayoutKind.Sequential)] public struct P { public int X; public int Y; }"
        " [DllImport(\"user32.dll\")] static extern bool GetCursorPos(out P p);"
        " [DllImport(\"winmm.dll\")] static extern uint timeBeginPeriod(uint p);"
        " public static string Run(int ms) { timeBeginPeriod(1); P last; GetCursorPos(out last);"
        " var t = new List<double>(); var sw = Stopwatch.StartNew(); while (sw.ElapsedMilliseconds < ms) {"
        " P p; GetCursorPos(out p); if (p.X != last.X || p.Y != last.Y) { t.Add(sw.Elapsed.TotalMilliseconds); last = p; }"
        " Thread.Sleep(1); } if (t.Count < 3) { return \"BVBENCH cursor-hz 0.0 Hz\"; }"
        " var d = new double[t.Count - 1]; for (int i = 1; i < t.Count; i++) { d[i - 1] = t[i] - t[i - 1]; } Array.Sort(d);"
        " double span = (t[t.Count - 1] - t[0]) / 1000.0; return string.Format(\"BVBENCH cursor-hz {0:F1} Hz\\n"
        "BVBENCH cursor-interval-p50 {1:F1} ms\\nBVBENCH cursor-interval-p95 {2:F1} ms\", (t.Count - 1) / span,"
        " d[d.Length / 2], d[(int)(d.Length * 0.95)]); } }';"
        f"[BvCur]::Run({ms})"
    )


def stall_under_flush(ms: int) -> str:
    """Guest responsiveness while another thread flushes small files: the longest gap of a
    Sleep(1) loop (1 ms timer resolution). A VMM that blocks every vCPU during a host flush
    shows up here even though throughput looks fine."""
    return (
        "Add-Type -TypeDefinition 'using System; using System.Collections.Generic; using System.Diagnostics;"
        " using System.IO; using System.Runtime.InteropServices; using System.Threading; public static class BvStall {"
        " [DllImport(\"winmm.dll\")] static extern uint timeBeginPeriod(uint p);"
        " public static string Run(string d, int ms) { timeBeginPeriod(1); if (Directory.Exists(d)) { Directory.Delete(d, true); }"
        " Directory.CreateDirectory(d); var stop = false; int files = 0; var buf = new byte[4096];"
        " var writer = new Thread(() => { while (!stop) { using (var f = new FileStream(Path.Combine(d, \"f\" + (files % 64)),"
        " FileMode.Create, FileAccess.Write, FileShare.None, 4096, FileOptions.WriteThrough)) { f.Write(buf, 0, buf.Length);"
        " f.Flush(true); } files++; } }); writer.Start(); Thread.Sleep(200);"
        " var gaps = new List<double>(); var sw = Stopwatch.StartNew(); double last = 0;"
        " while (sw.ElapsedMilliseconds < ms) { Thread.Sleep(1); double now = sw.Elapsed.TotalMilliseconds; gaps.Add(now - last); last = now; }"
        " stop = true; writer.Join(); Directory.Delete(d, true); gaps.Sort(); int over = 0; foreach (var g in gaps) { if (g > 20) { over++; } }"
        " return string.Format(\"BVBENCH stall-max {0:F1} ms\\nBVBENCH stall-p99 {1:F1} ms\\nBVBENCH stall-over-20ms {2} count\\n"
        "BVBENCH stall-flush-rate {3:F1} files/s\", gaps[gaps.Count - 1], gaps[(int)(gaps.Count * 0.99)], over, files * 1000.0 / (ms + 200)); } }';"
        f"[BvStall]::Run((Join-Path $env:TEMP 'bvbench-stall'), {ms})"
    )


def stall_idle(ms: int) -> str:
    """The same Sleep(1) loop as `stall_under_flush` with no writer: how late an otherwise idle
    guest's timer wakes a thread. Separates the timer and wake path from disk interference."""
    return (
        "Add-Type -TypeDefinition 'using System; using System.Collections.Generic; using System.Diagnostics;"
        " using System.Runtime.InteropServices; using System.Threading; public static class BvStallIdle {"
        " [DllImport(\"winmm.dll\")] static extern uint timeBeginPeriod(uint p);"
        " public static string Run(int ms) { timeBeginPeriod(1); Thread.Sleep(200);"
        " var gaps = new List<double>(); var sw = Stopwatch.StartNew(); double last = 0;"
        " while (sw.ElapsedMilliseconds < ms) { Thread.Sleep(1); double now = sw.Elapsed.TotalMilliseconds; gaps.Add(now - last); last = now; }"
        " gaps.Sort(); int over = 0; foreach (var g in gaps) { if (g > 5) { over++; } }"
        " return string.Format(\"BVBENCH stall-idle-max {0:F1} ms\\nBVBENCH stall-idle-p99 {1:F1} ms\\nBVBENCH stall-idle-over-5ms {2} count\","
        " gaps[gaps.Count - 1], gaps[(int)(gaps.Count * 0.99)], over); } }';"
        f"[BvStallIdle]::Run({ms})"
    )


def display_animation(ms: int) -> str:
    """A borderless full-screen WinForms window repainting as fast as it can: a moving bar and a
    frame counter. Reports the guest's own render rate; the host side counts what reaches it."""
    return (
        "Add-Type -ReferencedAssemblies System.Windows.Forms,System.Drawing -TypeDefinition 'using System;"
        " using System.Diagnostics; using System.Drawing; using System.Windows.Forms; public class BvAnim : Form {"
        " int n; Stopwatch sw = Stopwatch.StartNew(); int ms; public BvAnim(int ms) { this.ms = ms;"
        " FormBorderStyle = FormBorderStyle.None; WindowState = FormWindowState.Maximized; TopMost = true;"
        " DoubleBuffered = true; BackColor = Color.Black; }"
        " protected override void OnPaint(PaintEventArgs e) { n++; int w = ClientSize.Width, h = ClientSize.Height;"
        " int x = (int)(sw.ElapsedMilliseconds * w / 1000) % w; e.Graphics.FillRectangle(Brushes.White, x, 0, 24, h);"
        " e.Graphics.DrawString(n.ToString(), Font, Brushes.Yellow, 10, 10);"
        " if (sw.ElapsedMilliseconds < ms) { Invalidate(); } else { Close(); } }"
        " public static string Run(int ms) { var f = new BvAnim(ms); Application.Run(f);"
        " return string.Format(\"BVBENCH guest-render-fps {0:F1} fps\", f.n * 1000.0 / f.sw.ElapsedMilliseconds); } }';"
        f"[BvAnim]::Run({ms})"
    )


# Every guest metric is higher-is-better; one script may print several.
HIGHER_IS_BETTER = {name for name, _ in SUITE} | {
    "disk-write-seq-1m", "disk-read-seq-1m", "disk-read-rand-4k", "net-download", "net-download-sha256-match",
    "cursor-hz", "stall-flush-rate",
    "guest-render-fps", "display-fps", "pointer-display-fps",
    "agent-cmd-hz"}

_BVBENCH = re.compile(r"^BVBENCH (\S+) ([0-9]+(?:\.[0-9]+)?) (\S+)\s*$", re.M)


def desktop_mode(native: bool) -> str:
    """The Windows desktop resolution and its mode count; with `native`, first switch the desktop
    (persistently) to the display's largest mode, as picking the recommended resolution does.
    PowerShell passes `$null` to a string parameter as "", which names no display."""
    return (
        "Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class BvDm {"
        " [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct M {"
        " [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string N; public short V1, V2, Size, Extra;"
        " public int Fields, X, Y, O, F; public short C1, C2, C3, C4, C5;"
        " [MarshalAs(UnmanagedType.ByValTStr, SizeConst=32)] public string Form; public short Lp;"
        " public int Bpp, W, H, Fl, Hz, I1, I2, I3, I4, R1, R2, P1, P2; }"
        " [DllImport(\"user32.dll\", CharSet=CharSet.Unicode)] public static extern bool EnumDisplaySettings(string d, int i, ref M m);"
        " [DllImport(\"user32.dll\", CharSet=CharSet.Unicode)] public static extern int ChangeDisplaySettingsEx(string d, ref M m, IntPtr h, int f, IntPtr l); }';"
        "function Cur { $m = New-Object BvDm+M; $m.Size = 220; $null = [BvDm]::EnumDisplaySettings([NullString]::Value, -1, [ref]$m); $m };"
        "$c = Cur; 'BVBENCH desktop-width-before {0} px' -f $c.W; 'BVBENCH desktop-height-before {0} px' -f $c.H;"
        "$i = 0; $best = $null; while ($true) { $m = New-Object BvDm+M; $m.Size = 220;"
        " if (-not [BvDm]::EnumDisplaySettings([NullString]::Value, $i, [ref]$m)) { break }; $i++;"
        " if (-not $best -or $m.W * $m.H -gt $best.W * $best.H) { $best = $m } };"
        "'BVBENCH desktop-modes {0} count' -f $i; 'BVBENCH desktop-max-width {0} px' -f $best.W;"
        "'BVBENCH desktop-max-height {0} px' -f $best.H;"
        + ("if ($best.W * $best.H -gt $c.W * $c.H) { $best.Fields = 0x180000;"
           " $r = [BvDm]::ChangeDisplaySettingsEx([NullString]::Value, [ref]$best, [IntPtr]::Zero, 1, [IntPtr]::Zero);"
           " 'BVBENCH desktop-change-ok {0} bool' -f [int]($r -eq 0); Start-Sleep -Milliseconds 1500 };"
           if native else "")
        + "$c = Cur; 'BVBENCH desktop-width {0} px' -f $c.W; 'BVBENCH desktop-height {0} px' -f $c.H"
    )


def command_line(script: str) -> str:
    # Any guest-side error aborts the script before its BVBENCH line: no number from a failed run.
    script = "$ErrorActionPreference='Stop'; " + script
    encoded = base64.b64encode(script.encode("utf-16-le")).decode("ascii")
    return "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand " + encoded


def parse_bvbench(text: str) -> dict[str, tuple[float, str]]:
    return {m.group(1): (float(m.group(2)), m.group(3)) for m in _BVBENCH.finditer(text.replace("\r", ""))}


def _self_test() -> None:
    assert parse_bvbench("noise\r\nBVBENCH cpu-sha256-1t 812.5 MB/s\r\n") == {"cpu-sha256-1t": (812.5, "MB/s")}
    assert parse_bvbench("BVBENCH x 1,5 MB/s") == {}
    net = net_download("http://h/blob", "ab" * 32)
    assert "'BVBENCH net-download-sha256-match {0} bool' -f [int]($h -eq '" + "ab" * 32 + "')" in net
    assert parse_bvbench("BVBENCH net-download-sha256-match 1 bool\r\n") == {"net-download-sha256-match": (1.0, "bool")}
    idle = stall_idle(8000)
    assert "BVBENCH stall-idle-max" in idle and "FileStream" not in idle and len(command_line(idle)) < 8000
    for native in (False, True):
        assert len(command_line(desktop_mode(native))) < 8000
        assert ("[BvDm]::ChangeDisplaySettingsEx(" in desktop_mode(native)) == native
    for name, script in SUITE:
        line = command_line(script)
        assert "\n" not in line and len(line) < 8000, name
        decoded = base64.b64decode(line.rsplit(" ", 1)[1]).decode("utf-16-le")
        assert decoded.endswith(script) and decoded.startswith("$ErrorActionPreference='Stop'; "), name
        assert "BVBENCH " in script, name


if __name__ == "__main__":
    _self_test()
    print("guest bench commands: self-test PASS")
