#ifndef FIXTURE_UEFI_H
#define FIXTURE_UEFI_H
#include <stddef.h>
#include <stdint.h>
#define IN
#define OUT
#define STATIC static
#define CONST const
typedef void VOID;
typedef uint32_t UINT32;
typedef uint64_t UINT64;
typedef uintptr_t UINTN;
typedef UINT64 EFI_STATUS;
typedef void *EFI_HANDLE;
typedef struct { UINT32 Data1; } EFI_GUID;
#define EFI_SUCCESS 0
#define EFI_COMPROMISED_DATA UINT64_C(0x8000000000000021)
#define EFI_DEVICE_ERROR UINT64_C(0x8000000000000007)
#define EFI_ERROR(Status) (((Status) >> 63) != 0)
#define ByProtocol 2
extern EFI_GUID gEfiPciIoProtocolGuid;
typedef struct {
  EFI_STATUS (*LocateHandleBuffer)(int, EFI_GUID *, VOID *, UINTN *, EFI_HANDLE **);
  EFI_STATUS (*HandleProtocol)(EFI_HANDLE, EFI_GUID *, VOID **);
  EFI_STATUS (*FreePool)(VOID *);
} EFI_BOOT_SERVICES;
typedef struct { EFI_BOOT_SERVICES *BootServices; } EFI_SYSTEM_TABLE;
#endif
