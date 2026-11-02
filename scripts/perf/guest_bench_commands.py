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

def net_download(url: str) -> str:
    """Guest<-host TCP through the VMM's NAT: one warm-up GET, then one timed GET."""
    return (
        f"$u='{url}'; $c=New-Object Net.WebClient; $null=$c.DownloadData($u);"
        "$s=[Diagnostics.Stopwatch]::StartNew(); $d=$c.DownloadData($u); $s.Stop();"
        "'BVBENCH net-download {0:F1} MB/s' -f ($d.Length/1MB/$s.Elapsed.TotalSeconds)"
    )


# Every guest metric is higher-is-better; one script may print several.
HIGHER_IS_BETTER = {name for name, _ in SUITE} | {
    "disk-write-seq-1m", "disk-read-seq-1m", "disk-read-rand-4k", "net-download"}

_BVBENCH = re.compile(r"^BVBENCH (\S+) ([0-9]+(?:\.[0-9]+)?) (\S+)\s*$", re.M)


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
    for name, script in SUITE:
        line = command_line(script)
        assert "\n" not in line and len(line) < 8000, name
        decoded = base64.b64decode(line.rsplit(" ", 1)[1]).decode("utf-16-le")
        assert decoded.endswith(script) and decoded.startswith("$ErrorActionPreference='Stop'; "), name
        assert "BVBENCH " in script, name


if __name__ == "__main__":
    _self_test()
    print("guest bench commands: self-test PASS")
