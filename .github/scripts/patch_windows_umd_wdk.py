#!/usr/bin/env python3
"""Patch the Windows ARM64 UMD workflow to install the official SDK and WDK."""

from pathlib import Path


path = Path(".github/workflows/windows-umd.yml")
text = path.read_text()
marker = "      - name: Install matching Windows SDK and WDK\n"
if marker in text:
    raise SystemExit(0)

text = text.replace(
    "    timeout-minutes: 60\n",
    "    timeout-minutes: 90\n",
    1,
)
needle = """      - name: Cross-build the ARM64 UMD payload
        shell: pwsh
        run: |
"""
insert = """      - name: Install matching Windows SDK and WDK
        shell: pwsh
        run: |
          $packages = @(
            "Microsoft.WindowsSDK.10.0.28000",
            "Microsoft.WindowsWDK.10.0.28000"
          )
          foreach ($package in $packages) {
            winget install --id $package --exact --source winget --silent `
              --accept-source-agreements --accept-package-agreements `
              --disable-interactivity
            if ($LASTEXITCODE -ne 0) {
              throw "winget installation failed for $package with $LASTEXITCODE"
            }
          }

          $includeRoot = "${env:ProgramFiles(x86)}\\Windows Kits\\10\\Include"
          $kit = Get-ChildItem -LiteralPath $includeRoot -Directory |
            Sort-Object Name -Descending |
            Where-Object {
              (Test-Path (Join-Path $_.FullName "um\\d3d10umddi.h")) -and
              (Test-Path (Join-Path $_.FullName "shared\\d3dkmthk.h"))
            } |
            Select-Object -First 1
          if ($null -eq $kit) {
            throw "Installed WDK does not expose the required D3D UMD headers"
          }
          Write-Host "Using Windows SDK/WDK headers from $($kit.FullName)"
      - name: Cross-build the ARM64 UMD payload
        shell: pwsh
        run: |
"""
if text.count(needle) != 1:
    raise SystemExit("Windows UMD cross-build insertion anchor changed")
path.write_text(text.replace(needle, insert, 1))
