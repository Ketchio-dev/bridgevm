#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdint.h>

#define BV_MAGIC 0x42565431u
#define BV_TEXT_UNITS 65536u
#define BV_HEADER_BYTES 64u
#define BV_MAPPING_BYTES (BV_HEADER_BYTES + BV_TEXT_UNITS * 2u)

typedef struct {
    uint32_t magic, nonce, length, thread;
    uint64_t window;
    volatile LONG state, units;
    uint32_t error, reserved[7];
} BvTextHeader;
typedef char BvHeaderSizeMustBe64[(sizeof(BvTextHeader) == BV_HEADER_BYTES) ? 1 : -1];

static void object_name(WCHAR *out, uint32_t nonce, BOOL event) {
    const WCHAR prefix[] = L"Local\\BridgeVM.TextPacket.";
    unsigned at = 0;
    while (prefix[at]) { out[at] = prefix[at]; ++at; }
    const WCHAR digits[] = L"0123456789ABCDEF";
    for (int shift = 28; shift >= 0; shift -= 4) { out[at++] = digits[(nonce >> shift) & 15]; }
    if (event) { const WCHAR suffix[] = L".done"; unsigned i = 0; while (suffix[i]) { out[at++] = suffix[i++]; } }
    out[at] = 0;
}

static BOOL ascii_equal(const WCHAR *left, const WCHAR *right) {
    while (*left && *right) {
        WCHAR a = *left++, b = *right++;
        if (a >= L'A' && a <= L'Z') { a += L'a' - L'A'; }
        if (b >= L'A' && b <= L'Z') { b += L'a' - L'A'; }
        if (a != b) { return FALSE; }
    }
    return *left == *right;
}

static BOOL notepad_editor(HWND window) {
    WCHAR kind[64], path[512];
    if (!GetClassNameW(window, kind, 64) || !ascii_equal(kind, L"RichEditD2DPT")) { return FALSE; }
    DWORD path_length = GetModuleFileNameW(NULL, path, 512);
    if (!path_length || path_length >= 512) { return FALSE; }
    WCHAR *name = path;
    for (WCHAR *cursor = path; *cursor; ++cursor) { if (*cursor == L'\\' || *cursor == L'/') { name = cursor + 1; } }
    return ascii_equal(name, L"Notepad.exe") && !(GetWindowLongPtrW(window, GWL_STYLE) & ES_READONLY);
}

static void reject(BvTextHeader *header, uint32_t error) {
    header->error = error;
    InterlockedCompareExchange(&header->state, 3, 1);
}

// The marker is in the keyboard input queue, after earlier injected keys. Its
// callback runs on the focused editor thread, so sent character messages cannot
// overtake those earlier queued keys as an external SendMessage would.
__declspec(dllexport) LRESULT CALLBACK BridgeVMTextPacketHook(int code, WPARAM removed, LPARAM value) {
    if (code != HC_ACTION || removed != PM_REMOVE) { return CallNextHookEx(NULL, code, removed, value); }
    MSG *message = (MSG *)value;
    uint64_t extra = (uint64_t)(ULONG_PTR)GetMessageExtraInfo();
    if ((message->message != WM_KEYUP && message->message != WM_SYSKEYUP) || message->wParam != VK_F24 || (extra >> 32) != BV_MAGIC) {
        return CallNextHookEx(NULL, code, removed, value);
    }
    uint32_t nonce = (uint32_t)extra;
    WCHAR name[64]; object_name(name, nonce, FALSE);
    HANDLE mapping = OpenFileMappingW(FILE_MAP_READ | FILE_MAP_WRITE, FALSE, name);
    if (!mapping) { return CallNextHookEx(NULL, code, removed, value); }
    BvTextHeader *header = MapViewOfFile(mapping, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, BV_MAPPING_BYTES);
    if (!header) { CloseHandle(mapping); return CallNextHookEx(NULL, code, removed, value); }
    if (header->magic != BV_MAGIC || header->nonce != nonce || header->length == 0 || header->length > BV_TEXT_UNITS
        || header->thread != GetCurrentThreadId() || header->window != (uint64_t)(ULONG_PTR)message->hwnd) {
        UnmapViewOfFile(header); CloseHandle(mapping); return CallNextHookEx(NULL, code, removed, value);
    }
    HWND target = message->hwnd;
    message->message = WM_NULL; message->wParam = 0; message->lParam = 0;
    WCHAR *copy = NULL;
    if (InterlockedCompareExchange(&header->state, 1, 0) == 0) {
        uint32_t length = header->length;
        copy = HeapAlloc(GetProcessHeap(), 0, (length + 1) * sizeof(WCHAR));
        if (!copy) { reject(header, 1); }
        else {
            WCHAR *text = (WCHAR *)((BYTE *)header + BV_HEADER_BYTES);
            for (uint32_t i = 0; i < length; ++i) { copy[i] = text[i]; }
            copy[length] = 0;
            BOOL valid = TRUE, has_null = FALSE;
            for (uint32_t i = 0; i < length; ++i) {
                if (!copy[i]) { has_null = TRUE; }
                if (copy[i] >= 0xd800 && copy[i] <= 0xdbff) {
                    if (++i >= length || copy[i] < 0xdc00 || copy[i] > 0xdfff) { valid = FALSE; break; }
                } else if (copy[i] >= 0xdc00 && copy[i] <= 0xdfff) { valid = FALSE; break; }
            }
            if (!valid) { reject(header, 2); }
            else if (!IsWindowUnicode(target) || SendMessageW(target, WM_UNICHAR, UNICODE_NOCHAR, 0) != TRUE) {
                reject(header, 3);
            } else {
                DWORD owner = 0;
                GetWindowThreadProcessId(GetForegroundWindow(), &owner);
                if (GetFocus() != target || owner != GetCurrentProcessId()) { reject(header, 4); }
                else if (!has_null && notepad_editor(target) && header->state == 1) {
                    // Notepad's known plain-text RichEdit supports undoable selection
                    // replacement. One edit avoids per-character layout work.
                    SendMessageW(target, EM_REPLACESEL, TRUE, (LPARAM)copy);
                    InterlockedExchange(&header->units, (LONG)length);
                } else
                for (uint32_t i = 0; i < length;) {
                    DWORD foreground_process = 0;
                    GetWindowThreadProcessId(GetForegroundWindow(), &foreground_process);
                    if (header->state != 1) { break; }
                    if (GetFocus() != target || foreground_process != GetCurrentProcessId()) { reject(header, 4); break; }
                    uint32_t scalar = copy[i++];
                    if (scalar >= 0xd800 && scalar <= 0xdbff) { scalar = 0x10000 + ((scalar - 0xd800) << 10) + (copy[i++] - 0xdc00); }
                    SendMessageW(target, WM_UNICHAR, scalar, 1);
                    InterlockedExchange(&header->units, (LONG)i);
                }
                if (header->units == (LONG)length) { InterlockedCompareExchange(&header->state, 2, 1); }
            }
        }
    }
    if (copy) { HeapFree(GetProcessHeap(), 0, copy); }
    object_name(name, nonce, TRUE);
    HANDLE done = OpenEventW(EVENT_MODIFY_STATE, FALSE, name);
    if (done) { SetEvent(done); CloseHandle(done); }
    UnmapViewOfFile(header); CloseHandle(mapping);
    return CallNextHookEx(NULL, code, removed, value);
}

// Cancelling a marker that has not started is a positive no-dispatch result.
// Once dispatch began, a timeout has an unknown prefix and must never replay.
__declspec(dllexport) LONG WINAPI BridgeVMCancelTextPacket(BvTextHeader *header) {
    LONG before = InterlockedCompareExchange(&header->state, 4, 0);
    if (before == 1) { before = InterlockedCompareExchange(&header->state, 4, 1); }
    return before;
}
BOOL WINAPI DllMain(HINSTANCE module, DWORD reason, LPVOID reserved) {
    (void)module; (void)reason; (void)reserved; return TRUE;
}
