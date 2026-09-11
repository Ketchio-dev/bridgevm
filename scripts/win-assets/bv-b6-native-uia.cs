using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;

// Prefixes follow Microsoft's WinSDK UIAutomationClient.h vtable order.
// Reserved methods preserve slots and are never invoked. No UI actions are used.
namespace BridgeVmNativeUia {
    [ComImport, Guid("30cbe57d-d9d0-452a-ab13-7ac5ac4825ee"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface Automation {
        void Reserved01(); void Reserved02(); void Reserved03();
        void ElementFromHandle(IntPtr hwnd, out Element element);
        void Reserved05(); void Reserved06(); void Reserved07(); void Reserved08();
        void Reserved09(); void Reserved10(); void Reserved11(); void Reserved12();
        void Reserved13(); void Reserved14(); void Reserved15(); void Reserved16();
        void Reserved17(); void Reserved18(); void Reserved19(); void Reserved20();
        void CreatePropertyCondition(int property, [MarshalAs(UnmanagedType.Struct)] object value, out Condition condition);
        void Reserved22();
        void CreateAndCondition(Condition first, Condition second, out Condition condition);
    }
    [ComImport, Guid("d22108aa-8ac5-49a5-837b-37bbb3d7591e"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface Element {
        void Reserved01(); void Reserved02(); void Reserved03();
        void FindAll(int scope, Condition condition, out Elements elements);
        void Reserved05(); void Reserved06(); void Reserved07();
        void GetCurrentPropertyValue(int property, [MarshalAs(UnmanagedType.Struct)] out object value);
    }
    [ComImport, Guid("14314595-b4bc-4055-95f2-58f2e42c9855"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface Elements {
        void GetLength(out int length);
        void GetElement(int index, out Element element);
    }
    [ComImport, Guid("352ffba8-0973-437c-a61f-f64cafd81df9"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface Condition { }
    public sealed class Match {
        public int process_id;
        public bool is_enabled;
        public bool is_offscreen;
        public double[] bounding_rectangle;
    }
    public sealed class Observation {
        public string status = "started";
        public string stage = "activate";
        public int root_process_id;
        public int match_count;
        public bool truncated;
        public List<Match> matches = new List<Match>();
        public string exception_type;
        public int hresult;
    }
    public static class Probe {
        private static object Property(Element element, int id) {
            object value; element.GetCurrentPropertyValue(id, out value); return value;
        }
        public static Observation Query(long hwnd) {
            var result = new Observation();
            var owned = new List<object>();
            try {
                if (hwnd <= 0) throw new ArgumentOutOfRangeException("hwnd");
                var automation = (Automation)Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("ff48dba4-60ef-4201-aa87-54103eef594e")));
                owned.Add(automation);
                result.stage = "element-from-handle";
                Element root; automation.ElementFromHandle(new IntPtr(hwnd), out root); owned.Add(root);
                result.stage = "root-process-id";
                result.root_process_id = (int)Property(root, 30002);
                result.stage = "create-condition";
                Condition button, name, both;
                automation.CreatePropertyCondition(30003, 50000, out button); owned.Add(button);
                automation.CreatePropertyCondition(30005, "Got it", out name); owned.Add(name);
                automation.CreateAndCondition(button, name, out both); owned.Add(both);
                result.stage = "find-all";
                Elements matches; root.FindAll(4, both, out matches); owned.Add(matches);
                result.stage = "match-count";
                matches.GetLength(out result.match_count);
                if (result.match_count < 0) throw new InvalidOperationException("Negative match count");
                result.truncated = result.match_count > 16;
                result.stage = "match-properties";
                for (int i = 0; i < Math.Min(result.match_count, 16); i++) {
                    Element element; matches.GetElement(i, out element); owned.Add(element);
                    result.matches.Add(new Match {
                        process_id = (int)Property(element, 30002),
                        is_enabled = (bool)Property(element, 30010),
                        is_offscreen = (bool)Property(element, 30022),
                        bounding_rectangle = (double[])Property(element, 30001)
                    });
                }
                result.stage = "complete";
                result.status = "query-returned";
            } catch (Exception error) {
                result.status = "query-failed";
                result.exception_type = error.GetType().FullName;
                result.hresult = error.HResult;
            } finally {
                for (int i = owned.Count - 1; i >= 0; i--) {
                    if (owned[i] != null && Marshal.IsComObject(owned[i])) {
                        try { Marshal.ReleaseComObject(owned[i]); } catch (InvalidComObjectException) { }
                    }
                }
            }
            return result;
        }
    }
}
