using System;
using System.Globalization;
using System.Text.RegularExpressions;
namespace BridgeVM {
    public static partial class BvUnicodeInput {
        public static Input[] BuildPointer(string command) {
            if (String.IsNullOrEmpty(command) || command.Length > 64) throw new ArgumentException("pointer-input-length");
            string[] fields = command.Split(':');
            if (fields.Length != 2) throw new ArgumentException("pointer-input-format");
            if (fields[0] == "wheel" || fields[0] == "scroll") {
                bool scroll = fields[0] == "scroll";
                string[] parts = fields[1].Split('@');
                int ticks;
                if (parts.Length != (scroll ? 2 : 1) ||
                    !Regex.IsMatch(parts[0], @"\A-?[1-9][0-9]{0,2}\z") ||
                    !Int32.TryParse(parts[0], NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out ticks) ||
                    ticks < (scroll ? -128 : -127) || ticks > 127) throw new ArgumentException("pointer-input-wheel");
                Input wheel = MouseEvent(0, 0, 0x0800);
                wheel.Data.Mouse.MouseData = unchecked((uint)(ticks * 120));
                return scroll ? new Input[] { BuildPointer("move:" + parts[1])[0], wheel } : new Input[] { wheel };
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
                case "rightpress": return new Input[] { MouseEvent(x, y, 0x8009) };
                case "rightrelease": return new Input[] { MouseEvent(x, y, 0x8011) };
                case "click": down = 0x8003; up = 0x8005; break;
                case "rightclick": down = 0x8009; up = 0x8011; break;
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
        private static Input MouseEvent(int x, int y, uint flags) {
            Input input = new Input();
            input.Type = 0;
            input.Data.Mouse.X = x; input.Data.Mouse.Y = y;
            input.Data.Mouse.Flags = flags;
            return input;
        }
    }
}
