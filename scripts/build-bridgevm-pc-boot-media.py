#!/usr/bin/env python3
"""Build a deterministic GPT/FAT16 ESP containing EFI/BOOT/BOOTAA64.EFI."""
import binascii
import hashlib
import pathlib
import struct
import sys
import uuid

SECTOR = 512
SECTORS = 131072
PARTITION_START = 2048
PARTITION_END = SECTORS - 34
SECTORS_PER_CLUSTER = 4
DISK_GUID = uuid.UUID("6ee4aa36-4e86-4e27-a4ea-364aed39205f")
PARTITION_GUID = uuid.UUID("34da5450-76c0-4c02-a6bb-a8920a893f59")
ESP_GUID = uuid.UUID("c12a7328-f81f-11d2-ba4b-00a0c93ec93b")


def guid_bytes(value: uuid.UUID) -> bytes:
    return value.bytes_le


def directory_entry(name: bytes, attributes: int, cluster: int, size: int) -> bytes:
    if len(name) != 11:
        raise ValueError("FAT name must be exactly eleven bytes")
    date = ((2026 - 1980) << 9) | (5 << 5) | 7
    time = (13 << 11) | (30 << 5)
    entry = bytearray(32)
    entry[:11] = name
    entry[11] = attributes
    struct.pack_into("<HHH", entry, 14, time, date, date)
    struct.pack_into("<H", entry, 22, time)
    struct.pack_into("<H", entry, 24, date)
    struct.pack_into("<H", entry, 26, cluster)
    struct.pack_into("<I", entry, 28, size)
    return bytes(entry)


def gpt_header(current: int, backup: int, entries_lba: int, entries_crc: int) -> bytes:
    header = bytearray(SECTOR)
    header[:8] = b"EFI PART"
    struct.pack_into("<I", header, 8, 0x00010000)
    struct.pack_into("<I", header, 12, 92)
    struct.pack_into("<QQQQ", header, 24, current, backup, 34, SECTORS - 34)
    header[56:72] = guid_bytes(DISK_GUID)
    struct.pack_into("<QIII", header, 72, entries_lba, 128, 128, entries_crc)
    struct.pack_into("<I", header, 16, binascii.crc32(header[:92]) & 0xFFFFFFFF)
    return bytes(header)


def fat_geometry(partition_sectors: int) -> tuple[int, int, int]:
    root_sectors = 32
    fat_sectors = 1
    while True:
        clusters = (partition_sectors - 1 - root_sectors - 2 * fat_sectors) // SECTORS_PER_CLUSTER
        wanted = (2 * (clusters + 2) + SECTOR - 1) // SECTOR
        if wanted == fat_sectors:
            return fat_sectors, root_sectors, clusters
        fat_sectors = wanted


def build_fat(image: bytearray, application: bytes) -> None:
    partition_sectors = PARTITION_END - PARTITION_START + 1
    fat_sectors, root_sectors, cluster_count = fat_geometry(partition_sectors)
    if not 4085 <= cluster_count < 65525:
        raise ValueError("image geometry is not FAT16")
    boot = bytearray(SECTOR)
    boot[:11] = b"\xeb\x3c\x90BRIDGEVM"
    struct.pack_into("<HBHBHHBHHHII", boot, 11, SECTOR, SECTORS_PER_CLUSTER, 1, 2,
                     512, 0, 0xF8, fat_sectors, 63, 255, PARTITION_START, partition_sectors)
    boot[36:62] = b"\x80\x00\x29" + struct.pack("<I", 0x42564D50) + b"BRIDGEVMPC " + b"FAT16   "
    boot[510:512] = b"\x55\xaa"
    partition = PARTITION_START * SECTOR
    image[partition:partition + SECTOR] = boot
    fat = bytearray(fat_sectors * SECTOR)
    struct.pack_into("<HHHH", fat, 0, 0xFFF8, 0xFFFF, 0xFFFF, 0xFFFF)
    clusters_needed = (len(application) + SECTOR * SECTORS_PER_CLUSTER - 1) // (SECTOR * SECTORS_PER_CLUSTER)
    if clusters_needed == 0 or clusters_needed + 4 >= cluster_count:
        raise ValueError("BOOTAA64.EFI does not fit the deterministic ESP")
    for index in range(clusters_needed):
        cluster = 4 + index
        struct.pack_into("<H", fat, cluster * 2,
                         0xFFFF if index + 1 == clusters_needed else cluster + 1)
    first_fat = partition + SECTOR
    image[first_fat:first_fat + len(fat)] = fat
    second_fat = first_fat + len(fat)
    image[second_fat:second_fat + len(fat)] = fat
    root = second_fat + len(fat)
    image[root:root + 32] = directory_entry(b"EFI        ", 0x10, 2, 0)
    data = root + root_sectors * SECTOR
    cluster_bytes = SECTORS_PER_CLUSTER * SECTOR
    efi = bytearray(cluster_bytes)
    efi[:32] = directory_entry(b".          ", 0x10, 2, 0)
    efi[32:64] = directory_entry(b"..         ", 0x10, 0, 0)
    efi[64:96] = directory_entry(b"BOOT       ", 0x10, 3, 0)
    image[data:data + cluster_bytes] = efi
    boot_dir = bytearray(cluster_bytes)
    boot_dir[:32] = directory_entry(b".          ", 0x10, 3, 0)
    boot_dir[32:64] = directory_entry(b"..         ", 0x10, 2, 0)
    boot_dir[64:96] = directory_entry(b"BOOTAA64EFI", 0x20, 4, len(application))
    image[data + cluster_bytes:data + 2 * cluster_bytes] = boot_dir
    app_offset = data + 2 * cluster_bytes
    image[app_offset:app_offset + len(application)] = application


def build(application: bytes) -> bytes:
    if application[:2] != b"MZ" or b"PE\0\0" not in application[:4096]:
        raise ValueError("application is not a PE/COFF image")
    image = bytearray(SECTORS * SECTOR)
    image[510:512] = b"\x55\xaa"
    image[446:462] = b"\x00\x00\x02\x00\xee\xff\xff\xff" + struct.pack("<II", 1, SECTORS - 1)
    entries = bytearray(128 * 128)
    entries[:16] = guid_bytes(ESP_GUID)
    entries[16:32] = guid_bytes(PARTITION_GUID)
    struct.pack_into("<QQQ", entries, 32, PARTITION_START, PARTITION_END, 0)
    name = "BridgeVM ESP".encode("utf-16-le")
    entries[56:56 + len(name)] = name
    entries_crc = binascii.crc32(entries) & 0xFFFFFFFF
    image[2 * SECTOR:34 * SECTOR] = entries
    backup_entries_lba = SECTORS - 33
    image[backup_entries_lba * SECTOR:(SECTORS - 1) * SECTOR] = entries
    image[SECTOR:2 * SECTOR] = gpt_header(1, SECTORS - 1, 2, entries_crc)
    image[(SECTORS - 1) * SECTOR:] = gpt_header(SECTORS - 1, 1, backup_entries_lba, entries_crc)
    build_fat(image, application)
    return bytes(image)


def self_test() -> None:
    application = bytearray(1024)
    application[:2] = b"MZ"
    struct.pack_into("<I", application, 0x3C, 0x80)
    application[0x80:0x84] = b"PE\0\0"
    image = build(bytes(application))
    if len(image) != SECTORS * SECTOR or image[510:512] != b"\x55\xaa":
        raise AssertionError("disk size or protective MBR is invalid")
    if image[SECTOR:SECTOR + 8] != b"EFI PART":
        raise AssertionError("primary GPT header is missing")
    if image[-SECTOR:-SECTOR + 8] != b"EFI PART":
        raise AssertionError("backup GPT header is missing")
    partition = PARTITION_START * SECTOR
    if image[partition + 43:partition + 54] != b"BRIDGEVMPC ":
        raise AssertionError("FAT16 volume label is missing")
    if image.find(bytes(application), partition) < 0:
        raise AssertionError("BOOTAA64.EFI payload is missing")
    if hashlib.sha256(image).digest() != hashlib.sha256(build(bytes(application))).digest():
        raise AssertionError("disk construction is not deterministic")
    try:
        build(b"not a PE image")
    except ValueError:
        pass
    else:
        raise AssertionError("non-PE input was accepted")
    print("BridgeVM PC GPT/FAT boot-media self-test: PASS")


def main() -> None:
    if sys.argv[1:] == ["--self-test"]:
        self_test()
        return
    if len(sys.argv) != 3:
        raise SystemExit("usage: build-bridgevm-pc-boot-media.py APP OUTPUT | --self-test")
    source = pathlib.Path(sys.argv[1])
    output = pathlib.Path(sys.argv[2])
    data = build(source.read_bytes())
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_bytes(data)
    print(f"built {output}")
    print(f"sha256 {hashlib.sha256(data).hexdigest()}")


if __name__ == "__main__":
    main()
