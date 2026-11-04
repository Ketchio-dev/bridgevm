// Captures the default capture endpoint in WASAPI exclusive mode (48 kHz, 16-bit,
// stereo: the only format BridgeVM's microphone offers, so no conversion or volume
// touches the bytes) and checks them against the host's index pattern
// (scripts/live-gates/hda-microphone-live-check.py): frame i of the looped test WAV
// holds L = i & 0xFFFF and R = 0xA500 | (i >> 16), i < PatternFrames. C# 5.
using System;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;

public static class BvMicCapture
{
    const int PatternFrames = 480000;
    const int SpanFrames = 96000;
    const int Exclusive = 1;
    const int EventCallback = 0x00040000;
    const int ClsctxAll = 0x17;
    const int ECapture = 1, EConsole = 0;
    const int BufferNotAligned = unchecked((int)0x88890019);
    const int UnsupportedFormat = unchecked((int)0x88890008);
    const uint SilentFlag = 0x2, DiscontinuityFlag = 0x1;

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
        [PreserveSig] int OpenPropertyStore(int access, out IntPtr store);
        [PreserveSig] int GetId([MarshalAs(UnmanagedType.LPWStr)] out string id);
    }

    [ComImport, Guid("1CB9AD4C-DBFA-4c32-B178-C2F568A703B2"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    interface IAudioClient
    {
        [PreserveSig] int Initialize(int shareMode, int streamFlags, long bufferDuration, long periodicity, IntPtr format, IntPtr session);
        [PreserveSig] int GetBufferSize(out uint frames);
        [PreserveSig] int GetStreamLatency(out long latency);
        [PreserveSig] int GetCurrentPadding(out uint frames);
        [PreserveSig] int IsFormatSupported(int shareMode, IntPtr format, out IntPtr closest);
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

    static void Check(int hr, string step)
    {
        if (hr < 0) throw new InvalidOperationException("MIC ERROR " + step + " hr=0x" + hr.ToString("X8"));
    }

    /// WAVEFORMATEX PCM, or WAVEFORMATEXTENSIBLE PCM with front left and right.
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
            Guid.Parse("00000001-0000-0010-8000-00aa00389b71").ToByteArray().CopyTo(bytes, 24);
        }
        Marshal.Copy(bytes, 0, format, size);
        return format;
    }

    static IAudioClient Open(IMMDevice device, long duration, bool extensible, out int hr)
    {
        Guid iid = typeof(IAudioClient).GUID;
        object instance;
        Check(device.Activate(ref iid, ClsctxAll, IntPtr.Zero, out instance), "Activate");
        IAudioClient client = (IAudioClient)instance;
        IntPtr format = Format(extensible);
        try
        {
            hr = client.Initialize(Exclusive, EventCallback, duration, duration, format, IntPtr.Zero);
        }
        finally
        {
            Marshal.FreeCoTaskMem(format);
        }
        return client;
    }

    public static string Run(int seconds)
    {
        IMMDeviceEnumerator enumerator = (IMMDeviceEnumerator)new MMDeviceEnumerator();
        IMMDevice device;
        Check(enumerator.GetDefaultAudioEndpoint(ECapture, EConsole, out device), "GetDefaultAudioEndpoint");
        string id;
        Check(device.GetId(out id), "GetId");
        long duration = 100000; // 10 ms
        bool extensible = false;
        int hr;
        IAudioClient client = Open(device, duration, extensible, out hr);
        if (hr == UnsupportedFormat)
        {
            Marshal.ReleaseComObject(client);
            extensible = true;
            client = Open(device, duration, extensible, out hr);
        }
        if (hr == BufferNotAligned)
        {
            uint aligned;
            Check(client.GetBufferSize(out aligned), "GetBufferSize");
            Marshal.ReleaseComObject(client);
            duration = (long)(10000000.0 * aligned / 48000 + 0.5);
            client = Open(device, duration, extensible, out hr);
        }
        Check(hr, "Initialize");
        AutoResetEvent ready = new AutoResetEvent(false);
        Check(client.SetEventHandle(ready.SafeWaitHandle.DangerousGetHandle()), "SetEventHandle");
        Guid captureIid = typeof(IAudioCaptureClient).GUID;
        object service;
        Check(client.GetService(ref captureIid, out service), "GetService");
        IAudioCaptureClient capture = (IAudioCaptureClient)service;

        int wanted = 48000 * seconds;
        byte[] pcm = new byte[(wanted + 48000) * 4];
        int frames = 0, silentPackets = 0, discontinuities = 0, timeouts = 0;
        Check(client.Start(), "Start");
        try
        {
            while (frames < wanted)
            {
                if (!ready.WaitOne(500))
                {
                    if (++timeouts > 4) throw new InvalidOperationException("MIC ERROR no capture packets for 2 s");
                    continue;
                }
                uint packet;
                Check(capture.GetNextPacketSize(out packet), "GetNextPacketSize");
                while (packet > 0)
                {
                    IntPtr data;
                    uint count, flags;
                    ulong devicePosition, qpcPosition;
                    Check(capture.GetBuffer(out data, out count, out flags, out devicePosition, out qpcPosition), "GetBuffer");
                    int take = (int)Math.Min(count, (uint)(pcm.Length / 4 - frames));
                    if ((flags & SilentFlag) != 0) silentPackets++;
                    else Marshal.Copy(data, pcm, frames * 4, take * 4);
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
        }
        return "MIC CAPTURE endpoint=" + Convert.ToBase64String(Encoding.UTF8.GetBytes(id))
            + " extensible=" + (extensible ? 1 : 0) + " period_hns=" + duration
            + " frames=" + frames + " silent_packets=" + silentPackets + " discontinuities=" + discontinuities
            + " " + Analyze(pcm, frames);
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
    /// and the SHA-256 of the unbroken span from the first pattern frame.
    static string Analyze(byte[] pcm, int frames)
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
                if (!gap) breaks++;
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
