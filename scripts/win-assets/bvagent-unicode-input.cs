using System;
using System.Runtime.InteropServices;

namespace BridgeVM {
    // Keep this type separate from the resident channel's P/Invoke declarations.
    // A successful return means stream insertion, NOT application consumption.
    public static partial class BvUnicodeInput {
        public const int MaximumCodeUnits = 65536;

        [StructLayout(LayoutKind.Sequential)]
        public struct KeyboardInput {
            public ushort VirtualKey;
            public ushort Scan;
            public uint Flags;
            public uint Time;
            public UIntPtr ExtraInfo;
        }

        [StructLayout(LayoutKind.Sequential)]
        public struct MouseInput {
            public int X;
            public int Y;
            public uint MouseData;
            public uint Flags;
            public uint Time;
            public UIntPtr ExtraInfo;
        }

        [StructLayout(LayoutKind.Explicit)]
        public struct InputUnion {
            [FieldOffset(0)] public KeyboardInput Keyboard;
            [FieldOffset(0)] public MouseInput Mouse;
        }

        [StructLayout(LayoutKind.Sequential)]
        public struct Input {
            public uint Type;
            public InputUnion Data;
        }

        [DllImport("user32.dll", SetLastError = true)]
        private static extern uint SendInput(uint count, [In] Input[] inputs, int size);

        public static Input[] Build(string text) {
            if (String.IsNullOrEmpty(text) || text.Length > MaximumCodeUnits) {
                throw new ArgumentException("unicode-input-length");
            }
            for (int i = 0; i < text.Length; i++) {
                if (Char.IsHighSurrogate(text[i])) {
                    if (i + 1 >= text.Length || !Char.IsLowSurrogate(text[i + 1])) {
                        throw new ArgumentException("unicode-input-surrogate");
                    }
                    i++;
                } else if (Char.IsLowSurrogate(text[i])) {
                    throw new ArgumentException("unicode-input-surrogate");
                }
            }
            Input[] inputs = new Input[text.Length * 2];
            for (int i = 0; i < text.Length; i++) {
                Input down = new Input();
                down.Type = 1; // INPUT_KEYBOARD
                down.Data.Keyboard.Scan = text[i];
                down.Data.Keyboard.Flags = 4; // KEYEVENTF_UNICODE
                inputs[i * 2] = down;
                Input up = down;
                up.Data.Keyboard.Flags = 6; // KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                inputs[i * 2 + 1] = up;
            }
            return inputs;
        }

        public static void RequireComplete(uint inserted, uint requested) {
            if (requested == 0 || inserted != requested) {
                // An unknown prefix may already be inserted. Never replay it.
                throw new InvalidOperationException("unicode-input-incomplete inserted="
                    + inserted + " requested=" + requested);
            }
        }

        public static uint Insert(string text) {
            return InsertEvents(Build(text));
        }
    }
}
