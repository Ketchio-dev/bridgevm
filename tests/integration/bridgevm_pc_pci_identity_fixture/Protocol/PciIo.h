#ifndef FIXTURE_PCI_IO_H
#define FIXTURE_PCI_IO_H
#include <Uefi.h>
#define EfiPciIoWidthUint32 2
typedef struct EFI_PCI_IO_PROTOCOL EFI_PCI_IO_PROTOCOL;
struct EFI_PCI_IO_PROTOCOL {
  EFI_STATUS (*GetLocation)(EFI_PCI_IO_PROTOCOL *, UINTN *, UINTN *, UINTN *, UINTN *);
  struct {
    EFI_STATUS (*Read)(EFI_PCI_IO_PROTOCOL *, int, UINT32, UINTN, VOID *);
  } Pci;
};
#endif
