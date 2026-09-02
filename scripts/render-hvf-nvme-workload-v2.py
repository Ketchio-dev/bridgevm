#!/usr/bin/env python3
"""Render the fixed private Windows warm-sequential NVMe v2 workload.

The generated PowerShell is a per-live-job private artifact.  The renderer and
its canonical configuration are deterministic; benchmark data and guest result
artifacts remain outside git.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
from pathlib import Path


SCHEMA = "bridgevm.windows-nvme-warm-seq.v2"
WORKLOAD_PROFILE = "windows-nvme-warm-seq-v2"
PATTERN_ID = "offset-xorshift64-v1"
PRECONDITION_SHA256 = (
    "6b60d964c6165fd46dc68630f01fc7118edb6f54915799a9cb0d59f7a961037b"
)
FINAL_SHA256 = (
    "9c48543aea55b3571fd028c58d9bf5159d82a5d0d7b90acd1adc06a469cce0c8"
)
DATA_PATH = r"C:\ProgramData\BridgeVMPerf\nvme-seq-v2.bin"
FILE_MIB = 2048
TRANSFER_KIB = 128
READ_PASSES = 16
WRITE_PASSES = 4
QUEUE_DEPTH = 1
POST_WARMUP_SETTLE_MS = 30_000
EXPECTED_CONFIG_SHA256 = (
    "243fc66006b67ff702d74609b56f2c7f044331cef31c35758e294b96865b7908"
)
NONCE_RE = re.compile(r"^[0-9A-Fa-f]{32}$")
MAX_PRIVATE_JSON_BYTES = 8 * 1024 * 1024
RAW_SAMPLE_JSON_BUDGET = 22
RAW_JSON_FIXED_BUDGET = 16 * 1024

FILE_BYTES = FILE_MIB * 1024 * 1024
TRANSFER_BYTES = TRANSFER_KIB * 1024
BLOCKS_PER_PASS = FILE_BYTES // TRANSFER_BYTES
READ_OPS = BLOCKS_PER_PASS * READ_PASSES
WRITE_OPS = BLOCKS_PER_PASS * WRITE_PASSES
RAW_SAMPLE_COUNT = READ_OPS + WRITE_OPS + WRITE_PASSES + READ_PASSES
MAX_ESTIMATED_RAW_JSON_BYTES = (
    RAW_SAMPLE_COUNT * RAW_SAMPLE_JSON_BUDGET + RAW_JSON_FIXED_BUDGET
)


def canonical_config() -> dict[str, object]:
    return {
        "schema": SCHEMA,
        "workload_profile": WORKLOAD_PROFILE,
        "pattern_id": PATTERN_ID,
        "precondition_sha256": PRECONDITION_SHA256,
        "final_sha256": FINAL_SHA256,
        "data_path": DATA_PATH,
        "file_mib": FILE_MIB,
        "transfer_kib": TRANSFER_KIB,
        "read_passes": READ_PASSES,
        "write_passes": WRITE_PASSES,
        "queue_depth": QUEUE_DEPTH,
        "post_warmup_settle_ms": POST_WARMUP_SETTLE_MS,
        "read_flags": ["FILE_FLAG_NO_BUFFERING"],
        "write_flags": [
            "FILE_FLAG_NO_BUFFERING",
            "FILE_FLAG_WRITE_THROUGH",
        ],
        "file_attributes": ["FILE_ATTRIBUTE_NOT_CONTENT_INDEXED"],
        "read_phase_semantics": (
            "sum of per-pass SeekStart plus ReadFile intervals; pattern and hash "
            "verification excluded"
        ),
        "flush_semantics": (
            "FlushFileBuffers after precondition and each measured write pass"
        ),
        "verification_semantics": (
            "full warmup and post-read verification plus full readback after "
            "every measured write pass"
        ),
        "cache_profile": "guest-unbuffered-host-warm",
    }


def canonical_config_bytes() -> bytes:
    return json.dumps(
        canonical_config(), sort_keys=True, separators=(",", ":")
    ).encode("utf-8")


def config_sha256() -> str:
    return hashlib.sha256(canonical_config_bytes()).hexdigest()


def validate_contract() -> None:
    if (
        FILE_MIB != 2048
        or TRANSFER_KIB != 128
        or READ_PASSES != 16
        or WRITE_PASSES != 4
        or QUEUE_DEPTH != 1
        or POST_WARMUP_SETTLE_MS != 30_000
        or DATA_PATH != r"C:\ProgramData\BridgeVMPerf\nvme-seq-v2.bin"
    ):
        raise ValueError("the v2 workload geometry is not the fixed contract")
    if FILE_BYTES % TRANSFER_BYTES or TRANSFER_KIB & (TRANSFER_KIB - 1):
        raise ValueError("the v2 workload geometry is not storage aligned")
    if BLOCKS_PER_PASS != 16_384 or READ_OPS != 262_144 or WRITE_OPS != 65_536:
        raise ValueError("the v2 operation counts are not canonical")
    if MAX_ESTIMATED_RAW_JSON_BYTES >= MAX_PRIVATE_JSON_BYTES:
        raise ValueError("the v2 raw timing vectors may exceed the private limit")
    if config_sha256() != EXPECTED_CONFIG_SHA256:
        raise ValueError("the canonical v2 configuration identity changed")


def ps_quote(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def render(nonce_value: str) -> bytes:
    validate_contract()
    if not NONCE_RE.fullmatch(nonce_value):
        raise ValueError("nonce must contain exactly 32 hexadecimal characters")
    nonce = nonce_value.lower()
    config_json = canonical_config_bytes().decode("utf-8")
    template = r'''param([switch]$CompileOnly)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Schema = __SCHEMA__
$WorkloadProfile = __WORKLOAD_PROFILE__
$Nonce = __NONCE__
$ConfigSha256 = __CONFIG_SHA__
$ConfigJson = __CONFIG_JSON__
$Root = 'C:\BridgeVMNVMe'
$DataPath = 'C:\ProgramData\BridgeVMPerf\nvme-seq-v2.bin'
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
using System.Threading;

namespace BridgeVM.NvmePerfV2 {
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
        public int PostReadVerifyOps { get; set; }
        public int MeasuredWriteOps { get; set; }
        public int WriteVerifyReadOps { get; set; }
        public int FinalVerifyReadOps { get; set; }
        public int VerifiedReadOps { get; set; }
        public int FlushCalls { get; set; }
        public int PostWarmupSettleMs { get; set; }
        public uint BytesPerSector { get; set; }
        public uint FileAlignmentBytes { get; set; }
        public uint RequiredAlignmentBytes { get; set; }
        public long ReadPhaseElapsedNs { get; set; }
        public long ReadServiceElapsedNs { get; set; }
        public long WritePhaseElapsedNs { get; set; }
        public long WriteAndFlushServiceElapsedNs { get; set; }
        public double ReadPhaseMibPerSec { get; set; }
        public double ReadServiceMibPerSec { get; set; }
        public double WriteDurableMibPerSec { get; set; }
        public double WriteAndFlushServiceMibPerSec { get; set; }
        public string WarmupSha256 { get; set; }
        public string PostReadSha256 { get; set; }
        public string FinalSha256 { get; set; }
        public LatencySummary ReadLatencyNs { get; set; }
        public LatencySummary ReadPassElapsedNs { get; set; }
        public LatencySummary WriteLatencyNs { get; set; }
        public LatencySummary FlushLatencyNs { get; set; }
        public long[] ReadLatencyRawNs { get; set; }
        public long[] ReadPassElapsedRawNs { get; set; }
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
        private const uint FILE_ATTRIBUTE_NOT_CONTENT_INDEXED = 0x00002000u;
        private const uint INVALID_FILE_ATTRIBUTES = 0xFFFFFFFFu;
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

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern uint GetFileAttributesW(string name);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool SetFileAttributesW(string name, uint attributes);

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
        private static extern bool VirtualFree(IntPtr address, UIntPtr size,
            uint freeType);

        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern bool GetDiskFreeSpaceW(string rootPath,
            out uint sectorsPerCluster, out uint bytesPerSector,
            out uint freeClusters, out uint totalClusters);

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
            return checked((long)Math.Round(
                ticks * (1000000000.0 / CounterFrequency)));
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
            if (!GetFileInformationByHandleEx(file,
                    FILE_ALIGNMENT_INFO_CLASS, out info,
                    (uint)Marshal.SizeOf(typeof(FileAlignmentInfo)))) {
                throw LastError(
                    "GetFileInformationByHandleEx(FileAlignmentInfo)");
            }
            fileAlignmentBytes = checked(info.AlignmentRequirement + 1u);
            if (!PowerOfTwo(bytesPerSector) ||
                    !PowerOfTwo(fileAlignmentBytes)) {
                throw new InvalidDataException(
                    "non-power-of-two storage alignment");
            }
            uint required = Math.Max(bytesPerSector, fileAlignmentBytes);
            if ((transferBytes % required) != 0 ||
                    (fileBytes % required) != 0) {
                throw new InvalidDataException(
                    "workload geometry violates storage alignment");
            }
            return required;
        }

        private static uint CommonAttributes() {
            return FILE_ATTRIBUTE_NOT_CONTENT_INDEXED;
        }

        private static void EnsureNotContentIndexed(string path) {
            uint attributes = GetFileAttributesW(path);
            if (attributes == INVALID_FILE_ATTRIBUTES) {
                throw LastError("GetFileAttributesW");
            }
            if ((attributes & FILE_ATTRIBUTE_NOT_CONTENT_INDEXED) == 0u) {
                if (!SetFileAttributesW(path,
                        attributes | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED)) {
                    throw LastError("SetFileAttributesW(not-content-indexed)");
                }
                attributes = GetFileAttributesW(path);
            }
            if (attributes == INVALID_FILE_ATTRIBUTES ||
                    (attributes & FILE_ATTRIBUTE_NOT_CONTENT_INDEXED) == 0u) {
                throw new InvalidDataException(
                    "benchmark data file is content-indexable");
            }
        }

        private static IntPtr OpenWrite(string path, bool create) {
            uint disposition = create ? CREATE_ALWAYS : OPEN_EXISTING;
            IntPtr handle = CreateFileW(path, GENERIC_WRITE, 0u,
                IntPtr.Zero, disposition,
                CommonAttributes() | FILE_FLAG_NO_BUFFERING |
                    FILE_FLAG_WRITE_THROUGH,
                IntPtr.Zero);
            if (BadHandle(handle)) {
                throw LastError(
                    "CreateFileW(write,no-buffering,write-through,not-indexed)");
            }
            return handle;
        }

        private static IntPtr OpenRead(string path) {
            IntPtr handle = CreateFileW(path, GENERIC_READ, 0u,
                IntPtr.Zero, OPEN_EXISTING,
                CommonAttributes() | FILE_FLAG_NO_BUFFERING,
                IntPtr.Zero);
            if (BadHandle(handle)) {
                throw LastError(
                    "CreateFileW(read,no-buffering,not-indexed)");
            }
            return handle;
        }

        private static void SeekStart(IntPtr handle) {
            long position;
            if (!SetFilePointerEx(handle, 0L, out position, FILE_BEGIN) ||
                    position != 0L) {
                throw LastError("SetFilePointerEx");
            }
        }

        private static void WriteExact(IntPtr handle, IntPtr buffer, int length) {
            uint written;
            if (!WriteFile(handle, buffer, (uint)length,
                    out written, IntPtr.Zero)) {
                throw LastError("WriteFile");
            }
            if (written != (uint)length) {
                throw new EndOfStreamException("short unbuffered write");
            }
        }

        private static void ReadExact(IntPtr handle, IntPtr buffer, int length) {
            uint read;
            if (!ReadFile(handle, buffer, (uint)length,
                    out read, IntPtr.Zero)) {
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

        private static void FillPattern(byte[] bytes, long blockIndex,
            uint tag) {
            ulong state = ((ulong)tag << 32) ^ (ulong)blockIndex ^
                0x9E3779B97F4A7C15UL;
            int offset = 0;
            while (offset < bytes.Length) {
                state = Mix(state + (ulong)offset +
                    0xD1B54A32D192ED03UL);
                int take = Math.Min(8, bytes.Length - offset);
                for (int index = 0; index < take; index++) {
                    bytes[offset + index] =
                        (byte)(state >> (index * 8));
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
                    throw new InvalidDataException(
                        "deterministic readback mismatch");
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

        private static string VerifyAndHash(IntPtr handle, IntPtr aligned,
            byte[] observed, byte[] expected, int blocks,
            int transferBytes, uint tag) {
            SeekStart(handle);
            using (SHA256 hash = SHA256.Create()) {
                for (int block = 0; block < blocks; block++) {
                    ReadExact(handle, aligned, transferBytes);
                    VerifyPattern(aligned, observed, expected, block, tag);
                    HashBlock(hash, observed);
                }
                return FinishHash(hash);
            }
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
                throw new InvalidDataException(
                    "non-positive throughput input");
            }
            return (bytes / (1024.0 * 1024.0)) /
                (elapsedNs / 1000000000.0);
        }

        public static WorkloadReport Run(string dataPath, int fileMib,
            int transferKib, int readPasses, int writePasses,
            int postWarmupSettleMs) {
            if (dataPath != @"C:\ProgramData\BridgeVMPerf\nvme-seq-v2.bin" ||
                    fileMib != 2048 || transferKib != 128 ||
                    readPasses != 16 || writePasses != 4 ||
                    postWarmupSettleMs != 30000) {
                throw new InvalidDataException(
                    "runtime workload contract is not canonical v2");
            }
            long fileBytes = checked((long)fileMib * 1024L * 1024L);
            int transferBytes = checked(transferKib * 1024);
            if (fileBytes % transferBytes != 0) {
                throw new InvalidDataException(
                    "runtime workload geometry is not exact");
            }
            int blocks = checked((int)(fileBytes / transferBytes));
            if (blocks != 16384) {
                throw new InvalidDataException(
                    "runtime workload block count is not canonical");
            }
            List<long> readLatencies =
                new List<long>(checked(blocks * readPasses));
            List<long> readPassElapsed = new List<long>(readPasses);
            List<long> writeLatencies =
                new List<long>(checked(blocks * writePasses));
            List<long> flushLatencies = new List<long>(writePasses);
            IntPtr handle = new IntPtr(-1);
            IntPtr aligned = IntPtr.Zero;
            byte[] expected = new byte[transferBytes];
            byte[] observed = new byte[transferBytes];
            uint bytesPerSector = 0u;
            uint fileAlignmentBytes = 0u;
            uint requiredAlignment = 0u;
            string warmupHash = null;
            string postReadHash = null;
            string finalHash = null;
            long writePhaseNs = 0L;
            try {
                handle = OpenWrite(dataPath, true);
                requiredAlignment = RequiredAlignment(dataPath, handle,
                    transferBytes, fileBytes, out bytesPerSector,
                    out fileAlignmentBytes);
                aligned = VirtualAlloc(IntPtr.Zero,
                    (UIntPtr)(uint)transferBytes,
                    MEM_COMMIT_RESERVE, PAGE_READWRITE);
                if (aligned == IntPtr.Zero) {
                    throw LastError("VirtualAlloc");
                }
                if (((ulong)aligned.ToInt64() &
                        ((ulong)requiredAlignment - 1UL)) != 0UL) {
                    throw new InvalidDataException(
                        "VirtualAlloc buffer is not storage-aligned");
                }

                // Full deterministic precondition write; deliberately untimed.
                for (int block = 0; block < blocks; block++) {
                    FillPattern(expected, block, PRECONDITION_TAG);
                    Marshal.Copy(expected, 0, aligned, transferBytes);
                    WriteExact(handle, aligned, transferBytes);
                }
                Flush(handle);
                Close(ref handle);
                EnsureNotContentIndexed(dataPath);

                // Full verified warmup followed by the fixed 30-second settle.
                handle = OpenRead(dataPath);
                warmupHash = VerifyAndHash(handle, aligned, observed,
                    expected, blocks, transferBytes, PRECONDITION_TAG);
                Thread.Sleep(postWarmupSettleMs);

                // BEGIN_TIMED_READ_REGION
                // Each pass contains only seek plus exact synchronous reads.
                // Pattern reconstruction, byte comparison and hashing stay out.
                for (int pass = 0; pass < readPasses; pass++) {
                    long passStart = Counter();
                    SeekStart(handle);
                    for (int block = 0; block < blocks; block++) {
                        readLatencies.Add(
                            TimedRead(handle, aligned, transferBytes));
                    }
                    readPassElapsed.Add(
                        ToNanoseconds(Counter() - passStart));
                }
                // END_TIMED_READ_REGION

                // Full post-read verification proves the read-only phase left
                // the exact precondition bytes intact.
                postReadHash = VerifyAndHash(handle, aligned, observed,
                    expected, blocks, transferBytes, PRECONDITION_TAG);
                if (!String.Equals(warmupHash, postReadHash,
                        StringComparison.Ordinal)) {
                    throw new InvalidDataException(
                        "post-read integrity hash differs from warmup");
                }
                Close(ref handle);

                // Durable measured writes retain the v1 guardrail semantics.
                for (int pass = 0; pass < writePasses; pass++) {
                    handle = OpenWrite(dataPath, false);
                    SeekStart(handle);
                    uint tag = checked(
                        MEASURED_WRITE_TAG_BASE + (uint)pass);
                    long writePassStart = Counter();
                    for (int block = 0; block < blocks; block++) {
                        FillPattern(expected, block, tag);
                        Marshal.Copy(expected, 0, aligned, transferBytes);
                        writeLatencies.Add(
                            TimedWrite(handle, aligned, transferBytes));
                    }
                    flushLatencies.Add(TimedFlush(handle));
                    writePhaseNs = checked(writePhaseNs +
                        ToNanoseconds(Counter() - writePassStart));
                    Close(ref handle);
                    handle = OpenRead(dataPath);
                    string verified = VerifyAndHash(handle, aligned,
                        observed, expected, blocks, transferBytes, tag);
                    if (pass == writePasses - 1) {
                        finalHash = verified;
                    }
                    Close(ref handle);
                }

                long measuredReadBytes =
                    checked(fileBytes * readPasses);
                long measuredWriteBytes =
                    checked(fileBytes * writePasses);
                long readPhaseNs = Sum(readPassElapsed);
                long readServiceNs = Sum(readLatencies);
                long writeAndFlushServiceNs = checked(
                    Sum(writeLatencies) + Sum(flushLatencies));
                if (readPhaseNs < readServiceNs) {
                    throw new InvalidDataException(
                        "read phase is shorter than read service time");
                }
                return new WorkloadReport {
                    FileBytes = fileBytes,
                    TransferBytes = transferBytes,
                    BlocksPerPass = blocks,
                    PreconditionWriteOps = blocks,
                    WarmupReadOps = blocks,
                    MeasuredReadOps = checked(blocks * readPasses),
                    PostReadVerifyOps = blocks,
                    MeasuredWriteOps = checked(blocks * writePasses),
                    WriteVerifyReadOps = checked(blocks * writePasses),
                    FinalVerifyReadOps = blocks,
                    VerifiedReadOps = checked(
                        blocks * (2 + writePasses)),
                    FlushCalls = checked(writePasses + 1),
                    PostWarmupSettleMs = postWarmupSettleMs,
                    BytesPerSector = bytesPerSector,
                    FileAlignmentBytes = fileAlignmentBytes,
                    RequiredAlignmentBytes = requiredAlignment,
                    ReadPhaseElapsedNs = readPhaseNs,
                    ReadServiceElapsedNs = readServiceNs,
                    WritePhaseElapsedNs = writePhaseNs,
                    WriteAndFlushServiceElapsedNs =
                        writeAndFlushServiceNs,
                    ReadPhaseMibPerSec = MibPerSecond(
                        measuredReadBytes, readPhaseNs),
                    ReadServiceMibPerSec = MibPerSecond(
                        measuredReadBytes, readServiceNs),
                    WriteDurableMibPerSec = MibPerSecond(
                        measuredWriteBytes, writePhaseNs),
                    WriteAndFlushServiceMibPerSec = MibPerSecond(
                        measuredWriteBytes, writeAndFlushServiceNs),
                    WarmupSha256 = warmupHash,
                    PostReadSha256 = postReadHash,
                    FinalSha256 = finalHash,
                    ReadLatencyNs = Summarize(readLatencies),
                    ReadPassElapsedNs = Summarize(readPassElapsed),
                    WriteLatencyNs = Summarize(writeLatencies),
                    FlushLatencyNs = Summarize(flushLatencies),
                    ReadLatencyRawNs = readLatencies.ToArray(),
                    ReadPassElapsedRawNs = readPassElapsed.ToArray(),
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
    Write-Output 'BRIDGEVM_NVME_V2_WORKLOAD_COMPILE_OK'
    exit 0
}

New-Item -ItemType Directory -Force -Path $Root | Out-Null
New-Item -ItemType Directory -Force -Path `
    ([IO.Path]::GetDirectoryName($DataPath)) | Out-Null
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
        return ([BitConverter]::ToString(
            $sha.ComputeHash($bytes))).Replace('-', '').ToLowerInvariant()
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
    $stream = New-Object IO.FileStream($temp,
        [IO.FileMode]::CreateNew, [IO.FileAccess]::Write,
        [IO.FileShare]::None, 4096, [IO.FileOptions]::WriteThrough)
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
    $Run = [BridgeVM.NvmePerfV2.WarmSequentialWorkload]::Run(
        $DataPath, 2048, 128, 16, 4, 30000)
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
        read_pass_elapsed_ns = $Run.ReadPassElapsedRawNs
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
        read_pass_elapsed_ns = @()
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
        measured_read_bytes = ($Run.FileBytes * 16)
        post_read_verify_ops = $Run.PostReadVerifyOps
        post_read_verify_bytes = $Run.FileBytes
        measured_write_ops = $Run.MeasuredWriteOps
        measured_write_bytes = ($Run.FileBytes * 4)
        write_verify_read_ops = $Run.WriteVerifyReadOps
        write_verify_read_bytes = ($Run.FileBytes * 4)
        read_result_count = 16
        write_result_count = 4
        final_verify_read_ops = $Run.FinalVerifyReadOps
        final_verify_read_bytes = $Run.FileBytes
        verified_read_ops = $Run.VerifiedReadOps
        flush_calls = $Run.FlushCalls
        post_warmup_settle_ms = $Run.PostWarmupSettleMs
        bytes_per_sector = $Run.BytesPerSector
        file_alignment_bytes = $Run.FileAlignmentBytes
        required_alignment_bytes = $Run.RequiredAlignmentBytes
        warmup_sha256 = $Run.WarmupSha256
        post_read_sha256 = $Run.PostReadSha256
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
        read_pass_p50_ms = ($Run.ReadPassElapsedNs.P50Ns / 1000000.0)
        read_pass_p95_ms = ($Run.ReadPassElapsedNs.P95Ns / 1000000.0)
        read_pass_p99_ms = ($Run.ReadPassElapsedNs.P99Ns / 1000000.0)
        read_pass_max_ms = ($Run.ReadPassElapsedNs.MaxNs / 1000000.0)
        write_phase_elapsed_ns = $Run.WritePhaseElapsedNs
        write_and_flush_service_elapsed_ns = `
            $Run.WriteAndFlushServiceElapsedNs
        write_durable_mib_per_sec = $Run.WriteDurableMibPerSec
        write_and_flush_service_mib_per_sec = `
            $Run.WriteAndFlushServiceMibPerSec
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
        read_pass_elapsed_ns = [ordered]@{
            count = $Run.ReadPassElapsedNs.Count
            p50 = $Run.ReadPassElapsedNs.P50Ns
            p95 = $Run.ReadPassElapsedNs.P95Ns
            p99 = $Run.ReadPassElapsedNs.P99Ns
            max = $Run.ReadPassElapsedNs.MaxNs
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
        "__CONFIG_SHA__": ps_quote(config_sha256()),
        "__CONFIG_JSON__": ps_quote(config_json),
    }
    rendered = template
    for marker, value in replacements.items():
        rendered = rendered.replace(marker, value)
    if re.search(r"__[A-Z0-9_]+__", rendered):
        raise AssertionError("unexpanded workload template marker")
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
        return
    raise ValueError("generated guest workload must stay outside the repository")


def write_output(path: Path, payload: bytes) -> None:
    validate_output_path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags, 0o600)
    with os.fdopen(descriptor, "wb") as output:
        output.write(payload)


def nonce_from_script(script: bytes) -> str:
    matches = re.findall(
        rb"^\$Nonce = '([0-9a-f]{32})'\r$", script, re.MULTILINE
    )
    if len(matches) != 1:
        raise ValueError("workload script must contain one canonical nonce")
    return matches[0].decode("ascii")


def verify_output(script_path: Path, config_path: Path) -> None:
    validate_contract()
    for path in (script_path, config_path):
        if not path.is_file() or path.is_symlink():
            raise ValueError(
                "verified inputs must be regular non-symbolic-link files"
            )
        if path.stat().st_size >= MAX_PRIVATE_JSON_BYTES:
            raise ValueError("verified input exceeds the private artifact limit")
    script = script_path.read_bytes()
    nonce = nonce_from_script(script)
    if script != render(nonce):
        raise ValueError("workload script is not the exact v2 renderer output")
    if config_path.read_bytes() != canonical_config_bytes():
        raise ValueError("workload config is not the exact canonical v2 config")


def self_test() -> None:
    validate_contract()
    assert MAX_ESTIMATED_RAW_JSON_BYTES == 7_225_784
    assert MAX_ESTIMATED_RAW_JSON_BYTES < MAX_PRIVATE_JSON_BYTES
    assert FILE_BYTES == 2_147_483_648
    assert TRANSFER_BYTES == 131_072
    assert BLOCKS_PER_PASS == 16_384
    assert READ_OPS == 262_144
    assert WRITE_OPS == 65_536
    assert config_sha256() == EXPECTED_CONFIG_SHA256
    assert canonical_config()["queue_depth"] == 1
    assert canonical_config()["data_path"] == DATA_PATH

    nonce = "0123456789abcdef0123456789abcdef"
    payload = render(nonce)
    text = payload.decode("utf-8")
    assert payload.startswith(b"param([switch]$CompileOnly)\r\n")
    assert payload.endswith(b"\r\n")
    assert payload.count(b"\n") == payload.count(b"\r\n")
    assert b"\n" in payload
    required = (
        SCHEMA,
        WORKLOAD_PROFILE,
        DATA_PATH,
        "2048, 128, 16, 4, 30000",
        "FILE_ATTRIBUTE_NOT_CONTENT_INDEXED",
        "EnsureNotContentIndexed(dataPath)",
        "SetFileAttributesW(not-content-indexed)",
        "Thread.Sleep(postWarmupSettleMs)",
        "BEGIN_TIMED_READ_REGION",
        "END_TIMED_READ_REGION",
        "read_pass_elapsed_ns",
        "ReadPassElapsedRawNs",
        "PostReadSha256",
        "post-read integrity hash differs from warmup",
        PRECONDITION_SHA256,
        FINAL_SHA256,
        "BRIDGEVM_NVME_V2_WORKLOAD_COMPILE_OK",
        "Write-AtomicUtf8",
        config_sha256(),
    )
    for marker in required:
        assert marker in text, marker
    timed = text.split("// BEGIN_TIMED_READ_REGION", 1)[1].split(
        "// END_TIMED_READ_REGION", 1
    )[0]
    assert "TimedRead" in timed and "SeekStart" in timed
    for forbidden in ("VerifyPattern", "FillPattern", "Marshal.Copy", "HashBlock"):
        assert forbidden not in timed, forbidden
    after_timed = text.split("// END_TIMED_READ_REGION", 1)[1]
    assert "VerifyAndHash" in after_timed
    assert text.index("Thread.Sleep(postWarmupSettleMs)") < text.index(
        "// BEGIN_TIMED_READ_REGION"
    )
    assert text.index("// END_TIMED_READ_REGION") < text.index(
        "postReadHash = VerifyAndHash"
    )
    lowered = text.lower()
    for forbidden in ("password=", "credential=", "private key", "bearer "):
        assert forbidden not in lowered

    with tempfile.TemporaryDirectory(
        prefix="bridgevm-nvme-v2-render-"
    ) as temporary:
        root = Path(temporary)
        script_path = root / "nvme-workload-v2.ps1"
        config_path = root / "nvme-workload-v2-config.json"
        write_output(script_path, payload)
        write_output(config_path, canonical_config_bytes())
        verify_output(script_path, config_path)

        tampered_script = root / "tampered.ps1"
        tampered_script.write_bytes(payload.replace(b"30000", b"30001", 1))
        try:
            verify_output(tampered_script, config_path)
        except ValueError:
            pass
        else:
            raise AssertionError("tampered workload script was accepted")

        tampered_config = root / "tampered.json"
        tampered_config.write_bytes(canonical_config_bytes() + b"\n")
        try:
            verify_output(script_path, tampered_config)
        except ValueError:
            pass
        else:
            raise AssertionError("tampered workload config was accepted")

        victim = root / "victim"
        linked = root / "linked"
        victim.write_bytes(b"do-not-truncate")
        os.link(victim, linked)
        try:
            write_output(linked, b"replacement")
        except ValueError:
            pass
        else:
            raise AssertionError("an existing hardlink output was accepted")
        assert victim.read_bytes() == b"do-not-truncate"

    for invalid in ("", "not-hex", "0123456789abcdef", "a" * 31, "a" * 33):
        try:
            render(invalid)
        except ValueError:
            pass
        else:
            raise AssertionError("invalid nonce was accepted")
    try:
        validate_output_path(
            Path(__file__).resolve().parent / "forbidden-v2.ps1"
        )
    except ValueError:
        pass
    else:
        raise AssertionError("repository output path was accepted")
    print("render HVF NVMe workload v2 self-test: PASS")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--output", type=Path)
    result.add_argument("--config-output", type=Path)
    result.add_argument("--verify-output", type=Path)
    result.add_argument("--verify-config", type=Path)
    result.add_argument("--nonce")
    result.add_argument("--self-test", action="store_true")
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    if args.self_test:
        if any(
            value is not None
            for value in (
                args.output,
                args.config_output,
                args.verify_output,
                args.verify_config,
                args.nonce,
            )
        ):
            raise SystemExit("--self-test cannot be combined with other options")
        self_test()
        return 0
    if args.verify_output is not None or args.verify_config is not None:
        if (
            args.verify_output is None
            or args.verify_config is None
            or args.output is not None
            or args.config_output is not None
            or args.nonce is not None
        ):
            raise SystemExit(
                "verification requires only --verify-output and --verify-config"
            )
        try:
            verify_output(args.verify_output, args.verify_config)
        except (OSError, ValueError) as error:
            print(f"render HVF NVMe workload v2: {error}", file=sys.stderr)
            return 2
        print("render HVF NVMe workload v2 verification: PASS")
        return 0
    if args.output is None or args.config_output is None or args.nonce is None:
        raise SystemExit(
            "--output, --config-output and --nonce are required"
        )
    if args.output.resolve(strict=False) == args.config_output.resolve(
        strict=False
    ):
        raise SystemExit("--output and --config-output must differ")
    try:
        payload = render(args.nonce)
        validate_output_path(args.output)
        validate_output_path(args.config_output)
        write_output(args.output, payload)
        write_output(args.config_output, canonical_config_bytes())
    except (OSError, ValueError) as error:
        print(f"render HVF NVMe workload v2: {error}", file=sys.stderr)
        return 2
    print(
        f"rendered {args.output} bytes={len(payload)} "
        f"config_sha256={config_sha256()} "
        f"config_output={args.config_output}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
