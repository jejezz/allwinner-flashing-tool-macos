# Package a Windows release and publish it to GitHub — the Windows
# counterpart to scripts/release.sh.
#
#   .\scripts\release-windows.ps1 v0.1.0            # build, package, create/update a draft release
#   .\scripts\release-windows.ps1 v0.1.0 -Publish   # ...and publish it immediately
#
# Produces, under dist\:
#   AllwinnerFlasherSetup-<version>.exe   the Inno Setup installer
#   SHA256SUMS-windows
#
# Requires ISCC.exe (Inno Setup) on PATH or at its default install location,
# and `gh` authenticated (GH_TOKEN in CI).
#
# The version baked into the installer comes from the tag, not from
# installer/windows/aw-flasher.iss — passed via `ISCC /DMyAppVersion=x.y.z`,
# which the .iss only falls back away from when nothing overrides it.

param(
    [Parameter(Mandatory = $true)]
    [string]$Tag,
    [switch]$Publish
)

$ErrorActionPreference = "Stop"

if ($Tag -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+') {
    Write-Error "tag should look like v0.1.0 (got '$Tag')"
    exit 2
}
$version = $Tag.TrimStart("v")

$root = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $root "dist"

Push-Location $root
try {
    # A release should be reproducible from what is on the branch, not from
    # whatever happens to be in the working tree.
    $dirty = git status --porcelain
    if ($dirty) {
        Write-Error "working tree is dirty — commit or stash first"
        exit 1
    }

    Write-Host "==> building"
    & "$root\scripts\build-app-windows.ps1"
    if ($LASTEXITCODE -ne 0) { throw "build-app-windows.ps1 failed" }

    New-Item -ItemType Directory -Force -Path $dist | Out-Null

    Write-Host "==> compiling installer (Inno Setup)"
    $isccCmd = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($isccCmd) {
        $iscc = $isccCmd.Source
    } else {
        $iscc = "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
    }
    if (-not (Test-Path $iscc)) {
        throw "ISCC.exe not found — install Inno Setup (winget install JRSoftware.InnoSetup)"
    }
    & $iscc "/DMyAppVersion=$version" "$root\installer\windows\aw-flasher.iss"
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed" }

    $installer = Join-Path $dist "AllwinnerFlasherSetup-$version.exe"
    if (-not (Test-Path $installer)) {
        throw "expected installer not found: $installer"
    }

    $sums = Join-Path $dist "SHA256SUMS-windows"
    Get-FileHash $installer -Algorithm SHA256 |
        ForEach-Object { "$($_.Hash.ToLower())  $(Split-Path $_.Path -Leaf)" } |
        Out-File $sums -Encoding ascii

    Write-Host "==> publishing to GitHub"

    # macOS and Windows release independently off the same tag push, so
    # whichever finishes `gh release create` first owns the release notes —
    # the other just uploads its assets. A release that shows up within this
    # short wait is treated as "the macOS job got there first"; past it, this
    # is assumed to be a Windows-only release (see docs/windows-installer.md).
    gh release view $Tag *>$null
    $exists = ($LASTEXITCODE -eq 0)
    if (-not $exists) {
        Start-Sleep -Seconds 30
        gh release view $Tag *>$null
        $exists = ($LASTEXITCODE -eq 0)
    }

    $assets = @($installer, $sums)

    if ($exists) {
        Write-Host "    release $Tag exists — uploading Windows assets"
        gh release upload $Tag @assets --clobber
        if ($LASTEXITCODE -ne 0) { throw "gh release upload failed" }
        if ($Publish) {
            gh release edit $Tag --draft=false
        }
    } else {
        Write-Host "    release $Tag not found — creating a Windows-only release"
        $ghArgs = @("--title", $Tag, "--notes", "Windows build for $Tag.")
        if (-not $Publish) { $ghArgs += "--draft" }
        gh release create $Tag @assets @ghArgs
        if ($LASTEXITCODE -ne 0) { throw "gh release create failed" }
    }

    Write-Host ""
    if ($Publish) {
        Write-Host "완료: $Tag 공개됨"
    } else {
        Write-Host "완료: $Tag 초안 갱신됨 — 확인 후 gh release edit $Tag --draft=false"
    }
} finally {
    Pop-Location
}
