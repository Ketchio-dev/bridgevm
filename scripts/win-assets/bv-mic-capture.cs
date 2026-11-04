// Captures the default capture endpoint and checks it against the host's index pattern
// (scripts/live-gates/hda-microphone-live-check.py): frame i of the looped test WAV holds
// L = i & 0xFFFF and R = 0xA500 | (i >> 16), i < PatternFrames. First in WASAPI exclusive
// mode, in the first format IsFormatSupported accepts (48 kHz 16-bit stereo as WAVEFORMATEX,
// then as WAVEFORMATEXTENSIBLE, then the endpoint's device format from its property store):
// no conversion or volume touches those bytes, so 48 kHz 16-bit stereo PCM is compared
// bit-exact. Then in shared mode with GetMixFormat, the path most apps use, where the engine
// converts and may apply volume: the left channel's correlation, gain and phase against the
// pattern's ramp. C# 5.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;

public static class BvMicCapture
{
    const int PatternFrames = 480000;
    const int SpanFrames = 96000;
    const int Shared = 0, Exclusive = 1;
    const int EventCallback = 0x00040000;
    const int ClsctxAll = 0x17;
    const int ECapture = 1, EConsole = 0;
    const int BufferNotAligned = unchecked((int)0x88890019);
    const uint SilentFlag = 0x2, DiscontinuityFlag = 0x1;
    static readonly Guid PcmSubFormat = new Guid("00000001-0000-0010-8000-00aa00389b71");
    static readonly Guid FloatSubFormat = new Guid("00000003-0000-0010-8000-00aa00389b71");

    [ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")]
    class MMDeviceEnumerator { }

    [ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMMDeviceEnumerator
    {
        [PreserveSig] int EnumAudioEndpoints(int dataFlow, int stateMask, out IntPtr devices);
        [PreserveSig] int GetDefaultAudioEndpoint(int dataFlow, int role, out IMMDevice endpoint);
    }

    [ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IMMDevice
    {
        [PreserveSig] int Activate(ref Guid iid, int clsCtx, IntPtr parameters, [MarshalAs(UnmanagedType.IUnknown)] out object instance);
        [PreserveSig] int OpenPropertyStore(int access, out IPropertyStore store);
        [PreserveSig] int GetId([MarshalAs(UnmanagedType.LPWStr)] out string id);
    }

    [StructLayout(LayoutKind.Sequential)]
    struct PropertyKey
    {
        public Guid Format;
        public int Id;
    }

    [ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IPropertyStore
    {
        [PreserveSig] int GetCount(out int count);
        [PreserveSig] int GetAt(int index, out PropertyKey key);
        [PreserveSig] int GetValue(ref PropertyKey key, IntPtr value);
    }

    [ComImport, Guid("1CB9AD4C-DBFA-4c32-B178-C2F568A703B2"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IAudioClient
    {
        [PreserveSig] int Initialize(int shareMode, int streamFlags, long bufferDuration, long periodicity, IntPtr format, IntPtr session);
        [PreserveSig] int GetBufferSize(out uint frames);
        [PreserveSig] int GetStreamLatency(out long latency);
        [PreserveSig] int GetCurrentPadding(out uint frames);
        [PreserveSig] int IsFormatSupported(int shareMode, IntPtr format, IntPtr closestMatch);
        [PreserveSig] int GetMixFormat(out IntPtr format);
        [PreserveSig] int GetDevicePeriod(out long defaultPeriod, out long minimumPeriod);
        [PreserveSig] int Start();
        [PreserveSig] int Stop();
        [PreserveSig] int Reset();
        [PreserveSig] int SetEventHandle(IntPtr handle);
        [PreserveSig] int GetService(ref Guid iid, [MarshalAs(UnmanagedType.IUnknown)] out object service);
    }

    [ComImport, Guid("C8ADBD64-E71E-48a0-A4DE-185C395CD317"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IAudioCaptureClient
    {
        [PreserveSig] int GetBuffer(out IntPtr data, out uint frames, out uint flags, out ulong devicePosition, out ulong qpcPosition);
        [PreserveSig] int ReleaseBuffer(uint frames);
        [PreserveSig] int GetNextPacketSize(out uint frames);
    }

    [DllImport("ole32.dll")]
    static extern int PropVariantClear(IntPtr value);

    /// One GetBuffer: where it landed in the output, its size, flags, the device position and
    /// QPC time (100 ns) Windows gave it, and when this thread read it (QPC, 100 ns).
    struct Packet
    {
        public int At, Count;
        public uint Flags;
        public ulong Position, Qpc;
        public long Read;
    }

    struct Wave
    {
        public int Tag, Channels, Rate, BlockAlign, Bits, ValidBits, Mask;
        public Guid SubFormat;
    }

    static void Check(int hr, string step)
    {
        if (hr < 0) throw new InvalidOperationException(step + " hr=0x" + hr.ToString("X8"));
    }

    static IAudioClient Activate(IMMDevice device)
    {
        Guid iid = typeof(IAudioClient).GUID;
        object instance;
        Check(device.Activate(ref iid, ClsctxAll, IntPtr.Zero, out instance), "Activate");
        return (IAudioClient)instance;
    }

    /// 48 kHz 16-bit stereo PCM as WAVEFORMATEX, or as WAVEFORMATEXTENSIBLE with front left and right.
    static IntPtr Format(bool extensible)
    {
        int size = extensible ? 40 : 18;
        IntPtr format = Marshal.AllocCoTaskMem(size);
        byte[] bytes = new byte[size];
        Action<int, int> u16 = (at, value) => { bytes[at] = (byte)value; bytes[at + 1] = (byte)(value >> 8); };
        Action<int, int> u32 = (at, value) => { u16(at, value & 0xFFFF); u16(at + 2, (value >> 16) & 0xFFFF); };
        u16(0, extensible ? 0xFFFE : 1);
        u16(2, 2);
        u32(4, 48000);
        u32(8, 192000);
        u16(12, 4);
        u16(14, 16);
        u16(16, extensible ? 22 : 0);
        if (extensible)
        {
            u16(18, 16);
            u32(20, 3);
            PcmSubFormat.ToByteArray().CopyTo(bytes, 24);
        }
        Marshal.Copy(bytes, 0, format, size);
        return format;
    }

    /// The endpoint's device format (PKEY_AudioEngine_DeviceFormat, a VT_BLOB), or Zero.
    static IntPtr DeviceFormat(IMMDevice device)
    {
        IPropertyStore store;
        if (device.OpenPropertyStore(0, out store) < 0 || store == null) return IntPtr.Zero;
        PropertyKey key = new PropertyKey { Format = new Guid("F19F064D-082C-4E27-BC73-6882A1BB8E4C"), Id = 0 };
        IntPtr value = Marshal.AllocCoTaskMem(32);
        try
        {
            Marshal.Copy(new byte[32], 0, value, 32);
            if (store.GetValue(ref key, value) < 0 || Marshal.ReadInt16(value) != 65) return IntPtr.Zero;
            int size = Marshal.ReadInt32(value, 8);
            IntPtr blob = Marshal.ReadIntPtr(value, 8 + IntPtr.Size);
            if (size < 18 || blob == IntPtr.Zero) return IntPtr.Zero;
            byte[] bytes = new byte[size];
            Marshal.Copy(blob, bytes, 0, size);
            IntPtr format = Marshal.AllocCoTaskMem(size);
            Marshal.Copy(bytes, 0, format, size);
            return format;
        }
        finally
        {
            PropVariantClear(value);
            Marshal.FreeCoTaskMem(value);
            Marshal.ReleaseComObject(store);
        }
    }

    static Wave Read(IntPtr format)
    {
        Wave wave = new Wave();
        wave.Tag = (ushort)Marshal.ReadInt16(format, 0);
        wave.Channels = (ushort)Marshal.ReadInt16(format, 2);
        wave.Rate = Marshal.ReadInt32(format, 4);
        wave.BlockAlign = (ushort)Marshal.ReadInt16(format, 12);
        wave.Bits = (ushort)Marshal.ReadInt16(format, 14);
        wave.ValidBits = wave.Bits;
        if (wave.Tag == 0xFFFE && (ushort)Marshal.ReadInt16(format, 16) >= 22)
        {
            wave.ValidBits = (ushort)Marshal.ReadInt16(format, 18);
            wave.Mask = Marshal.ReadInt32(format, 20);
            byte[] guid = new byte[16];
            Marshal.Copy(IntPtr.Add(format, 24), guid, 0, 16);
            wave.SubFormat = new Guid(guid);
        }
        return wave;
    }

    static bool IsPcm(Wave wave) { return wave.Tag == 1 || (wave.Tag == 0xFFFE && wave.SubFormat == PcmSubFormat); }

    static bool IsFloat(Wave wave) { return wave.Tag == 3 || (wave.Tag == 0xFFFE && wave.SubFormat == FloatSubFormat); }

    static string Describe(Wave wave)
    {
        string kind = IsPcm(wave) ? "pcm" : IsFloat(wave) ? "float" : "other";
        return "tag=0x" + wave.Tag.ToString("X4") + " kind=" + kind + " channels=" + wave.Channels + " rate=" + wave.Rate
            + " bits=" + wave.Bits + " valid_bits=" + wave.ValidBits + " mask=0x" + wave.Mask.ToString("X");
    }

    /// Starts `client`, keeps `wanted` frames of `blockAlign` bytes (zeros for silent
    /// packets), and stops it.
    static byte[] Record(IAudioClient client, int wanted, int blockAlign, List<Packet> packets, out int frames, out int silentPackets, out int discontinuities)
    {
        AutoResetEvent ready = new AutoResetEvent(false);
        Check(client.SetEventHandle(ready.SafeWaitHandle.DangerousGetHandle()), "SetEventHandle");
        Guid captureIid = typeof(IAudioCaptureClient).GUID;
        object service;
        Check(client.GetService(ref captureIid, out service), "GetService");
        IAudioCaptureClient capture = (IAudioCaptureClient)service;
        byte[] data = new byte[wanted * blockAlign];
        frames = 0;
        silentPackets = 0;
        discontinuities = 0;
        int timeouts = 0;
        Check(client.Start(), "Start");
        try
        {
            while (frames < wanted)
            {
                if (!ready.WaitOne(500))
                {
                    if (++timeouts > 4) throw new InvalidOperationException("no capture packets for 2 s");
                    continue;
                }
                uint packet;
                Check(capture.GetNextPacketSize(out packet), "GetNextPacketSize");
                while (packet > 0)
                {
                    IntPtr buffer;
                    uint count, flags;
                    ulong devicePosition, qpcPosition;
                    Check(capture.GetBuffer(out buffer, out count, out flags, out devicePosition, out qpcPosition), "GetBuffer");
                    if (packets != null)
                    {
                        packets.Add(new Packet { At = frames, Count = (int)count, Flags = flags, Position = devicePosition, Qpc = qpcPosition,
                            Read = (long)(Stopwatch.GetTimestamp() * (10000000.0 / Stopwatch.Frequency)) });
                    }
                    int take = (int)Math.Min(count, (uint)(wanted - frames));
                    if ((flags & SilentFlag) != 0) silentPackets++;
                    else if (take > 0) Marshal.Copy(buffer, data, frames * blockAlign, take * blockAlign);
                    if ((flags & DiscontinuityFlag) != 0) discontinuities++;
                    frames += take;
                    Check(capture.ReleaseBuffer(count), "ReleaseBuffer");
                    Check(capture.GetNextPacketSize(out packet), "GetNextPacketSize");
                }
            }
        }
        finally
        {
            client.Stop();
            Marshal.ReleaseComObject(capture);
        }
        return data;
    }

    static string CaptureExclusive(IMMDevice device, string id, int seconds, StringBuilder lines)
    {
        string[] names = { "pcm", "extensible", "device" };
        IntPtr[] formats = { Format(false), Format(true), DeviceFormat(device) };
        try
        {
            int chosen = -1;
            for (int i = 0; i < formats.Length; i++)
            {
                if (formats[i] == IntPtr.Zero)
                {
                    lines.AppendLine("MIC FORMAT exclusive " + names[i] + " none");
                    continue;
                }
                IAudioClient query = Activate(device);
                int supported = query.IsFormatSupported(Exclusive, formats[i], IntPtr.Zero);
                Marshal.ReleaseComObject(query);
                lines.AppendLine("MIC FORMAT exclusive " + names[i] + " " + Describe(Read(formats[i])) + " hr=0x" + supported.ToString("X8"));
                if (supported == 0 && chosen < 0) chosen = i;
            }
            if (chosen < 0) return "MIC ERROR no exclusive format is supported";
            Wave wave = Read(formats[chosen]);
            long duration = 100000; // 10 ms
            IAudioClient client = Activate(device);
            int hr = client.Initialize(Exclusive, EventCallback, duration, duration, formats[chosen], IntPtr.Zero);
            if (hr == BufferNotAligned)
            {
                uint aligned;
                Check(client.GetBufferSize(out aligned), "GetBufferSize");
                Marshal.ReleaseComObject(client);
                duration = (long)(10000000.0 * aligned / wave.Rate + 0.5);
                client = Activate(device);
                hr = client.Initialize(Exclusive, EventCallback, duration, duration, formats[chosen], IntPtr.Zero);
            }
            int frames, silentPackets, discontinuities;
            byte[] pcm;
            List<Packet> packets = new List<Packet>();
            try
            {
                Check(hr, "Initialize");
                uint bufferFrames;
                long defaultPeriod, minimumPeriod;
                Check(client.GetBufferSize(out bufferFrames), "GetBufferSize");
                Check(client.GetDevicePeriod(out defaultPeriod, out minimumPeriod), "GetDevicePeriod");
                lines.AppendLine("MIC LAYOUT buffer_frames=" + bufferFrames + " default_period_hns=" + defaultPeriod
                    + " minimum_period_hns=" + minimumPeriod);
                pcm = Record(client, wave.Rate * seconds, wave.BlockAlign, packets, out frames, out silentPackets, out discontinuities);
            }
            finally
            {
                Marshal.ReleaseComObject(client);
            }
            DescribePackets(packets, lines);
            bool exact = IsPcm(wave) && wave.Channels == 2 && wave.Bits == 16 && wave.Rate == 48000 && wave.BlockAlign == 4;
            return "MIC CAPTURE endpoint=" + Convert.ToBase64String(Encoding.UTF8.GetBytes(id)) + " format=" + names[chosen]
                + " period_hns=" + duration + " frames=" + frames + " silent_packets=" + silentPackets
                + " discontinuities=" + discontinuities + " " + (exact ? Analyze(pcm, frames, lines) : "analysis=not-48k-16-bit-stereo-pcm");
        }
        finally
        {
            foreach (IntPtr format in formats)
            {
                if (format != IntPtr.Zero) Marshal.FreeCoTaskMem(format);
            }
        }
    }

    static string CaptureShared(IMMDevice device, int seconds)
    {
        IAudioClient client = null;
        IntPtr mix = IntPtr.Zero;
        try
        {
            client = Activate(device);
            Check(client.GetMixFormat(out mix), "GetMixFormat");
            Wave wave = Read(mix);
            Check(client.Initialize(Shared, EventCallback, 1000000, 0, mix, IntPtr.Zero), "Initialize");
            int frames, silentPackets, discontinuities;
            byte[] data = Record(client, wave.Rate * seconds, wave.BlockAlign, null, out frames, out silentPackets, out discontinuities);
            string fit = "correlation=none reason=not-48k-float32-or-pcm16";
            if (wave.Rate == 48000 && ((IsFloat(wave) && wave.Bits == 32) || (IsPcm(wave) && wave.Bits == 16)))
                fit = Correlate(data, frames, wave.BlockAlign, IsFloat(wave));
            return "MIC SHARED " + Describe(wave) + " frames=" + frames + " silent_packets=" + silentPackets
                + " discontinuities=" + discontinuities + " " + fit;
        }
        catch (InvalidOperationException error)
        {
            return "MIC SHARED ERROR " + error.Message;
        }
        finally
        {
            if (mix != IntPtr.Zero) Marshal.FreeCoTaskMem(mix);
            if (client != null) Marshal.ReleaseComObject(client);
        }
    }

    public static string Run(int seconds)
    {
        IMMDeviceEnumerator enumerator = (IMMDeviceEnumerator)new MMDeviceEnumerator();
        IMMDevice device;
        Check(enumerator.GetDefaultAudioEndpoint(ECapture, EConsole, out device), "GetDefaultAudioEndpoint");
        string id;
        Check(device.GetId(out id), "GetId");
        StringBuilder lines = new StringBuilder();
        string exclusive;
        try
        {
            exclusive = CaptureExclusive(device, id, seconds, lines);
        }
        catch (InvalidOperationException error)
        {
            exclusive = "MIC ERROR " + error.Message;
        }
        lines.AppendLine(exclusive);
        lines.AppendLine(CaptureShared(device, seconds));
        return lines.ToString();
    }

    /// The left channel against the pattern's left channel, the 16-bit ramp (short)(i & 0xFFFF):
    /// the phase from its largest downward step (the wrap from 32767 to -32768), then the
    /// Pearson correlation and least-squares gain at the best phase within 2 frames of it.
    /// Samples are 32-bit float (scaled by 32768) or 16-bit PCM, left first in each frame.
    public static string Correlate(byte[] data, int frames, int blockAlign, bool isFloat)
    {
        double[] left = new double[frames];
        for (int n = 0; n < frames; n++)
        {
            int at = n * blockAlign;
            left[n] = isFloat ? BitConverter.ToSingle(data, at) * 32768.0 : BitConverter.ToInt16(data, at);
        }
        int wrap = -1;
        double step = 0;
        for (int n = 1; n < frames; n++)
        {
            double difference = left[n] - left[n - 1];
            if (difference < step)
            {
                step = difference;
                wrap = n;
            }
        }
        if (wrap < 0 || step > -16384) return "correlation=none reason=no-wrap min_step=" + (int)step;
        int start = ((32768 - wrap) % 65536 + 65536) % 65536;
        double best = -2, bestGain = 0;
        int bestPhase = start;
        for (int shift = -2; shift <= 2; shift++)
        {
            int phase = (start + shift + 65536) % 65536;
            double se = 0, sx = 0, see = 0, sxx = 0, sex = 0;
            for (int n = 0; n < frames; n++)
            {
                double expected = unchecked((short)((phase + n) & 0xFFFF));
                double x = left[n];
                se += expected;
                sx += x;
                see += expected * expected;
                sxx += x * x;
                sex += expected * x;
            }
            double covariance = sex - se * sx / frames, ve = see - se * se / frames, vx = sxx - sx * sx / frames;
            double r = ve > 0 && vx > 0 ? covariance / Math.Sqrt(ve * vx) : 0;
            if (r > best)
            {
                best = r;
                bestGain = covariance / ve;
                bestPhase = phase;
            }
        }
        return "correlation=" + best.ToString("F6", CultureInfo.InvariantCulture) + " gain="
            + bestGain.ToString("F4", CultureInfo.InvariantCulture) + " phase=" + bestPhase + " wrap_at=" + wrap;
    }

    /// The first 40 packets one per line (times in ms from the first packet's QPC time), then
    /// how many packets had each size and each device-position step, and the latest read.
    static void DescribePackets(List<Packet> packets, StringBuilder lines)
    {
        if (packets.Count == 0) return;
        double origin = packets[0].Qpc;
        SortedDictionary<int, int> sizes = new SortedDictionary<int, int>(), steps = new SortedDictionary<int, int>();
        double latestRead = double.MinValue;
        for (int i = 0; i < packets.Count; i++)
        {
            Packet packet = packets[i];
            if (i < 40)
            {
                lines.AppendLine("MIC PACKET n=" + i + " at=" + packet.At + " count=" + packet.Count + " flags=0x" + packet.Flags.ToString("X")
                    + " pos=" + packet.Position + " qpc_ms=" + ((packet.Qpc - origin) / 10000.0).ToString("F2", CultureInfo.InvariantCulture)
                    + " read_ms=" + ((packet.Read - origin) / 10000.0).ToString("F2", CultureInfo.InvariantCulture));
            }
            int size;
            sizes[packet.Count] = sizes.TryGetValue(packet.Count, out size) ? size + 1 : 1;
            if (i > 0)
            {
                int step = (int)((long)packet.Position - (long)packets[i - 1].Position), seen;
                steps[step] = steps.TryGetValue(step, out seen) ? seen + 1 : 1;
            }
            latestRead = Math.Max(latestRead, (packet.Read - (double)packet.Qpc) / 10000.0);
        }
        StringBuilder summary = new StringBuilder("MIC PACKETS total=" + packets.Count + " sizes=");
        foreach (KeyValuePair<int, int> entry in sizes) summary.Append(entry.Key + ":" + entry.Value + ",");
        summary.Append(" pos_steps=");
        foreach (KeyValuePair<int, int> entry in steps) summary.Append(entry.Key + ":" + entry.Value + ",");
        summary.Append(" max_read_after_qpc_ms=" + latestRead.ToString("F2", CultureInfo.InvariantCulture));
        lines.AppendLine(summary.ToString());
    }

    /// -1 unless the frame at `offset` is a pattern frame.
    static int Index(byte[] pcm, int offset)
    {
        int left = pcm[offset * 4] | (pcm[offset * 4 + 1] << 8);
        int right = pcm[offset * 4 + 2] | (pcm[offset * 4 + 3] << 8);
        if ((right & 0xFFF8) != 0xA500) return -1;
        int index = ((right & 7) << 16) | left;
        return index < PatternFrames ? index : -1;
    }

    /// Pattern frames, the first one's offset and index, breaks in the sequence after it
    /// (a silent gap or a jump counts once, where it starts), the longest unbroken run,
    /// and the SHA-256 of the unbroken span from the first pattern frame. The first 20 breaks
    /// go to `details` (if any) as MIC BREAK lines: output offset, expected and found index.
    public static string Analyze(byte[] pcm, int frames, StringBuilder details)
    {
        int patternFrames = 0, firstOffset = -1, firstIndex = -1, breaks = 0, run = 0, longest = 0, span = 0;
        int expected = -1;
        bool gap = false;
        for (int offset = 0; offset < frames; offset++)
        {
            int index = Index(pcm, offset);
            if (index >= 0) patternFrames++;
            if (firstOffset < 0)
            {
                if (index < 0) continue;
                firstOffset = offset;
                firstIndex = index;
                expected = index;
            }
            if (index >= 0 && index == expected)
            {
                run++;
            }
            else
            {
                if (!gap)
                {
                    breaks++;
                    if (details != null && breaks <= 20)
                    {
                        details.AppendLine("MIC BREAK at=" + offset + " expected=" + expected + " got=" + index
                            + " delta=" + (index >= 0 ? (index - expected).ToString() : "gap"));
                    }
                }
                run = index >= 0 ? 1 : 0;
            }
            gap = index < 0;
            if (index >= 0) expected = (index + 1) % PatternFrames;
            longest = Math.Max(longest, run);
            if (breaks == 0) span = Math.Min(span + 1, SpanFrames);
        }
        string hash = "none";
        if (span > 0)
        {
            using (SHA256 sha = SHA256.Create())
            {
                hash = BitConverter.ToString(sha.ComputeHash(pcm, firstOffset * 4, span * 4)).Replace("-", "").ToLowerInvariant();
            }
        }
        return "pattern_frames=" + patternFrames + " first_offset=" + firstOffset + " first_index=" + firstIndex
            + " breaks=" + breaks + " longest_run=" + longest + " span_frames=" + span + " span_sha256=" + hash;
    }
}
