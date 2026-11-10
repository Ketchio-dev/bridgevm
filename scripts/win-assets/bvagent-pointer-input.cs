using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text.RegularExpressions;
namespace BridgeVM {
    public static partial class BvUnicodeInput {
        public static Input[] BuildPointer(string command) {
            if (String.IsNullOrEmpty(command) || command.Length > 64) throw new ArgumentException("pointer-input-length");
            string[] fields = command.Split(':');
            if (fields.Length == 3) {
                // Modifiers held for this one action: w Win, c Ctrl, a Alt, s Shift, each at most once and in the
                // key builder's order. They go down before the pointer events and up after them in reverse, in
                // the same batch, so nothing interleaves and the guest never keeps one down.
                List<ushort> held = new List<ushort>();
                int next = 0;
                foreach (char name in fields[2]) {
                    int index = "wcas".IndexOf(name, next);
                    if (index < 0) throw new ArgumentException("pointer-input-modifiers");
                    held.Add(ModifierKeys[index]);
                    next = index + 1;
                }
                if (held.Count == 0) throw new ArgumentException("pointer-input-modifiers");
                List<Input> inputs = new List<Input>();
                foreach (ushort key in held) inputs.Add(KeyEvent(key, false, key == 0x5B));
                inputs.AddRange(BuildPointer(fields[0] + ":" + fields[1]));
                for (int i = held.Count - 1; i >= 0; i--) inputs.Add(KeyEvent(held[i], true, held[i] == 0x5B));
                return inputs.ToArray();
            }
            if (fields.Length != 2) throw new ArgumentException("pointer-input-format");
            if (fields[0] == "wheel" || fields[0] == "scroll") {
                bool scroll = fields[0] == "scroll";
                string[] parts = fields[1].Split('@');
                int ticks;
                if (parts.Length != (scroll ? 2 : 1) ||
                    !Regex.IsMatch(parts[0], @"\A-?[1-9][0-9]{0,2}\z") ||
                    !Int32.TryParse(parts[0], NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out ticks) ||
                    ticks < (scroll ? -128 : -127) || ticks > 127) throw new ArgumentException("pointer-input-wheel");
                Input wheel = Wheel(0x0800, ticks * 120);
                return scroll ? new Input[] { BuildPointer("move:" + parts[1])[0], wheel } : new Input[] { wheel };
            }
            if (fields[0] == "scrollunits") {
                // Raw signed wheel units (WHEEL_DELTA 120 = one notch): vertical > 0 scrolls up, horizontal > 0 right.
                Match units = Regex.Match(fields[1], @"\A(0|-?[1-9][0-9]{0,3}),(0|-?[1-9][0-9]{0,3})@([0-9]{1,5}x[0-9]{1,5})\z");
                int vertical, horizontal;
                if (!units.Success ||
                    !Int32.TryParse(units.Groups[1].Value, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out vertical) ||
                    !Int32.TryParse(units.Groups[2].Value, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out horizontal) ||
                    vertical < -1200 || vertical > 1200 || horizontal < -1200 || horizontal > 1200 ||
                    (vertical == 0 && horizontal == 0)) throw new ArgumentException("pointer-input-scroll-units");
                Input move = BuildPointer("move:" + units.Groups[3].Value)[0];
                if (horizontal == 0) return new Input[] { move, Wheel(0x0800, vertical) };
                if (vertical == 0) return new Input[] { move, Wheel(0x1000, horizontal) };
                return new Input[] { move, Wheel(0x0800, vertical), Wheel(0x1000, horizontal) };
            }
            string[] point = fields[1].Split('x');
            int x, y;
            if (point.Length != 2 || !Coordinate(point[0], out x) || !Coordinate(point[1], out y))
                throw new ArgumentException("pointer-input-coordinate");
            uint down, up;
            switch (fields[0]) {
                case "move": return new Input[] { MouseEvent(x, y, 0x8001) };
                case "press": return new Input[] { MouseEvent(x, y, 0x8003) };
                case "release": return new Input[] { MouseEvent(x, y, 0x8005) };
                // Every button this builder presses: left, right and middle up.
                case "releaseall": return new Input[] { MouseEvent(x, y, 0x8055) };
                case "rightpress": return new Input[] { MouseEvent(x, y, 0x8009) };
                case "rightrelease": return new Input[] { MouseEvent(x, y, 0x8011) };
                case "middlepress": return new Input[] { MouseEvent(x, y, 0x8021) };
                case "middlerelease": return new Input[] { MouseEvent(x, y, 0x8041) };
                case "click": down = 0x8003; up = 0x8005; break;
                case "rightclick": down = 0x8009; up = 0x8011; break;
                case "middleclick": down = 0x8021; up = 0x8041; break;
                default: throw new ArgumentException("unsupported-pointer-input");
            }
            return new Input[] { MouseEvent(x, y, down), MouseEvent(x, y, up) };
        }
        private static bool Coordinate(string value, out int normalized) {
            normalized = 0;
            int coordinate;
            if (!Regex.IsMatch(value, @"\A(?:0|[1-9][0-9]{0,4})\z") ||
                !Int32.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out coordinate) ||
                coordinate > 32767) return false;
            normalized = (int)(((long)coordinate * 65535 + 16383) / 32767);
            return true;
        }
        public static uint InsertPointer(string command) { return InsertEvents(BuildPointer(command)); }
        private static Input Wheel(uint flags, int units) {
            Input wheel = MouseEvent(0, 0, flags);
            wheel.Data.Mouse.MouseData = unchecked((uint)units);
            return wheel;
        }
        private static Input MouseEvent(int x, int y, uint flags) {
            Input input = new Input();
            input.Type = 0;
            input.Data.Mouse.X = x; input.Data.Mouse.Y = y;
            input.Data.Mouse.Flags = flags;
            return input;
        }
    }
}
