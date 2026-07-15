@echo off
echo BVCOPY START
wpeinit
set SRC=
for %%D in (C D E F G H I J) do if exist %%D:\v3dfull\viogpu3d.inf set SRC=%%D:
if "%SRC%"=="" ( echo BVCOPY ERROR: v3dfull not found & goto :end )
set WIN=
for %%D in (C D E F G H I J) do if exist %%D:\Windows\System32\ntoskrnl.exe set WIN=%%D:
if "%WIN%"=="" ( echo BVCOPY ERROR: Windows volume not found & goto :end )
echo SRC=%SRC% WIN=%WIN%
if not exist %WIN%\BridgeVM\v3dfull\ mkdir %WIN%\BridgeVM\v3dfull
copy /y %SRC%\v3dfull\* %WIN%\BridgeVM\v3dfull\
echo BVCOPY DONE
:end
wpeutil shutdown
