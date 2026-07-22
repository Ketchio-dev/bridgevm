#ifndef UNICODE
#define UNICODE
#endif
#ifndef _UNICODE
#define _UNICODE
#endif
#include <windows.h>

static const wchar_t *SERVICE_NAME = L"BridgeVMAgentInstaller";
static SERVICE_STATUS_HANDLE g_status_handle;
static SERVICE_STATUS g_status;

static void report_status(DWORD state, DWORD win32_exit) {
    ZeroMemory(&g_status, sizeof(g_status));
    g_status.dwServiceType = SERVICE_WIN32_OWN_PROCESS;
    g_status.dwCurrentState = state;
    g_status.dwWin32ExitCode = win32_exit;
    g_status.dwControlsAccepted = state == SERVICE_RUNNING ? SERVICE_ACCEPT_STOP : 0;
    SetServiceStatus(g_status_handle, &g_status);
}

static void append_log(const char *message) {
    HANDLE file = CreateFileW(
        L"C:\\BridgeVM\\bvagent-install-service.log",
        FILE_APPEND_DATA,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        NULL,
        OPEN_ALWAYS,
        FILE_ATTRIBUTE_NORMAL,
        NULL);
    if (file == INVALID_HANDLE_VALUE) return;
    DWORD written = 0;
    WriteFile(file, message, (DWORD)lstrlenA(message), &written, NULL);
    WriteFile(file, "\r\n", 2, &written, NULL);
    FlushFileBuffers(file);
    CloseHandle(file);
}

static DWORD run_and_wait(wchar_t *command_line) {
    STARTUPINFOW startup;
    PROCESS_INFORMATION process;
    ZeroMemory(&startup, sizeof(startup));
    ZeroMemory(&process, sizeof(process));
    startup.cb = sizeof(startup);
    if (!CreateProcessW(
            NULL, command_line, NULL, NULL, FALSE, CREATE_NO_WINDOW,
            NULL, L"C:\\BridgeVM", &startup, &process)) {
        return GetLastError();
    }
    DWORD wait = WaitForSingleObject(process.hProcess, 60000);
    DWORD exit_code = ERROR_TIMEOUT;
    if (wait == WAIT_OBJECT_0 && !GetExitCodeProcess(process.hProcess, &exit_code)) {
        exit_code = GetLastError();
    }
    CloseHandle(process.hThread);
    CloseHandle(process.hProcess);
    return exit_code;
}

static void WINAPI service_control(DWORD control) {
    if (control == SERVICE_CONTROL_STOP) report_status(SERVICE_STOPPED, ERROR_CANCELLED);
}

static void WINAPI service_main(DWORD argc, wchar_t **argv) {
    (void)argc;
    (void)argv;
    g_status_handle = RegisterServiceCtrlHandlerW(SERVICE_NAME, service_control);
    if (!g_status_handle) return;
    report_status(SERVICE_START_PENDING, NO_ERROR);
    report_status(SERVICE_RUNNING, NO_ERROR);
    append_log("installer-service-started");

    wchar_t create_task[] =
        L"C:\\Windows\\System32\\schtasks.exe /Create /F "
        L"/TN \"BridgeVM Guest Agent\" /SC ONLOGON /RU bridge /RL HIGHEST /IT "
        L"/TR \"powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\\bvagent.ps1\"";
    DWORD result = ERROR_SERVICE_NOT_ACTIVE;
    for (int attempt = 0; attempt < 12 && result != ERROR_SUCCESS; ++attempt) {
        result = run_and_wait(create_task);
        if (result != ERROR_SUCCESS) Sleep(5000);
    }
    if (result == ERROR_SUCCESS) {
        append_log("scheduled-task-created");
        wchar_t run_task[] =
            L"C:\\Windows\\System32\\schtasks.exe /Run /TN \"BridgeVM Guest Agent\"";
        DWORD run_result = run_and_wait(run_task);
        if (run_result == ERROR_SUCCESS) append_log("scheduled-task-start-requested");
        wchar_t delete_service[] =
            L"C:\\Windows\\System32\\sc.exe delete BridgeVMAgentInstaller";
        DWORD delete_result = run_and_wait(delete_service);
        if (delete_result == ERROR_SUCCESS) append_log("installer-service-delete-requested");
    } else {
        append_log("scheduled-task-create-failed");
    }
    report_status(SERVICE_STOPPED, result);
}

int wmain(void) {
    SERVICE_TABLE_ENTRYW table[] = {
        {(wchar_t *)SERVICE_NAME, service_main},
        {NULL, NULL},
    };
    if (!StartServiceCtrlDispatcherW(table)) return (int)GetLastError();
    return 0;
}
