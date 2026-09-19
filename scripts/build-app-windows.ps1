# Build the distributable Windows app: the Rust helper with libusb linked
# statically, placed next to the Flutter .exe.
#
# The helper must be the `vendored` build. An ordinary `cargo build` links
# against whatever libusb happens to be on the machine (usually nothing on
# Windows), so every command would fail at launch looking for a DLL that
# isn't there. `vendored` compiles libusb from source and links it in
# statically instead, using the MSVC toolchain (Visual Studio Build Tools'
# "Desktop development with C++" workload) that `cargo` also needs for the
# MSVC target.
#
# Requires: a device's Allwinner USB interfaces bound to WinUSB (e.g. via
# Zadig) — libusb on Windows cannot talk to a device still using Microsoft's
# default driver.

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$exeDir = Join-Path $root "gui\build\windows\x64\runner\Release"

Write-Host "==> aw-tool (vendored libusb)"
cargo build --manifest-path "$root\Cargo.toml" --release --features vendored
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

Write-Host "==> Flutter app"
Push-Location "$root\gui"
try {
    flutter build windows --release
    if ($LASTEXITCODE -ne 0) { throw "flutter build failed" }
} finally {
    Pop-Location
}

Write-Host "==> bundling helper"
Copy-Item "$root\target\release\aw-tool.exe" "$exeDir\aw-tool.exe" -Force

Write-Host ""
Write-Host "완료: $exeDir\aw_flasher.exe"
