#define UNICODE
#define _UNICODE
#include <stdio.h>
#include <windows.h>
#include <userenv.h>
#include <wtsapi32.h>

#define SERVICE_NAME L"BridgeVMGpuDiagnosticsProbe6"

static SERVICE_STATUS_HANDLE g_status_handle;
static SERVICE_STATUS g_status;

static void report_status(DWORD state, DWORD error, DWORD wait_hint) {
    static DWORD checkpoint = 1;
    ZeroMemory(&g_status, sizeof(g_status));
    g_status.dwServiceType = SERVICE_WIN32_OWN_PROCESS;
    g_status.dwCurrentState = state;
    g_status.dwWin32ExitCode = error;
    g_status.dwWaitHint = wait_hint;
    g_status.dwControlsAccepted =
        state == SERVICE_RUNNING ? SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN : 0;
    g_status.dwCheckPoint =
        (state == SERVICE_START_PENDING || state == SERVICE_STOP_PENDING) ? checkpoint++ : 0;
    SetServiceStatus(g_status_handle, &g_status);
}

static void WINAPI service_control(DWORD control) {
    if (control == SERVICE_CONTROL_STOP || control == SERVICE_CONTROL_SHUTDOWN) {
        report_status(SERVICE_STOP_PENDING, NO_ERROR, 5000);
    } else if (control == SERVICE_CONTROL_INTERROGATE) {
        SetServiceStatus(g_status_handle, &g_status);
    }
}

static void append_entry_marker(const char *marker, DWORD marker_size) {
    DWORD written = 0;
    HANDLE file = CreateFileW(
        L"C:\\BridgeVM\\bvgpu-native-service-entry.log",
        GENERIC_WRITE,
        FILE_SHARE_READ,
        NULL,
        OPEN_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        NULL);
    if (file == INVALID_HANDLE_VALUE) {
        return;
    }
    SetFilePointer(file, 0, NULL, FILE_END);
    WriteFile(file, marker, marker_size, &written, NULL);
    FlushFileBuffers(file);
    CloseHandle(file);
}

static void append_session_marker(
    const char *prefix,
    DWORD session_id,
    WTS_CONNECTSTATE_CLASS state,
    DWORD error) {
    char marker[160];
    int length = snprintf(
        marker,
        sizeof(marker),
        "%s session=%lu state=%d error=%lu\r\n",
        prefix,
        (unsigned long)session_id,
        (int)state,
        (unsigned long)error);
    if (length > 0) {
        append_entry_marker(marker, (DWORD)length);
    }
}

static BOOL session_has_user(DWORD session_id) {
    LPWSTR user_name = NULL;
    DWORD bytes = 0;
    BOOL have_user = FALSE;
    if (WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            session_id,
            WTSUserName,
            &user_name,
            &bytes)) {
        have_user = user_name != NULL && bytes > sizeof(WCHAR) && user_name[0] != L'\0';
    }
    if (user_name != NULL) {
        WTSFreeMemory(user_name);
    }
    return have_user;
}

static HANDLE find_logged_on_user_token(BOOL log_scan) {
    PWTS_SESSION_INFOW sessions = NULL;
    DWORD session_count = 0;
    if (!WTSEnumerateSessionsW(
            WTS_CURRENT_SERVER_HANDLE,
            0,
            1,
            &sessions,
            &session_count)) {
        if (log_scan) {
            append_session_marker(
                "session-enumeration-failed",
                0xffffffff,
                WTSInit,
                GetLastError());
        }
        return NULL;
    }

    HANDLE selected_token = NULL;
    if (log_scan) {
        for (DWORD i = 0; i < session_count; i++) {
            append_session_marker(
                session_has_user(sessions[i].SessionId)
                    ? "session-observed-user"
                    : "session-observed-no-user",
                sessions[i].SessionId,
                sessions[i].State,
                ERROR_SUCCESS);
        }
    }
    /* Prefer a logged-on Active session.  Headless Hyper-V/virtio display
     * guests do not necessarily report it as the physical console session. */
    for (DWORD pass = 0; pass < 2 && selected_token == NULL; pass++) {
        for (DWORD i = 0; i < session_count; i++) {
            WTS_SESSION_INFOW *session = &sessions[i];
            if (session->SessionId == 0 || !session_has_user(session->SessionId)) {
                continue;
            }
            if (pass == 0 && session->State != WTSActive) {
                continue;
            }
            DWORD error = ERROR_SUCCESS;
            if (!WTSQueryUserToken(session->SessionId, &selected_token)) {
                error = GetLastError();
            }
            if (log_scan || selected_token != NULL) {
                append_session_marker(
                    selected_token != NULL ? "session-token-selected" : "session-token-failed",
                    session->SessionId,
                    session->State,
                    error);
            }
            if (selected_token != NULL) {
                break;
            }
        }
    }
    WTSFreeMemory(sessions);
    return selected_token;
}

static void WINAPI service_main(DWORD argc, LPWSTR *argv) {
    (void)argc;
    (void)argv;
    g_status_handle = RegisterServiceCtrlHandlerW(SERVICE_NAME, service_control);
    if (g_status_handle == NULL) {
        return;
    }

    report_status(SERVICE_START_PENDING, NO_ERROR, 10000);
    static const char started[] = "native-service-started\r\n";
    append_entry_marker(started, (DWORD)(sizeof(started) - 1));

    /* Mesa's D3DKMT renderer opens the primary adapter through GetDC(NULL).
     * A Session-0 process has no interactive primary HDC.  Do not rely on
     * WTSGetActiveConsoleSessionId here: headless virtio display sessions can
     * be logged on without being identified as the physical console. */
    report_status(SERVICE_RUNNING, NO_ERROR, 0);
    BOOL firstboot_pending =
        GetFileAttributesW(L"C:\\BridgeVM\\viogpu3d-firstboot-pending.flag") !=
        INVALID_FILE_ATTRIBUTES;
    HANDLE user_token = NULL;
    if (firstboot_pending) {
        static const char firstboot_session0[] = "firstboot-pending-session0\r\n";
        append_entry_marker(firstboot_session0, (DWORD)(sizeof(firstboot_session0) - 1));
    } else {
        for (DWORD attempt = 0; attempt < 30; attempt++) {
            BOOL log_scan = attempt == 0 || attempt == 10 || attempt == 29;
            user_token = find_logged_on_user_token(log_scan);
            if (user_token != NULL) {
                break;
            }
            Sleep(1000);
        }
    }
    BOOL session_zero_fallback = firstboot_pending || user_token == NULL;
    if (!firstboot_pending && user_token == NULL) {
        static const char fallback[] = "logged-on-user-token-unavailable-session0-fallback\r\n";
        append_entry_marker(fallback, (DWORD)(sizeof(fallback) - 1));
    }

    WCHAR command_line[] =
        L"C:\\Windows\\System32\\cmd.exe /d /c call C:\\BridgeVM\\bvgpu-diagnostics-run.cmd";
    STARTUPINFOW startup;
    PROCESS_INFORMATION process;
    ZeroMemory(&startup, sizeof(startup));
    ZeroMemory(&process, sizeof(process));
    startup.cb = sizeof(startup);

    LPVOID environment = NULL;
    BOOL have_environment = !session_zero_fallback &&
                            CreateEnvironmentBlock(&environment, user_token, FALSE);
    DWORD creation_flags = CREATE_NO_WINDOW;
    if (have_environment) {
        creation_flags |= CREATE_UNICODE_ENVIRONMENT;
    }
    BOOL process_created;
    if (session_zero_fallback) {
        process_created = CreateProcessW(
            NULL,
            command_line,
            NULL,
            NULL,
            FALSE,
            creation_flags,
            NULL,
            L"C:\\BridgeVM",
            &startup,
            &process);
    } else {
        process_created = CreateProcessAsUserW(
            user_token,
            NULL,
            command_line,
            NULL,
            NULL,
            FALSE,
            creation_flags,
            have_environment ? environment : NULL,
            L"C:\\BridgeVM",
            &startup,
            &process);
    }
    if (!process_created) {
        DWORD error = GetLastError();
        static const char launch_failed[] = "diagnostics-child-create-failed\r\n";
        append_entry_marker(launch_failed, (DWORD)(sizeof(launch_failed) - 1));
        if (have_environment) {
            DestroyEnvironmentBlock(environment);
        }
        if (user_token != NULL) {
            CloseHandle(user_token);
        }
        report_status(SERVICE_STOPPED, error, 0);
        return;
    }

    static const char interactive_child_started[] = "interactive-child-started\r\n";
    static const char session0_child_started[] = "session0-child-started\r\n";
    append_entry_marker(
        session_zero_fallback ? session0_child_started : interactive_child_started,
        session_zero_fallback
            ? (DWORD)(sizeof(session0_child_started) - 1)
            : (DWORD)(sizeof(interactive_child_started) - 1));
    if (have_environment) {
        DestroyEnvironmentBlock(environment);
    }
    if (user_token != NULL) {
        CloseHandle(user_token);
    }

    WaitForSingleObject(process.hProcess, INFINITE);
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    report_status(SERVICE_STOPPED, NO_ERROR, 0);
}

int main(void) {
    SERVICE_TABLE_ENTRYW table[] = {
        {(LPWSTR)SERVICE_NAME, service_main},
        {NULL, NULL},
    };
    if (!StartServiceCtrlDispatcherW(table)) {
        return (int)GetLastError();
    }
    return 0;
}
