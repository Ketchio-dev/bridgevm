using System;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
namespace BridgeVM {
    public sealed class BvTpmPcrReading {
        public readonly string Status;
        public readonly string Pcr0Sha256;
        public readonly string Pcr7Sha256;
        public BvTpmPcrReading(string status, string pcr0Sha256, string pcr7Sha256) {
            Status = status; Pcr0Sha256 = pcr0Sha256; Pcr7Sha256 = pcr7Sha256;
        }
    }
    // PCR 0 and PCR 7 of the SHA-256 bank through TBS. Only the SHA-256 of each
    // PCR value leaves this class; the values themselves are never returned.
    public static class BvTpmPcr {
        // TPM2_PCR_Read (TPM 2.0 Part 3, 22.4): tag TPM_ST_NO_SESSIONS 0x8001,
        // commandSize 20, TPM_CC_PCR_Read 0x0000017E, TPML_PCR_SELECTION count 1:
        // hash TPM_ALG_SHA256 0x000B, sizeofSelect 3, pcrSelect 0x81 0x00 0x00
        // (bit 0 = PCR 0, bit 7 = PCR 7).
        private static readonly byte[] Command = new byte[] {
            0x80, 0x01, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x01, 0x7E,
            0x00, 0x00, 0x00, 0x01, 0x00, 0x0B, 0x03, 0x81, 0x00, 0x00 };
        public static byte[] PcrReadCommand() { return (byte[])Command.Clone(); }

        [StructLayout(LayoutKind.Sequential)]
        private struct ContextParams2 { public uint Version; public uint Flags; }
        [DllImport("tbs.dll")] private static extern uint Tbsi_Context_Create(ref ContextParams2 parameters, out IntPtr context);
        [DllImport("tbs.dll")] private static extern uint Tbsip_Submit_Command(IntPtr context, uint locality, uint priority, byte[] command, uint commandLength, [Out] byte[] result, ref uint resultLength);
        [DllImport("tbs.dll")] private static extern uint Tbsip_Context_Close(IntPtr context);

        public static BvTpmPcrReading Read() {
            // TBS_CONTEXT_PARAMS2: TBS_CONTEXT_VERSION_TWO, includeTpm20 (bit 2).
            ContextParams2 parameters; parameters.Version = 2; parameters.Flags = 4;
            IntPtr context;
            if (Tbsi_Context_Create(ref parameters, out context) != 0) return Failure("tbs-context-failed");
            try {
                byte[] command = PcrReadCommand(), response = new byte[4096];
                uint length = (uint)response.Length;
                // TBS_COMMAND_LOCALITY_ZERO, TBS_COMMAND_PRIORITY_NORMAL (200).
                if (Tbsip_Submit_Command(context, 0, 200, command, (uint)command.Length, response, ref length) != 0) return Failure("tbs-submit-failed");
                return Parse(response, (int)Math.Min(length, (uint)response.Length));
            } finally { Tbsip_Context_Close(context); }
        }

        // Accepts only the 96-byte success response to the command above: header (10),
        // pcrUpdateCounter (4), the echoed selection (10), then two 32-byte digests.
        public static BvTpmPcrReading Parse(byte[] response, int length) {
            if (response == null || length < 10 || length > response.Length) return Failure("pcr-response-malformed");
            if (U16(response, 0) != 0x8001 || U32(response, 2) != (uint)length) return Failure("pcr-response-malformed");
            if (U32(response, 6) != 0) return Failure("tpm-error-response");
            if (length < 24) return Failure("pcr-response-malformed");
            if (U32(response, 14) != 1 || U16(response, 18) != 0x000B || response[20] != 3 ||
                response[21] != 0x81 || response[22] != 0 || response[23] != 0) return Failure("pcr-selection-mismatch");
            if (length != 96 || U32(response, 24) != 2 || U16(response, 28) != 32 || U16(response, 62) != 32) return Failure("pcr-response-malformed");
            return new BvTpmPcrReading("ok", Sha256Hex(response, 30), Sha256Hex(response, 64));
        }

        private static BvTpmPcrReading Failure(string status) { return new BvTpmPcrReading(status, "absent", "absent"); }
        private static uint U16(byte[] data, int offset) { return ((uint)data[offset] << 8) | data[offset + 1]; }
        private static uint U32(byte[] data, int offset) { return (U16(data, offset) << 16) | U16(data, offset + 2); }
        private static string Sha256Hex(byte[] data, int offset) {
            using (SHA256 sha = SHA256.Create()) {
                return BitConverter.ToString(sha.ComputeHash(data, offset, 32)).Replace("-", "").ToLowerInvariant();
            }
        }
    }
}
