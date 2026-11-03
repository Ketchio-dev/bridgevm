# Development images only: remove every driver package for the test-signed 3D
# virtio-gpu driver (Hardsoft viogpu3d), so the signed viogpudo binds to the 2D
# device. Product VMs never carry that package.
$infs = @(Get-ChildItem C:\Windows\INF -Filter oem*.inf | Where-Object { (Get-Content $_.FullName -Raw) -match "Hardsoft|viogpu3d" } | ForEach-Object { $_.Name })
foreach ($i in $infs) { pnputil /delete-driver $i /uninstall /force | Out-Null }
"BVCLEAN removed=" + ($infs -join ",")
