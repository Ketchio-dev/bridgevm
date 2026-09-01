#!/usr/bin/env python3
"""Render the private Windows warm-sequential NVMe workload as CRLF PowerShell.

The generated PowerShell is a per-live-job artifact.  This renderer is kept in
git; its output, the benchmark data file, and guest results are not.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


SCHEMA = "bridgevm.windows-nvme-warm-seq.v1"
WORKLOAD_PROFILE = "windows-nvme-warm-seq-v1"
PATTERN_ID = "offset-xorshift64-v1"
NONCE_RE = re.compile(r"^[0-9A-Fa-f]{16,64}$")
MAX_PRIVATE_JSON_BYTES = 8 * 1024 * 1024
# A signed Int64 needs at most 20 decimal characters; reserve a comma and one
# extra byte per sample plus ample fixed JSON metadata.  Runtime checks remain
# authoritative, but rejecting an oversized geometry here avoids doing the I/O
# only to discover that its private raw vector cannot cross the share.
RAW_SAMPLE_JSON_BUDGET = 22
RAW_JSON_FIXED_BUDGET = 16 * 1024


@dataclass(frozen=True)
class WorkloadConfig:
    nonce: str
    file_mib: int = 512
    transfer_kib: int = 128
    read_passes: int = 5
    write_passes: int = 2

    @property
    def file_bytes(self) -> int:
        return self.file_mib * 1024 * 1024

    @property
    def transfer_bytes(self) -> int:
        return self.transfer_kib * 1024

    @property
    def blocks_per_pass(self) -> int:
        return self.file_bytes // self.transfer_bytes

    def canonical_config(self) -> dict[str, object]:
        return {
            "schema": SCHEMA,
            "workload_profile": WORKLOAD_PROFILE,
            "pattern_id": PATTERN_ID,
            "file_mib": self.file_mib,
            "transfer_kib": self.transfer_kib,
            "read_passes": self.read_passes,
            "write_passes": self.write_passes,
            "queue_depth": 1,
            "read_flags": ["FILE_FLAG_NO_BUFFERING"],
            "write_flags": [
                "FILE_FLAG_NO_BUFFERING",
                "FILE_FLAG_WRITE_THROUGH",
            ],
            "flush_semantics": "FlushFileBuffers after precondition and each measured write pass",
            "verification_semantics": "full readback after every measured write pass",
            "cache_profile": "guest-unbuffered-host-warm",
        }

    @property
    def canonical_config_bytes(self) -> bytes:
        return json.dumps(
            self.canonical_config(), sort_keys=True, separators=(",", ":")
        ).encode("utf-8")

    @property
    def config_sha256(self) -> str:
        return hashlib.sha256(self.canonical_config_bytes).hexdigest()


def validate_config(config: WorkloadConfig) -> None:
    if not NONCE_RE.fullmatch(config.nonce):
        raise ValueError("nonce must contain 16-64 hexadecimal characters")
    if not 64 <= config.file_mib <= 8192:
        raise ValueError("file-mib must be in 64..8192")
    if not 4 <= config.transfer_kib <= 1024:
        raise ValueError("transfer-kib must be in 4..1024")
    if config.transfer_kib & (config.transfer_kib - 1):
        raise ValueError("transfer-kib must be a power of two")
    if config.file_bytes % config.transfer_bytes:
        raise ValueError("file size must be an exact multiple of transfer size")
    if not 1 <= config.read_passes <= 32:
        raise ValueError("read-passes must be in 1..32")
    if not 1 <= config.write_passes <= 32:
        raise ValueError("write-passes must be in 1..32")
    raw_samples = (
        config.blocks_per_pass * (config.read_passes + config.write_passes)
        + config.write_passes
    )
    estimated_raw_bytes = (
        raw_samples * RAW_SAMPLE_JSON_BUDGET + RAW_JSON_FIXED_BUDGET
    )
    if estimated_raw_bytes >= MAX_PRIVATE_JSON_BYTES:
        raise ValueError("workload geometry would exceed the private raw JSON limit")


def ps_quote(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def render(config: WorkloadConfig) -> bytes:
    validate_config(config)
    nonce = config.nonce.lower()
    config_json = config.canonical_config_bytes.decode("utf-8")
    template = r'''param([switch]$CompileOnly)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Schema = __SCHEMA__
$WorkloadProfile = __WORKLOAD_PROFILE__
$Nonce = __NONCE__
$ConfigSha256 = __CONFIG_SHA__
$ConfigJson = __CONFIG_JSON__
$Root = 'C:\BridgeVMNVMe'
$DataPath = 'C:\ProgramData\BridgeVMPerf\nvme-seq-v1.bin'
$RawPath = Join-Path $Root 'nvme-raw.json'
$ResultPath = Join-Path $Root 'nvme-result.json'
$DonePath = Join-Path $Root 'nvme-result.done'
$MaxJsonBytes = (8 * 1024 * 1024) - 1
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)

$Source = @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;

namespace BridgeVM.NvmePerf {
    public sealed class LatencySummary {
        public int Count { get; set; }
        public long P50Ns { get; set; }
        public long P95Ns { get; set; }
        public long P99Ns { get; set; }
        public long MaxNs { get; set; }
    }

    public sealed class WorkloadReport {
        public long FileBytes { get; set; }
        public int TransferBytes { get; set; }
        public int BlocksPerPass { get; set; }
        public int PreconditionWriteOps { get; set; }
        public int WarmupReadOps { get; set; }
        public int MeasuredReadOps { get; set; }
        public int MeasuredWriteOps { get; set; }
        public int WriteVerifyReadOps { get; set; }
        public int FinalVerifyReadOps { get; set; }
        public int VerifiedReadOps { get; set; }
        public int FlushCalls { get; set; }
        public uint BytesPerSector { get; set; }
        public uint FileAlignmentBytes { get; set; }
        public uint RequiredAlignmentBytes { get; set; }
        public long ReadPhaseElapsedNs { get; set; }
        public long WritePhaseElapsedNs { get; set; }
        public long ReadServiceElapsedNs { get; set; }
        public long WriteAndFlushServiceElapsedNs { get; set; }
        public double ReadPhaseMibPerSec { get; set; }
        public double ReadServiceMibPerSec { get; set; }
        public double WriteDurableMibPerSec { get; set; }
        public double WriteAndFlushServiceMibPerSec { get; set; }
        public string WarmupSha256 { get; set; }
        public string FinalSha256 { get; set; }
        public LatencySummary ReadLatencyNs { get; set; }
        public LatencySummary WriteLatencyNs { get; set; }
        public LatencySummary FlushLatencyNs { get; set; }
        public long[] ReadLatencyRawNs { get; set; }
        public long[] WriteLatencyRawNs { get; set; }
        public long[] FlushLatencyRawNs { get; set; }
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct FileAlignmentInfo {
        public uint AlignmentRequirement;
    }

    public static class WarmSequentialWorkload {
        private const uint GENERIC_READ = 0x80000000u;
        private const uint GENERIC_WRITE = 0x40000000u;
        private const uint CREATE_ALWAYS = 2u;
        private const uint OPEN_EXISTING = 3u;
        private const uint FILE_ATTRIBUTE_NORMAL = 0x00000080u;
        private const uint FILE_FLAG_NO_BUFFERING = 0x20000000u;
        private const uint FILE_FLAG_WRITE_THROUGH = 0x80000000u;
        private const uint FILE_BEGIN = 0u;
        private const uint MEM_COMMIT_RESERVE = 0x3000u;
        private const uint MEM_RELEASE = 0x8000u;
        private const uint PAGE_READWRITE = 0x04u;
        private const int FILE_ALIGNMENT_INFO_CLASS = 17;
        private const uint PRECONDITION_TAG = 0x42565031u;
        private const uint MEASURED_WRITE_TAG_BASE = 0x42565730u;

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr CreateFileW(string name, uint access, uint share,
            IntPtr securityAttributes, uint creationDisposition, uint flags,
            IntPtr templateFile);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool ReadFile(IntPtr file, IntPtr buffer, uint bytesToRead,
            out uint bytesRead, IntPtr overlapped);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool WriteFile(IntPtr file, IntPtr buffer, uint bytesToWrite,
            out uint bytesWritten, IntPtr overlapped);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool FlushFileBuffers(IntPtr file);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool SetFilePointerEx(IntPtr file, long distance,
            out long newPosition, uint moveMethod);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool CloseHandle(IntPtr handle);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern IntPtr VirtualAlloc(IntPtr address, UIntPtr size,
            uint allocationType, uint protect);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool VirtualFree(IntPtr address, UIntPtr size, uint freeType);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool GetDiskFreeSpaceW(string rootPath, out uint sectorsPerCluster,
            out uint bytesPerSector, out uint freeClusters, out uint totalClusters);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool GetFileInformationByHandleEx(IntPtr file,
            int infoClass, out FileAlignmentInfo info, uint size);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool QueryPerformanceCounter(out long value);

        [DllImport("kernel32.dll", SetLastError = true)]
        private static extern bool QueryPerformanceFrequency(out long value);

        private static readonly long CounterFrequency = GetCounterFrequency();

        private static long GetCounterFrequency() {
            long value;
            if (!QueryPerformanceFrequency(out value) || value <= 0) {
                throw LastError("QueryPerformanceFrequency");
            }
            return value;
        }

        private static long Counter() {
            long value;
            if (!QueryPerformanceCounter(out value)) {
                throw LastError("QueryPerformanceCounter");
            }
            return value;
        }

        private static long ToNanoseconds(long ticks) {
            return checked((long)Math.Round(ticks * (1000000000.0 / CounterFrequency)));
        }

        private static Exception LastError(string operation) {
            return new Win32Exception(Marshal.GetLastWin32Error(), operation);
        }

        private static bool BadHandle(IntPtr handle) {
            return handle == IntPtr.Zero || handle == new IntPtr(-1);
        }

        private static void Close(ref IntPtr handle) {
            if (!BadHandle(handle)) {
                if (!CloseHandle(handle)) {
                    handle = new IntPtr(-1);
                    throw LastError("CloseHandle");
                }
            }
            handle = new IntPtr(-1);
        }

        private static bool PowerOfTwo(uint value) {
            return value != 0 && (value & (value - 1)) == 0;
        }

        private static uint RequiredAlignment(string dataPath, IntPtr file,
            int transferBytes, long fileBytes, out uint bytesPerSector,
            out uint fileAlignmentBytes) {
            string root = Path.GetPathRoot(dataPath);
            uint sectorsPerCluster;
            uint freeClusters;
            uint totalClusters;
            if (String.IsNullOrEmpty(root) || !GetDiskFreeSpaceW(root,
                    out sectorsPerCluster, out bytesPerSector,
                    out freeClusters, out totalClusters)) {
                throw LastError("GetDiskFreeSpaceW");
            }
            FileAlignmentInfo info;
            if (!GetFileInformationByHandleEx(file, FILE_ALIGNMENT_INFO_CLASS,
                    out info, (uint)Marshal.SizeOf(typeof(FileAlignmentInfo)))) {
                throw LastError("GetFileInformationByHandleEx(FileAlignmentInfo)");
            }
            fileAlignmentBytes = checked(info.AlignmentRequirement + 1u);
            if (!PowerOfTwo(bytesPerSector) || !PowerOfTwo(fileAlignmentBytes)) {
                throw new InvalidDataException("non-power-of-two storage alignment");
            }
            uint required = Math.Max(bytesPerSector, fileAlignmentBytes);
            if ((transferBytes % required) != 0 || (fileBytes % required) != 0) {
                throw new InvalidDataException("workload geometry violates storage alignment");
            }
            return required;
        }

        private static IntPtr OpenWrite(string path, bool create) {
            uint disposition = create ? CREATE_ALWAYS : OPEN_EXISTING;
            IntPtr handle = CreateFileW(path, GENERIC_WRITE, 0u, IntPtr.Zero,
                disposition, FILE_ATTRIBUTE_NORMAL | FILE_FLAG_NO_BUFFERING |
                FILE_FLAG_WRITE_THROUGH, IntPtr.Zero);
            if (BadHandle(handle)) {
                throw LastError("CreateFileW(write,no-buffering,write-through)");
            }
            return handle;
        }

        private static IntPtr OpenRead(string path) {
            IntPtr handle = CreateFileW(path, GENERIC_READ, 0u, IntPtr.Zero,
                OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL | FILE_FLAG_NO_BUFFERING,
                IntPtr.Zero);
            if (BadHandle(handle)) {
                throw LastError("CreateFileW(read,no-buffering)");
            }
            return handle;
        }

        private static void SeekStart(IntPtr handle) {
            long position;
            if (!SetFilePointerEx(handle, 0L, out position, FILE_BEGIN) || position != 0L) {
                throw LastError("SetFilePointerEx");
            }
        }

        private static void WriteExact(IntPtr handle, IntPtr buffer, int length) {
            uint written;
            if (!WriteFile(handle, buffer, (uint)length, out written, IntPtr.Zero)) {
                throw LastError("WriteFile");
            }
            if (written != (uint)length) {
                throw new EndOfStreamException("short unbuffered write");
            }
        }

        private static void ReadExact(IntPtr handle, IntPtr buffer, int length) {
            uint read;
            if (!ReadFile(handle, buffer, (uint)length, out read, IntPtr.Zero)) {
                throw LastError("ReadFile");
            }
            if (read != (uint)length) {
                throw new EndOfStreamException("short unbuffered read");
            }
        }

        private static long TimedWrite(IntPtr handle, IntPtr buffer, int length) {
            long before = Counter();
            WriteExact(handle, buffer, length);
            return ToNanoseconds(Counter() - before);
        }

        private static long TimedRead(IntPtr handle, IntPtr buffer, int length) {
            long before = Counter();
            ReadExact(handle, buffer, length);
            return ToNanoseconds(Counter() - before);
        }

        private static long TimedFlush(IntPtr handle) {
            long before = Counter();
            if (!FlushFileBuffers(handle)) {
                throw LastError("FlushFileBuffers");
            }
            return ToNanoseconds(Counter() - before);
        }

        private static void Flush(IntPtr handle) {
            if (!FlushFileBuffers(handle)) {
                throw LastError("FlushFileBuffers");
            }
        }

        private static ulong Mix(ulong value) {
            value ^= value >> 12;
            value ^= value << 25;
            value ^= value >> 27;
            return value * 2685821657736338717UL;
        }

        private static void FillPattern(byte[] bytes, long blockIndex, uint tag) {
            ulong state = ((ulong)tag << 32) ^ (ulong)blockIndex ^
                0x9E3779B97F4A7C15UL;
            int offset = 0;
            while (offset < bytes.Length) {
                state = Mix(state + (ulong)offset + 0xD1B54A32D192ED03UL);
                int take = Math.Min(8, bytes.Length - offset);
                for (int index = 0; index < take; index++) {
                    bytes[offset + index] = (byte)(state >> (index * 8));
                }
                offset += take;
            }
        }

        private static void VerifyPattern(IntPtr buffer, byte[] observed,
            byte[] expected, long blockIndex, uint tag) {
            Marshal.Copy(buffer, observed, 0, observed.Length);
            FillPattern(expected, blockIndex, tag);
            for (int index = 0; index < observed.Length; index++) {
                if (observed[index] != expected[index]) {
                    throw new InvalidDataException("deterministic readback mismatch");
                }
            }
        }

        private static void HashBlock(SHA256 hash, byte[] bytes) {
            hash.TransformBlock(bytes, 0, bytes.Length, bytes, 0);
        }

        private static string FinishHash(SHA256 hash) {
            hash.TransformFinalBlock(new byte[0], 0, 0);
            byte[] digest = hash.Hash;
            char[] chars = new char[digest.Length * 2];
            const string alphabet = "0123456789abcdef";
            for (int index = 0; index < digest.Length; index++) {
                chars[index * 2] = alphabet[digest[index] >> 4];
                chars[index * 2 + 1] = alphabet[digest[index] & 15];
            }
            return new string(chars);
        }

        private static long Sum(List<long> samples) {
            long total = 0L;
            foreach (long sample in samples) {
                total = checked(total + sample);
            }
            return total;
        }

        private static long NearestRank(long[] sorted, double quantile) {
            int index = Math.Max(0,
                (int)Math.Ceiling(quantile * sorted.Length) - 1);
            return sorted[index];
        }

        private static LatencySummary Summarize(List<long> samples) {
            if (samples.Count == 0) {
                throw new InvalidDataException("empty latency sample set");
            }
            long[] sorted = samples.ToArray();
            Array.Sort(sorted);
            return new LatencySummary {
                Count = sorted.Length,
                P50Ns = NearestRank(sorted, 0.50),
                P95Ns = NearestRank(sorted, 0.95),
                P99Ns = NearestRank(sorted, 0.99),
                MaxNs = sorted[sorted.Length - 1]
            };
        }

        private static double MibPerSecond(long bytes, long elapsedNs) {
            if (bytes <= 0 || elapsedNs <= 0) {
                throw new InvalidDataException("non-positive throughput input");
            }
            return (bytes / (1024.0 * 1024.0)) /
                (elapsedNs / 1000000000.0);
        }

        public static WorkloadReport Run(string dataPath, int fileMib,
            int transferKib, int readPasses, int writePasses) {
            long fileBytes = checked((long)fileMib * 1024L * 1024L);
            int transferBytes = checked(transferKib * 1024);
            if (fileBytes <= 0 || transferBytes <= 0 ||
                    fileBytes % transferBytes != 0 || readPasses <= 0 ||
                    writePasses <= 0) {
                throw new ArgumentOutOfRangeException("workload geometry");
            }
            int blocks = checked((int)(fileBytes / transferBytes));
            List<long> readLatencies = new List<long>(checked(blocks * readPasses));
            List<long> writeLatencies = new List<long>(checked(blocks * writePasses));
            List<long> flushLatencies = new List<long>(writePasses);
            IntPtr handle = new IntPtr(-1);
            IntPtr aligned = IntPtr.Zero;
            byte[] expected = new byte[transferBytes];
            byte[] observed = new byte[transferBytes];
            uint bytesPerSector = 0u;
            uint fileAlignmentBytes = 0u;
            uint requiredAlignment = 0u;
            string warmupHash = null;
            string finalHash = null;
            long readPhaseNs = 0L;
            long writePhaseNs = 0L;
            try {
                handle = OpenWrite(dataPath, true);
                requiredAlignment = RequiredAlignment(dataPath, handle,
                    transferBytes, fileBytes, out bytesPerSector,
                    out fileAlignmentBytes);
                aligned = VirtualAlloc(IntPtr.Zero, (UIntPtr)(uint)transferBytes,
                    MEM_COMMIT_RESERVE, PAGE_READWRITE);
                if (aligned == IntPtr.Zero) {
                    throw LastError("VirtualAlloc");
                }
                if (((ulong)aligned.ToInt64() & ((ulong)requiredAlignment - 1UL)) != 0UL) {
                    throw new InvalidDataException("VirtualAlloc buffer is not storage-aligned");
                }

                // Precondition: full deterministic unbuffered write, one explicit
                // FlushFileBuffers, and close. It is intentionally unmeasured.
                for (int block = 0; block < blocks; block++) {
                    FillPattern(expected, block, PRECONDITION_TAG);
                    Marshal.Copy(expected, 0, aligned, transferBytes);
                    WriteExact(handle, aligned, transferBytes);
                }
                Flush(handle);
                Close(ref handle);

                // Warmup: one full unbuffered read with complete verification.
                handle = OpenRead(dataPath);
                using (SHA256 hash = SHA256.Create()) {
                    for (int block = 0; block < blocks; block++) {
                        ReadExact(handle, aligned, transferBytes);
                        VerifyPattern(aligned, observed, expected, block,
                            PRECONDITION_TAG);
                        HashBlock(hash, observed);
                    }
                    warmupHash = FinishHash(hash);
                }

                // Measured reads: QD1, five passes by default. Per-call QPC
                // excludes verification; phase time includes fixed verification.
                long readPhaseStart = Counter();
                for (int pass = 0; pass < readPasses; pass++) {
                    SeekStart(handle);
                    for (int block = 0; block < blocks; block++) {
                        readLatencies.Add(TimedRead(handle, aligned, transferBytes));
                        VerifyPattern(aligned, observed, expected, block,
                            PRECONDITION_TAG);
                    }
                }
                readPhaseNs = ToNanoseconds(Counter() - readPhaseStart);
                Close(ref handle);

                // Measured writes: every operation is NO_BUFFERING|WRITE_THROUGH;
                // every pass ends in an explicitly timed FlushFileBuffers and
                // an unmeasured full readback. The measured interval excludes
                // readback while still proving every pass, not only the last.
                for (int pass = 0; pass < writePasses; pass++) {
                    handle = OpenWrite(dataPath, false);
                    SeekStart(handle);
                    uint tag = checked(MEASURED_WRITE_TAG_BASE + (uint)pass);
                    long writePassStart = Counter();
                    for (int block = 0; block < blocks; block++) {
                        FillPattern(expected, block, tag);
                        Marshal.Copy(expected, 0, aligned, transferBytes);
                        writeLatencies.Add(TimedWrite(handle, aligned, transferBytes));
                    }
                    flushLatencies.Add(TimedFlush(handle));
                    writePhaseNs = checked(writePhaseNs +
                        ToNanoseconds(Counter() - writePassStart));
                    Close(ref handle);
                    handle = OpenRead(dataPath);
                    SHA256 hash = pass == writePasses - 1 ? SHA256.Create() : null;
                    for (int block = 0; block < blocks; block++) {
                        ReadExact(handle, aligned, transferBytes);
                        VerifyPattern(aligned, observed, expected, block, tag);
                        if (hash != null) {
                            HashBlock(hash, observed);
                        }
                    }
                    if (hash != null) {
                        finalHash = FinishHash(hash);
                        hash.Dispose();
                    }
                    Close(ref handle);
                }

                long measuredReadBytes = checked(fileBytes * readPasses);
                long measuredWriteBytes = checked(fileBytes * writePasses);
                long readServiceNs = Sum(readLatencies);
                long writeAndFlushServiceNs = checked(Sum(writeLatencies) +
                    Sum(flushLatencies));
                return new WorkloadReport {
                    FileBytes = fileBytes,
                    TransferBytes = transferBytes,
                    BlocksPerPass = blocks,
                    PreconditionWriteOps = blocks,
                    WarmupReadOps = blocks,
                    MeasuredReadOps = checked(blocks * readPasses),
                    MeasuredWriteOps = checked(blocks * writePasses),
                    WriteVerifyReadOps = checked(blocks * writePasses),
                    FinalVerifyReadOps = blocks,
                    VerifiedReadOps = checked(blocks * (readPasses + writePasses + 1)),
                    FlushCalls = checked(writePasses + 1),
                    BytesPerSector = bytesPerSector,
                    FileAlignmentBytes = fileAlignmentBytes,
                    RequiredAlignmentBytes = requiredAlignment,
                    ReadPhaseElapsedNs = readPhaseNs,
                    WritePhaseElapsedNs = writePhaseNs,
                    ReadServiceElapsedNs = readServiceNs,
                    WriteAndFlushServiceElapsedNs = writeAndFlushServiceNs,
                    ReadPhaseMibPerSec = MibPerSecond(measuredReadBytes, readPhaseNs),
                    ReadServiceMibPerSec = MibPerSecond(measuredReadBytes, readServiceNs),
                    WriteDurableMibPerSec = MibPerSecond(measuredWriteBytes, writePhaseNs),
                    WriteAndFlushServiceMibPerSec = MibPerSecond(
                        measuredWriteBytes, writeAndFlushServiceNs),
                    WarmupSha256 = warmupHash,
                    FinalSha256 = finalHash,
                    ReadLatencyNs = Summarize(readLatencies),
                    WriteLatencyNs = Summarize(writeLatencies),
                    FlushLatencyNs = Summarize(flushLatencies),
                    ReadLatencyRawNs = readLatencies.ToArray(),
                    WriteLatencyRawNs = writeLatencies.ToArray(),
                    FlushLatencyRawNs = flushLatencies.ToArray()
                };
            } finally {
                if (!BadHandle(handle)) {
                    CloseHandle(handle);
                }
                if (aligned != IntPtr.Zero) {
                    VirtualFree(aligned, UIntPtr.Zero, MEM_RELEASE);
                }
            }
        }
    }
}
'@

Add-Type -TypeDefinition $Source -Language CSharp -ErrorAction Stop
if ($CompileOnly) {
    Write-Output 'BRIDGEVM_NVME_WORKLOAD_COMPILE_OK'
    exit 0
}

New-Item -ItemType Directory -Force -Path $Root | Out-Null
New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($DataPath)) | Out-Null
foreach ($stale in @($RawPath, $ResultPath, $DonePath,
        ($RawPath + '.' + $Nonce + '.tmp'),
        ($ResultPath + '.' + $Nonce + '.tmp'),
        ($DonePath + '.' + $Nonce + '.tmp'))) {
    if ([IO.File]::Exists($stale)) { [IO.File]::Delete($stale) }
}

function Get-Sha256Hex([string]$Text) {
    $bytes = $Utf8NoBom.GetBytes($Text)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
    } finally {
        $sha.Dispose()
    }
}

function Write-AtomicUtf8([string]$Path, [string]$Text) {
    $bytes = $Utf8NoBom.GetBytes($Text)
    if ($bytes.Length -ge $MaxJsonBytes) {
        throw 'JSON output exceeds the private share limit'
    }
    $temp = $Path + '.' + $Nonce + '.tmp'
    if ([IO.File]::Exists($temp)) { [IO.File]::Delete($temp) }
    $stream = New-Object IO.FileStream($temp, [IO.FileMode]::CreateNew,
        [IO.FileAccess]::Write, [IO.FileShare]::None, 4096,
        [IO.FileOptions]::WriteThrough)
    try {
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
    }
    if ([IO.File]::Exists($Path)) { [IO.File]::Delete($Path) }
    [IO.File]::Move($temp, $Path)
}

$Status = 'passed'
$FailureType = $null
$FailureHResult = $null
$Run = $null
try {
    $Run = [BridgeVM.NvmePerf.WarmSequentialWorkload]::Run(
        $DataPath, __FILE_MIB__, __TRANSFER_KIB__, __READ_PASSES__, __WRITE_PASSES__)
} catch {
    $Status = 'failed'
    $FailureType = $_.Exception.GetType().FullName
    $FailureHResult = $_.Exception.HResult
}

if ($null -ne $Run) {
    $Raw = [ordered]@{
        schema = $Schema
        workload_profile = $WorkloadProfile
        nonce = $Nonce
        config_sha256 = $ConfigSha256
        read_latency_ns = $Run.ReadLatencyRawNs
        write_latency_ns = $Run.WriteLatencyRawNs
        flush_latency_ns = $Run.FlushLatencyRawNs
    }
} else {
    $Raw = [ordered]@{
        schema = $Schema
        workload_profile = $WorkloadProfile
        nonce = $Nonce
        config_sha256 = $ConfigSha256
        read_latency_ns = @()
        write_latency_ns = @()
        flush_latency_ns = @()
    }
}
$RawJson = $Raw | ConvertTo-Json -Depth 4 -Compress
$RawSha256 = Get-Sha256Hex $RawJson

if ($null -ne $Run) {
    $Aggregate = [ordered]@{
        schema = $Schema
        workload_profile = $WorkloadProfile
        nonce = $Nonce
        config_sha256 = $ConfigSha256
        config = ($ConfigJson | ConvertFrom-Json)
        status = $Status
        raw_sha256 = $RawSha256
        file_bytes = $Run.FileBytes
        transfer_bytes = $Run.TransferBytes
        blocks_per_pass = $Run.BlocksPerPass
        precondition_write_ops = $Run.PreconditionWriteOps
        precondition_write_bytes = $Run.FileBytes
        warmup_read_ops = $Run.WarmupReadOps
        warmup_read_bytes = $Run.FileBytes
        measured_read_ops = $Run.MeasuredReadOps
        measured_read_bytes = ($Run.FileBytes * __READ_PASSES__)
        measured_write_ops = $Run.MeasuredWriteOps
        measured_write_bytes = ($Run.FileBytes * __WRITE_PASSES__)
        write_verify_read_ops = $Run.WriteVerifyReadOps
        read_result_count = __READ_PASSES__
        write_result_count = __WRITE_PASSES__
        final_verify_read_ops = $Run.FinalVerifyReadOps
        final_verify_read_bytes = $Run.FileBytes
        verified_read_ops = $Run.VerifiedReadOps
        flush_calls = $Run.FlushCalls
        bytes_per_sector = $Run.BytesPerSector
        file_alignment_bytes = $Run.FileAlignmentBytes
        required_alignment_bytes = $Run.RequiredAlignmentBytes
        warmup_sha256 = $Run.WarmupSha256
        final_sha256 = $Run.FinalSha256
        read_phase_elapsed_ns = $Run.ReadPhaseElapsedNs
        read_service_elapsed_ns = $Run.ReadServiceElapsedNs
        read_phase_mib_per_sec = $Run.ReadPhaseMibPerSec
        read_service_mib_per_sec = $Run.ReadServiceMibPerSec
        read_throughput_mib_s = $Run.ReadPhaseMibPerSec
        read_p50_ms = ($Run.ReadLatencyNs.P50Ns / 1000000.0)
        read_p95_ms = ($Run.ReadLatencyNs.P95Ns / 1000000.0)
        read_p99_ms = ($Run.ReadLatencyNs.P99Ns / 1000000.0)
        read_max_ms = ($Run.ReadLatencyNs.MaxNs / 1000000.0)
        write_phase_elapsed_ns = $Run.WritePhaseElapsedNs
        write_and_flush_service_elapsed_ns = $Run.WriteAndFlushServiceElapsedNs
        write_durable_mib_per_sec = $Run.WriteDurableMibPerSec
        write_and_flush_service_mib_per_sec = $Run.WriteAndFlushServiceMibPerSec
        write_durable_throughput_mib_s = $Run.WriteDurableMibPerSec
        write_p50_ms = ($Run.WriteLatencyNs.P50Ns / 1000000.0)
        write_p95_ms = ($Run.WriteLatencyNs.P95Ns / 1000000.0)
        write_p99_ms = ($Run.WriteLatencyNs.P99Ns / 1000000.0)
        write_max_ms = ($Run.WriteLatencyNs.MaxNs / 1000000.0)
        flush_p50_ms = ($Run.FlushLatencyNs.P50Ns / 1000000.0)
        flush_p95_ms = ($Run.FlushLatencyNs.P95Ns / 1000000.0)
        flush_p99_ms = ($Run.FlushLatencyNs.P99Ns / 1000000.0)
        flush_max_ms = ($Run.FlushLatencyNs.MaxNs / 1000000.0)
        read_latency_ns = [ordered]@{
            count = $Run.ReadLatencyNs.Count
            p50 = $Run.ReadLatencyNs.P50Ns
            p95 = $Run.ReadLatencyNs.P95Ns
            p99 = $Run.ReadLatencyNs.P99Ns
            max = $Run.ReadLatencyNs.MaxNs
        }
        write_latency_ns = [ordered]@{
            count = $Run.WriteLatencyNs.Count
            p50 = $Run.WriteLatencyNs.P50Ns
            p95 = $Run.WriteLatencyNs.P95Ns
            p99 = $Run.WriteLatencyNs.P99Ns
            max = $Run.WriteLatencyNs.MaxNs
        }
        flush_latency_ns = [ordered]@{
            count = $Run.FlushLatencyNs.Count
            p50 = $Run.FlushLatencyNs.P50Ns
            p95 = $Run.FlushLatencyNs.P95Ns
            p99 = $Run.FlushLatencyNs.P99Ns
            max = $Run.FlushLatencyNs.MaxNs
        }
    }
} else {
    $Aggregate = [ordered]@{
        schema = $Schema
        workload_profile = $WorkloadProfile
        nonce = $Nonce
        config_sha256 = $ConfigSha256
        config = ($ConfigJson | ConvertFrom-Json)
        status = $Status
        raw_sha256 = $RawSha256
        failure_type = $FailureType
        failure_hresult = $FailureHResult
        measured_read_ops = 0
        measured_write_ops = 0
        read_result_count = 0
        write_result_count = 0
        flush_calls = 0
    }
}

$ResultJson = $Aggregate | ConvertTo-Json -Depth 8 -Compress
$ResultSha256 = Get-Sha256Hex $ResultJson
Write-AtomicUtf8 $RawPath $RawJson
Write-AtomicUtf8 $ResultPath $ResultJson
$Done = [ordered]@{
    schema = $Schema
    workload_profile = $WorkloadProfile
    nonce = $Nonce
    config_sha256 = $ConfigSha256
    status = $Status
    raw_sha256 = $RawSha256
    result_sha256 = $ResultSha256
}
Write-AtomicUtf8 $DonePath ($Done | ConvertTo-Json -Depth 3 -Compress)
if ($Status -ne 'passed') { exit 1 }
exit 0
'''
    replacements = {
        "__SCHEMA__": ps_quote(SCHEMA),
        "__WORKLOAD_PROFILE__": ps_quote(WORKLOAD_PROFILE),
        "__NONCE__": ps_quote(nonce),
        "__CONFIG_SHA__": ps_quote(config.config_sha256),
        "__CONFIG_JSON__": ps_quote(config_json),
        "__FILE_MIB__": str(config.file_mib),
        "__TRANSFER_KIB__": str(config.transfer_kib),
        "__READ_PASSES__": str(config.read_passes),
        "__WRITE_PASSES__": str(config.write_passes),
    }
    rendered = template
    for marker, value in replacements.items():
        rendered = rendered.replace(marker, value)
    if "__" in rendered:
        raise AssertionError("unexpanded workload template marker")
    # Source text is deliberately LF for review; the only serialized guest
    # artifact is normalized to Windows CRLF here.
    rendered = rendered.replace("\r\n", "\n").replace("\r", "\n")
    return rendered.replace("\n", "\r\n").encode("utf-8")


def validate_output_path(path: Path) -> None:
    if path.exists() or path.is_symlink():
        raise ValueError("output path must not already exist")
    repository = Path(__file__).resolve().parents[1]
    resolved = path.resolve(strict=False)
    try:
        resolved.relative_to(repository)
    except ValueError:
        pass
    else:
        raise ValueError("generated guest workload must stay outside the repository")


def write_output(path: Path, payload: bytes) -> None:
    validate_output_path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags, 0o600)
    with os.fdopen(descriptor, "wb") as output:
        output.write(payload)


def verify_output(script_path: Path, config_path: Path, args: argparse.Namespace) -> None:
    for path in (script_path, config_path):
        if not path.is_file() or path.is_symlink():
            raise ValueError("verified inputs must be regular non-symbolic-link files")
    script = script_path.read_bytes()
    matches = re.findall(rb"^\$Nonce = '([0-9a-f]{16,64})'\r$", script, re.MULTILINE)
    if len(matches) != 1:
        raise ValueError("workload script must contain exactly one canonical nonce")
    config = WorkloadConfig(
        nonce=matches[0].decode("ascii"), file_mib=args.file_mib,
        transfer_kib=args.transfer_kib, read_passes=args.read_passes,
        write_passes=args.write_passes,
    )
    if script != render(config):
        raise ValueError("workload script is not the exact renderer output")
    if config_path.read_bytes() != config.canonical_config_bytes:
        raise ValueError("workload config is not the exact canonical renderer output")


def self_test() -> None:
    config = WorkloadConfig(nonce="0123456789abcdef")
    assert (
        config.config_sha256
        == "70da5472a5a7e89830ff240cfcb55dd3fff2b84e311016ac096d958879ec4c79"
    )
    payload = render(config)
    text = payload.decode("utf-8")
    assert payload.startswith(b"param([switch]$CompileOnly)\r\n")
    assert payload.endswith(b"\r\n")
    assert b"\n" in payload and payload.count(b"\n") == payload.count(b"\r\n")
    required = (
        "CreateFileW",
        "ReadFile",
        "WriteFile",
        "FlushFileBuffers",
        "VirtualAlloc",
        "QueryPerformanceCounter",
        "GetDiskFreeSpaceW",
        "GetFileInformationByHandleEx",
        "FILE_FLAG_NO_BUFFERING",
        "FILE_FLAG_WRITE_THROUGH",
        "Precondition",
        "Warmup",
        "Measured reads",
        "Measured writes",
        "unmeasured full readback",
        "Math.Ceiling",
        "nvme-result.json",
        "nvme-raw.json",
        "nvme-result.done",
        "Write-AtomicUtf8",
        "BRIDGEVM_NVME_WORKLOAD_COMPILE_OK",
        config.config_sha256,
        WORKLOAD_PROFILE,
        "read_result_count",
        "write_result_count",
        "write_verify_read_ops",
        "read_throughput_mib_s",
        "read_p99_ms",
        "write_durable_throughput_mib_s",
        "write_p99_ms",
        "flush_max_ms",
    )
    for marker in required:
        assert marker in text, marker
    add_type = text.index("Add-Type -TypeDefinition")
    compile_marker = text.index("BRIDGEVM_NVME_WORKLOAD_COMPILE_OK")
    first_guest_write = text.index("New-Item -ItemType Directory")
    assert add_type < compile_marker < first_guest_write
    assert "512, 128, 5, 2" in text
    assert "read_latency_ns" in text and "p99" in text and "max" in text
    assert "(8 * 1024 * 1024) - 1" in text
    forbidden = ("password=", "credential=", "private key", "bearer ")
    lowered = text.lower()
    for marker in forbidden:
        assert marker not in lowered, marker
    canonical = config.canonical_config_bytes
    assert canonical == json.dumps(
        config.canonical_config(), sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    assert not canonical.endswith(b"\n")
    assert hashlib.sha256(canonical).hexdigest() == config.config_sha256
    with tempfile.TemporaryDirectory(prefix="bridgevm-nvme-render-") as temporary:
        temporary_path = Path(temporary)
        ps_path = temporary_path / "nvme-workload.ps1"
        config_path = temporary_path / "nvme-workload-config.json"
        write_output(ps_path, payload)
        write_output(config_path, canonical)
        assert ps_path.read_bytes() == payload
        assert config_path.read_bytes() == canonical
        verify_output(ps_path, config_path, argparse.Namespace(
            file_mib=512, transfer_kib=128, read_passes=5, write_passes=2
        ))
        victim = temporary_path / "victim"
        linked = temporary_path / "linked"
        victim.write_bytes(b"do-not-truncate")
        os.link(victim, linked)
        try:
            write_output(linked, b"replacement")
        except ValueError:
            pass
        else:
            raise AssertionError("an existing hardlink output was accepted")
        assert victim.read_bytes() == b"do-not-truncate"
    try:
        validate_output_path(Path(__file__).resolve().parent / "forbidden.ps1")
    except ValueError:
        pass
    else:
        raise AssertionError("repository output path was accepted")

    invalid = (
        WorkloadConfig(nonce="not-hex"),
        WorkloadConfig(nonce="0123456789abcdef", file_mib=63),
        WorkloadConfig(nonce="0123456789abcdef", transfer_kib=96),
        WorkloadConfig(nonce="0123456789abcdef", read_passes=0),
        WorkloadConfig(nonce="0123456789abcdef", write_passes=33),
        WorkloadConfig(
            nonce="0123456789abcdef",
            file_mib=8192,
            transfer_kib=4,
            read_passes=32,
            write_passes=32,
        ),
    )
    for candidate in invalid:
        try:
            validate_config(candidate)
        except ValueError:
            pass
        else:
            raise AssertionError("invalid workload configuration was accepted")
    print("render HVF NVMe workload self-test: PASS")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--output", type=Path)
    result.add_argument("--config-output", type=Path)
    result.add_argument("--verify-output", type=Path)
    result.add_argument("--verify-config", type=Path)
    result.add_argument("--nonce")
    result.add_argument("--file-mib", type=int, default=512)
    result.add_argument("--transfer-kib", type=int, default=128)
    result.add_argument("--read-passes", type=int, default=5)
    result.add_argument("--write-passes", type=int, default=2)
    result.add_argument("--self-test", action="store_true")
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    if args.self_test:
        if (
            args.output is not None
            or args.config_output is not None
            or args.verify_output is not None
            or args.verify_config is not None
            or args.nonce is not None
        ):
            raise SystemExit(
                "--self-test cannot be combined with output paths or --nonce"
            )
        self_test()
        return 0
    if args.verify_output is not None or args.verify_config is not None:
        if (args.verify_output is None or args.verify_config is None or
                args.output is not None or args.config_output is not None or
                args.nonce is not None):
            raise SystemExit("verification requires only --verify-output and --verify-config")
        try:
            verify_output(args.verify_output, args.verify_config, args)
        except (OSError, ValueError) as error:
            print(f"render HVF NVMe workload: {error}", file=sys.stderr)
            return 2
        print("render HVF NVMe workload verification: PASS")
        return 0
    if args.output is None or args.nonce is None:
        raise SystemExit("--output and --nonce are required unless --self-test is used")
    config = WorkloadConfig(
        nonce=args.nonce,
        file_mib=args.file_mib,
        transfer_kib=args.transfer_kib,
        read_passes=args.read_passes,
        write_passes=args.write_passes,
    )
    try:
        payload = render(config)
        validate_output_path(args.output)
        if args.config_output is not None:
            validate_output_path(args.config_output)
            if args.config_output.resolve(strict=False) == args.output.resolve(strict=False):
                raise ValueError("--config-output must differ from --output")
        write_output(args.output, payload)
        if args.config_output is not None:
            write_output(args.config_output, config.canonical_config_bytes)
    except (OSError, ValueError) as error:
        print(f"render HVF NVMe workload: {error}", file=sys.stderr)
        return 2
    print(
        f"rendered {args.output} bytes={len(payload)} "
        f"config_sha256={config.config_sha256}"
        + (
            f" config_output={args.config_output}"
            if args.config_output is not None
            else ""
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
