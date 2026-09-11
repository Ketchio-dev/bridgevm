namespace BridgeVM {
    using System;
    using System.Collections.Generic;
    using System.Runtime.InteropServices;

    public static partial class BvUnicodeInput {
        private static readonly Dictionary<string, ushort> NamedKeys = new Dictionary<string, ushort>(StringComparer.Ordinal) {
            { "backspace", 0x08 }, { "tab", 0x09 }, { "enter", 0x0D },
            { "esc", 0x1B }, { "escape", 0x1B }, { "space", 0x20 },
            { "pageup", 0x21 }, { "pagedown", 0x22 }, { "end", 0x23 }, { "home", 0x24 },
            { "left", 0x25 }, { "up", 0x26 }, { "right", 0x27 }, { "down", 0x28 },
            { "insert", 0x2D }, { "delete", 0x2E },
            { "f1", 0x70 }, { "f2", 0x71 }, { "f3", 0x72 }, { "f4", 0x73 },
            { "f5", 0x74 }, { "f6", 0x75 }, { "f7", 0x76 }, { "f8", 0x77 },
            { "f9", 0x78 }, { "f10", 0x79 }, { "f11", 0x7A }, { "f12", 0x7B }
        };
        private static readonly HashSet<string> Navigation = new HashSet<string>(StringComparer.Ordinal) {
            "left", "up", "right", "down", "home", "end", "pageup", "pagedown"
        };

        public static Input[] BuildKey(string command) {
            if (String.IsNullOrEmpty(command) || command.Length > 32) {
                throw new ArgumentException("key-input-length");
            }
            int separator = command.LastIndexOf('+');
            string prefix = separator < 0 ? "" : command.Substring(0, separator);
            string key = command.Substring(separator + 1);
            bool letter = key.Length == 1 && "acvxyz".IndexOf(key[0]) >= 0;
            bool navigation = Navigation.Contains(key);
            bool allowed = prefix == "" && NamedKeys.ContainsKey(key)
                || prefix == "shift" && (navigation || key == "tab")
                || prefix == "ctrl" && (navigation || letter || key == "backspace" || key == "delete")
                || prefix == "ctrl+shift" && navigation
                || prefix == "alt" && (key == "tab" || key == "enter" || key == "f4");
            if (!allowed) { throw new ArgumentException("unsupported-key-input"); }
            ushort code = letter ? (ushort)Char.ToUpperInvariant(key[0]) : NamedKeys[key];
            List<ushort> modifiers = new List<ushort>();
            if (prefix == "ctrl" || prefix == "ctrl+shift") { modifiers.Add(0x11); }
            if (prefix == "shift" || prefix == "ctrl+shift") { modifiers.Add(0x10); }
            if (prefix == "alt") { modifiers.Add(0x12); }
            bool extended = (code >= 0x21 && code <= 0x28) || code == 0x2D || code == 0x2E;
            List<Input> inputs = new List<Input>();
            foreach (ushort modifier in modifiers) { inputs.Add(KeyEvent(modifier, false, false)); }
            inputs.Add(KeyEvent(code, false, extended));
            inputs.Add(KeyEvent(code, true, extended));
            for (int i = modifiers.Count - 1; i >= 0; i--) { inputs.Add(KeyEvent(modifiers[i], true, false)); }
            return inputs.ToArray();
        }

        private static Input KeyEvent(ushort key, bool up, bool extended) {
            Input input = new Input();
            input.Type = 1;
            input.Data.Keyboard.VirtualKey = key;
            input.Data.Keyboard.Flags = (up ? 2u : 0u) | (extended ? 1u : 0u);
            return input;
        }

        public static uint InsertKey(string command) {
            return InsertEvents(BuildKey(command));
        }

        private static uint InsertEvents(Input[] inputs) {
            if (Environment.OSVersion.Platform != PlatformID.Win32NT || IntPtr.Size != 8) {
                throw new PlatformNotSupportedException("guest-input-requires-win64");
            }
            int size = Marshal.SizeOf(typeof(Input));
            if (size != 40) { throw new InvalidOperationException("guest-input-layout"); }
            uint inserted = SendInput((uint)inputs.Length, inputs, size);
            RequireComplete(inserted, (uint)inputs.Length);
            return inserted;
        }
    }
}
