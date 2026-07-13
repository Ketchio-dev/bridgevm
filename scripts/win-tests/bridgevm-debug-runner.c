#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void print_debug_string(HANDLE process, const OUTPUT_DEBUG_STRING_INFO *info) {
  SIZE_T got = 0;
  SIZE_T bytes = info->nDebugStringLength;
  if (info->fUnicode) bytes *= sizeof(WCHAR);
  if (bytes > 8190) bytes = 8190;
  unsigned char buffer[8192] = {0};
  if (!ReadProcessMemory(process, info->lpDebugStringData, buffer, bytes, &got))
    return;
  if (info->fUnicode) {
    WCHAR *wide = (WCHAR *)buffer;
    int chars = (int)(got / sizeof(WCHAR));
    char utf8[8192];
    int written = WideCharToMultiByte(CP_UTF8, 0, wide, chars, utf8,
                                      (int)sizeof(utf8) - 1, NULL, NULL);
    if (written > 0) {
      utf8[written] = 0;
      printf("BV-DEBUG %s", utf8);
      if (utf8[written - 1] != '\n') putchar('\n');
    }
  } else {
    buffer[got < sizeof(buffer) ? got : sizeof(buffer) - 1] = 0;
    printf("BV-DEBUG %s", (char *)buffer);
    if (got && buffer[got - 1] != '\n') putchar('\n');
  }
  fflush(stdout);
}

int main(int argc, char **argv) {
  if (argc != 2) {
    fputs("usage: bridgevm-debug-runner.exe PROGRAM\n", stderr);
    return 2;
  }
  STARTUPINFOA si = {0};
  PROCESS_INFORMATION pi = {0};
  si.cb = sizeof(si);
  size_t command_bytes = strlen(argv[1]) + 3;
  char *command = malloc(command_bytes);
  if (!command) return 2;
  snprintf(command, command_bytes, "\"%s\"", argv[1]);
  if (!CreateProcessA(NULL, command, NULL, NULL, FALSE,
                      DEBUG_ONLY_THIS_PROCESS, NULL, NULL, &si, &pi)) {
    printf("BV-DEBUG-RUNNER-FAIL create_process=%lu\n", GetLastError());
    free(command);
    return 2;
  }
  free(command);
  DWORD child_exit = 2;
  int running = 1;
  while (running) {
    DEBUG_EVENT event;
    if (!WaitForDebugEvent(&event, INFINITE)) {
      printf("BV-DEBUG-RUNNER-FAIL wait=%lu\n", GetLastError());
      break;
    }
    switch (event.dwDebugEventCode) {
      case CREATE_PROCESS_DEBUG_EVENT:
        if (event.u.CreateProcessInfo.hFile)
          CloseHandle(event.u.CreateProcessInfo.hFile);
        break;
      case CREATE_THREAD_DEBUG_EVENT:
        if (event.u.CreateThread.hThread) CloseHandle(event.u.CreateThread.hThread);
        break;
      case LOAD_DLL_DEBUG_EVENT:
        if (event.u.LoadDll.hFile) CloseHandle(event.u.LoadDll.hFile);
        break;
      case OUTPUT_DEBUG_STRING_EVENT:
        print_debug_string(pi.hProcess, &event.u.DebugString);
        break;
      case EXIT_PROCESS_DEBUG_EVENT:
        child_exit = event.u.ExitProcess.dwExitCode;
        running = 0;
        break;
    }
    ContinueDebugEvent(event.dwProcessId, event.dwThreadId, DBG_CONTINUE);
  }
  WaitForSingleObject(pi.hProcess, 5000);
  CloseHandle(pi.hThread);
  CloseHandle(pi.hProcess);
  printf("BV-DEBUG-RUNNER-END exit=%lu\n", child_exit);
  return (int)child_exit;
}
