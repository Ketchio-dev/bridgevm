namespace BridgeVM {
    using System;
    using System.Collections.Generic;
    using System.Runtime.InteropServices;

    public static partial class BvUnicodeInput {
        // One generic chord: win, ctrl, alt and shift, each at most once and in this order, then
        // one key; or a single modifier alone (a bare tap, such as the Windows key opening Start).
        private static readonly string[] ModifierNames = { "win", "ctrl", "alt", "shift" };
        private static readonly ushort[] ModifierKeys = { 0x5B, 0x11, 0x12, 0x10 };
        private static readonly Dictionary<string, ushort> NamedKeys = new Dictionary<string, ushort>(StringComparer.Ordinal) {
            { "backspace", 0x08 }, { "tab", 0x09 }, { "enter", 0x0D },
            { "esc", 0x1B }, { "escape", 0x1B }, { "space", 0x20 },
            { "pageup", 0x21 }, { "pagedown", 0x22 }, { "end", 0x23 }, { "home", 0x24 },
            { "left", 0x25 }, { "up", 0x26 }, { "right", 0x27 }, { "down", 0x28 },
            { "printscreen", 0x2C }, { "insert", 0x2D }, { "delete", 0x2E },
            { "f1", 0x70 }, { "f2", 0x71 }, { "f3", 0x72 }, { "f4", 0x73 },
            { "f5", 0x74 }, { "f6", 0x75 }, { "f7", 0x76 }, { "f8", 0x77 },
            { "f9", 0x78 }, { "f10", 0x79 }, { "f11", 0x7A }, { "f12", 0x7B }
        };
        // US-layout punctuation keys by name (VK_OEM_*).
        private static readonly Dictionary<string, ushort> PunctuationKeys = new Dictionary<string, ushort>(StringComparer.Ordinal) {
            { "minus", 0xBD }, { "equal", 0xBB }, { "lbracket", 0xDB }, { "rbracket", 0xDD }, { "backslash", 0xDC },
            { "semicolon", 0xBA }, { "quote", 0xDE }, { "grave", 0xC0 }, { "comma", 0xBC }, { "period", 0xBE }, { "slash", 0xBF }
        };

        // The actions `down:` halves hold now, this agent instance's (a restarted one holds nothing), at most six
        // like the host's. As in the VMM's HID report, a modifier stays down while a held action still needs it.
        private static readonly List<string> HeldActions = new List<string>();

        // Inserts the events and returns how many Windows took: SendInput (InsertEvents) or a contract test's stand-in.
        public delegate uint Sender(Input[] inputs);

        // `down:<key>` is the first half of a key's events (modifiers and key down) and `up:<key>` the second
        // (key and modifiers up, in reverse): a key held from the Mac's key-down to its key-up.
        public static Input[] BuildKey(string command) {
            return BuildKey(command, null);
        }

        // `held`: the actions held now. An `up:` presses a modifier another of them still needs again in place of its
        // release (a repeat, as a keyboard's typematic sends): up:ctrl+left leaves Ctrl down for a held ctrl+right, and
        // every half keeps the event count the host's receipt checks. A plain tap is the same whatever is held.
        private static Input[] BuildKey(string command, List<string> held) {
            int phase = 0;
            if (command != null && command.StartsWith("down:", StringComparison.Ordinal)) { phase = 1; command = command.Substring(5); }
            else if (command != null && command.StartsWith("up:", StringComparison.Ordinal)) { phase = 2; command = command.Substring(3); }
            List<ushort> modifiers;
            bool extended;
            ushort code = ParseKey(command, out modifiers, out extended);
            List<ushort> kept = phase == 2 ? HeldModifiers(held, command) : new List<ushort>();
            List<Input> inputs = new List<Input>();
            if (phase != 2) {
                foreach (ushort modifier in modifiers) { inputs.Add(KeyEvent(modifier, false, modifier == 0x5B)); }
                inputs.Add(KeyEvent(code, false, extended));
            }
            if (phase != 1) {
                inputs.Add(KeyEvent(code, !kept.Contains(code), extended));
                for (int i = modifiers.Count - 1; i >= 0; i--) { inputs.Add(KeyEvent(modifiers[i], !kept.Contains(modifiers[i]), modifiers[i] == 0x5B)); }
            }
            return inputs.ToArray();
        }

        // The modifier keys the held `actions` other than `except` need down (none for null).
        private static List<ushort> HeldModifiers(List<string> actions, string except) {
            List<ushort> kept = new List<ushort>();
            if (actions == null) { return kept; }
            foreach (string other in actions) {
                if (String.Equals(other, except, StringComparison.Ordinal)) { continue; }
                List<ushort> its;
                bool ignored;
                ushort key = ParseKey(other, out its, out ignored);
                kept.AddRange(its);
                if (Array.IndexOf(ModifierKeys, key) >= 0) { kept.Add(key); }
            }
            return kept;
        }

        // One generic chord's modifiers and key; anything else throws.
        private static ushort ParseKey(string command, out List<ushort> modifiers, out bool extended) {
            if (String.IsNullOrEmpty(command) || command.Length > 32) {
                throw new ArgumentException("key-input-length");
            }
            string[] parts = command.Split('+');
            modifiers = new List<ushort>();
            int next = 0;
            for (int i = 0; i < parts.Length - 1; i++) {
                int index = next < ModifierNames.Length ? Array.IndexOf(ModifierNames, parts[i], next) : -1;
                if (index < 0) { throw new ArgumentException("unsupported-key-input"); }
                modifiers.Add(ModifierKeys[index]);
                next = index + 1;
            }
            string key = parts[parts.Length - 1];
            int bare = parts.Length == 1 ? Array.IndexOf(ModifierNames, key) : -1;
            ushort code;
            if (bare >= 0) {
                code = ModifierKeys[bare];
            } else if (!NamedKeys.TryGetValue(key, out code)) {
                // A character key alone or with only Shift is text, which has its own path.
                bool chord = modifiers.Exists(delegate(ushort modifier) { return modifier != 0x10; });
                bool character = key.Length == 1 && (key[0] >= 'a' && key[0] <= 'z' || key[0] >= '0' && key[0] <= '9');
                if (character) { code = (ushort)Char.ToUpperInvariant(key[0]); }
                if (!chord || !character && !PunctuationKeys.TryGetValue(key, out code)) {
                    throw new ArgumentException("unsupported-key-input");
                }
            }
            // SendInput cannot raise the secure attention sequence; never type its keys into an application instead.
            if (code == 0x2E && modifiers.Contains(0x11) && modifiers.Contains(0x12)) {
                throw new ArgumentException("unsupported-key-input");
            }
            // Like the physical keys: the navigation block, Print Screen (E0 37), Insert, Delete and the Windows key.
            extended = (code >= 0x21 && code <= 0x28) || code == 0x2C || code == 0x2D || code == 0x2E || code == 0x5B;
            return code;
        }

        private static Input KeyEvent(ushort key, bool up, bool extended) {
            Input input = new Input();
            input.Type = 1;
            input.Data.Keyboard.VirtualKey = key;
            input.Data.Keyboard.Flags = (up ? 2u : 0u) | (extended ? 1u : 0u);
            return input;
        }

        public static uint InsertKey(string command) {
            return InsertKey(command, InsertEvents);
        }

        // A `down:` holds its action once Windows
        // took all of its events, a repeat holding it once; an `up:` forgets it whatever it inserts. An `up:` for an
        // action not held still releases it, but for what a held action needs: a restarted agent gets the releases of
        // the keys its predecessor pressed, which Windows still holds down.
        public static uint InsertKey(string command, Sender send) {
            Input[] inputs = BuildKey(command, HeldActions);
            if (command.StartsWith("up:", StringComparison.Ordinal)) { HeldActions.Remove(command.Substring(3)); }
            uint inserted = send(inputs);
            RequireComplete(inserted, (uint)inputs.Length);
            string action = command.StartsWith("down:", StringComparison.Ordinal) ? command.Substring(5) : null;
            if (action != null && !HeldActions.Contains(action) && HeldActions.Count < 6) { HeldActions.Add(action); }
            return inserted;
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
