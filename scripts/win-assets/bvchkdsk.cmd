@echo off
rem BridgeVM WinPE offline NTFS repair. The target (NSID-2, disk 1) Windows
rem partition presents as RAW when its NTFS metadata is corrupt, so we do NOT
rem gate on \Windows existing. Assign a deterministic letter to disk1/part3 and
rem run chkdsk /f on it (and on the auto-mounted D:) to repair NTFS_FILE_SYSTEM(0x24).
echo BVCHKDSK START
wpeinit
echo BVCHKDSK WPEINIT-DONE

echo BVCHKDSK DISKPART-ASSIGN-BEGIN
(
echo select disk 1
echo list partition
echo select partition 3
echo assign letter=W
echo list volume
) | diskpart
echo BVCHKDSK ASSIGN-EXIT=%errorlevel%

echo BVCHKDSK FSINFO-W-BEGIN
fsutil fsinfo volumeinfo W:
echo BVCHKDSK FSINFO-W-EXIT=%errorlevel%

echo BVCHKDSK CHKDSK-W-FIX-BEGIN
echo Y| chkdsk W: /f
echo BVCHKDSK CHKDSK-W-FIX-EXIT=%errorlevel%

echo BVCHKDSK CHKDSK-D-FIX-BEGIN
echo Y| chkdsk D: /f
echo BVCHKDSK CHKDSK-D-FIX-EXIT=%errorlevel%

echo BVCHKDSK POST-VOLUMES-BEGIN
echo list volume | diskpart
echo BVCHKDSK DONE
wpeutil shutdown
