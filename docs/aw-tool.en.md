# aw-tool — CLI and development notes

[한국어](aw-tool.md) · For what the app is and how to install it, see the [README](../README.md)

> This was the repository README up to v1.1.3. When conventions-v1 rewrote the README as the app's front page, the CLI usage, build, driver and verification notes moved here.

Flashes Allwinner T507/T527 boards. Runs on macOS, Windows, and Linux — an open-source alternative to the vendor tool, PhoenixSuit, implementing the FEL/EFEX USB protocol directly.

There is a CLI and a GUI. Point either at a board sitting in FEL mode and one command carries it from bootstrap through the full fusing pass to reboot.

```bash
aw-tool flash-all firmware.img sys_partition.fex --reboot
```

| | |
|---|---|
| `aw-tool` (Rust) | The CLI. The whole protocol implementation lives here |
| `gui/` (Flutter) | Desktop app (macOS / Windows / Linux). Runs the CLI as a subprocess and shows progress |

<img src="images/gui-en.png" width="620" alt="Allwinner Flasher main window">

The app follows the system language; Korean and English are supported.

## Status

**CLI** (the protocol itself is OS-agnostic; the hardware verification below was done from a macOS host)

| Item | Status |
|---|---|
| T527 (sun55iw3 / A523) full flash → Android boots normally | Verified on hardware |
| Images verified | `pluto_lobby`, `pluto_wallpad` |
| Wall time | ~1 min 40 s (bootstrap + ~1.1 GB written + reboot) |
| T507 | **Unverified** — see "Porting to T507" below |
| Storage | Verified on eMMC/SD (`storage type = 2`). NAND unverified |
| `--json` events · `probe` · exit codes | Verified offline (`gui/test/aw_tool_integration_test.dart` drives the real binary) |

**GUI**

| Item | macOS | Windows | Linux |
|---|---|---|---|
| Builds, runs, uses the bundled helper | Confirmed (release `.app`, zero Homebrew links) | Confirmed (release build, vendored libusb linked statically) | Not attempted — only the platform scaffold has been generated |
| CLI detects a FEL device (`fel-version`) | Verified on hardware | Verified on hardware (2026-09-19, after binding WinUSB — no admin rights needed) | **Unverified** |
| Full flash through the GUI on real hardware | Verified on hardware (2026-09-07) | **Unverified** — CLI detection confirmed, `flash-all` not yet tried | **Unverified** |
| GUI device detection · image selection · progress · completion screens | Exercised by the flash above | Unverified | Unverified |
| Failure screen · stop button · per-partition selection | Unverified on hardware | Unverified | Unverified |

Everything still unverified needs that situation to actually occur (a cancel, a failure, a selective write). On the selection feature, the CLI side was checked against the real wallpad image: 12 partitions filtered down to 10, plus all three rejection cases (combined with a full format, a misspelled name, `--only` and `--skip` together) — this part is OS-agnostic.

## Requirements

**All platforms**

- Rust toolchain (built with 1.98)
- Flutter for the GUI (checked with 3.47)

**macOS**

- Apple Silicon (the GUI app is arm64-only, see "GUI" below)
- libusb — `brew install libusb` (not needed for distributable builds, see "Build" below)

**Windows**

- Visual Studio Build Tools' "Desktop development with C++" workload (provides the `cl.exe` needed by both the Rust MSVC target and libusb's source build)
- **Mandatory:** bind the board's FEL/EFEX USB interface to the **WinUSB** driver with [Zadig](https://zadig.akeo.ie/) (see "USB driver" below). The device can look perfectly normal in Device Manager and `aw-tool` will still fail to find it at all without this — confirmed on real hardware, not a maybe.

**Linux**

- libusb1 development headers and build tools (`build-essential`, `libusb-1.0-0-dev`, etc.) — *building on Linux has not been verified in this repo yet*
- A udev rule for USB access without root (see "USB driver" below)

With the driver/rule in place, no `sudo` is needed for USB access.

## Build

```bash
cargo build --release
# output: target/release/aw-tool        (Windows: target\release\aw-tool.exe)
```

A binary you ship to another machine (bundled into the GUI app, say) has to link libusb statically.

- **macOS** — the default build links Homebrew's `libusb-1.0.0.dylib`, which is absent on most machines and makes every command fail at launch.
- **Linux** — the default build is expected to link the distribution's libusb dynamically (unverified).
- **Windows** — there is no standard system location for libusb, so `libusb1-sys` falls back to building it from source whenever it can't find one through vcpkg. On the machine this was built on (no vcpkg configured), even the plain build above came out statically linked. That can vary by machine, though, so a distributable build should always say so explicitly:

```bash
cargo build --release --features vendored
```

`vendored` builds libusb from source alongside and links it statically. The macOS result links only system frameworks (CoreFoundation, IOKit, Security, libSystem, libiconv); the Windows result links only the MSVC runtime.

## USB driver

Both FEL and EFEX mode use VID:PID `1f3a:efe8`.

**macOS** — no setup needed.

**Windows — mandatory, not optional.** The board can show up **looking completely fine** in Device Manager, under "Universal Serial Bus controllers" as `USB Device (VID_1f3a&PID_efe8)`, and libusb will still be unable to open it at all if what's bound is the Microsoft default driver. Running a command against it then gives:

```
> aw-tool.exe fel-version
Error: Allwinner USB FEL device (1f3a:efe8) not found
```

That exact error means this, every time. The fix:

1. Keep the board connected in FEL (or EFEX) mode.
2. Run [Zadig](https://zadig.akeo.ie/) (no admin rights needed — confirmed on real hardware).
3. Turn on **Options → List All Devices**. Without it, a device that already has a driver bound (the default one, in this case) won't show up in the list.
4. Find the device by VID `1f3a` / PID `efe8`, set the target driver to **WinUSB**, and click **Replace Driver**.
5. Check Device Manager again — the device should have moved under "Universal Serial Bus devices".

Once bound, it's recognised automatically after that, including across reboots. No need to redo it unless the board re-enumerates under a different VID/PID.

**Linux** — a udev rule grants access without root.

```bash
echo 'SUBSYSTEM=="usb", ATTR{idVendor}=="1f3a", ATTR{idProduct}=="efe8", MODE="0666"' | sudo tee /etc/udev/rules.d/99-allwinner.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Without the rule, the tool needs `sudo`.

## Usage

(The commands below are shown as `aw-tool`. In Windows PowerShell that's `.\aw-tool.exe`.)

### 1. Put the board in FEL mode

Hold the FEL button/pin while connecting power, then check:

```bash
aw-tool fel-version
# AWUSBFEX soc=00001890(A523) ...
```

### 2. Pull out the partition spec

`sys_partition.fex` ships inside the image.

```bash
aw-tool extract firmware.img sys_partition.fex --out sys_partition.fex
aw-tool list-partitions sys_partition.fex     # offline, no hardware needed
```

### 3. Flash

```bash
aw-tool flash-all firmware.img sys_partition.fex --reboot
```

From FEL it bootstraps into EFEX on its own; if the board is already in EFEX that step is skipped.

```
bootstrap: FEL device found (soc_id=0x1890)
bootstrap: fes1 running at 0x4c000 (47264 bytes), initialising DRAM...
bootstrap: u-boot running at 0x4a000000 (950272 bytes), waiting for EFEX...
bootstrap: EFEX mode reached
[1] flash_set_on OK
[2] erase_flag=1 OK
[3] MBR OK
[4.1/12] partition 'bootloader_a' OK (16129024 bytes, raw)
...
[4.6/12] partition 'super' OK (1021182504 bytes, sparse -> 3584 MB expanded, 977 MB written)
...
[8] reboot triggered
```

The options that matter:

| Option | Meaning |
|---|---|
| `--reboot` | Reboot when finished |
| `--erase-flag 0` | Overwrite only, instead of a full format |
| `--skip env_a,misc` | Leave these partitions alone (needs `--erase-flag 0`) |
| `--only super,boot_a` | Write only these (needs `--erase-flag 0`) |
| `--boot0-item boot0_nand.fex` | BOOT0 variant for NAND boards (default `boot0_sdcard.fex`) |
| `--skip-larger-than <bytes>` | Skip large partitions (for partial test runs) |
| `--no-bootstrap` | Turn off the automatic FEL→EFEX bootstrap |

### Writing selected partitions

For keeping what the board itself has changed (`env_a`, `misc`) while updating everything else.

```bash
aw-tool flash-all firmware.img sys_partition.fex --erase-flag 0 --skip env_a,misc --reboot
```

```
keeping 'env_a' (not selected)
keeping 'misc' (not selected)
plan: MBR(65536 B) + 10 partitions (1017 MB total) + BOOT1(1376256 B) + BOOT0(69632 B)
```

**`--erase-flag 0` is required, and the command is refused without it.** A full format (`erase_flag=1`) erases every partition before writing, so one left out would end up **blank rather than preserved** — the opposite of the intent, so the combination is rejected outright.

A name that is not in `sys_partition.fex` is an error, and the known names are printed. Ignoring it silently would mean a typo like `--skip env` (the real name is `env_a`) overwrites the very partition you were protecting.

MBR / BOOT0 / BOOT1 are always written regardless of the selection. They belong to the firmware, not to user data.

## Commands

**Working with images (offline)**

| Command | Description |
|---|---|
| `list` | Items in an IMAGEWTY image |
| `extract` | Pull out one item |
| `list-partitions` | Computed start sector for each partition in `sys_partition.fex` |
| `patch-workmode` | Patch `work_mode` and recompute the checksum |

**Checking the device**

| Command | Description |
|---|---|
| `probe` | Reports whether `fel`, `efex`, or `none` is connected, in one shot |
| `fel-version` | Whether it is in FEL mode, and which SoC |
| `efex-verify-dev` | Whether it is in EFEX mode |
| `efex-query-storage` | Which storage it booted from (picks the BOOT0 variant) |

`probe` exits 0 and reports `none` when nothing is attached: it is meant for polling while waiting for a board, where "nothing" is a normal state.

**Flashing (destructive)**

| Command | Description |
|---|---|
| `bootstrap` | FEL → EFEX only |
| `flash-all` | The whole fusing pass — **the main command** |
| `flash-partition` | A single partition |
| `flash-mbr` / `flash-boot1` / `flash-boot0` | Individual targets |
| `flash-set-erase-flag` | Set the erase flag |

> `flash-*` commands erase what is on the device. There is no undo.

## GUI

The Flutter desktop app in `gui/` (macOS / Windows / Linux). Pick an image and it previews the partition table; it detects the board and enables the flash button, then shows progress and a log.

Language and theme follow the system by default. Only Korean and English exist: Korean if the system is Korean, English for every other language. The theme button (System / Light / Dark) and the language button (Follow System / 한국어 / English) in the header override them, and the choice is saved in `shared_preferences` under `theme_mode` / `app_locale`. The language file used up to v1.1.3 (`<config folder>/AllwinnerFlasher/language`) is moved over once on first launch and deleted (`gui/lib/settings/legacy_language_file.dart`).

**Turning off Full format puts checkboxes on the partition list.** Unchecked partitions are left as they are (see "Writing selected partitions"). In full-format mode the checkboxes are locked and everything is written — a format wipes it all anyway. The confirmation dialog spells out the partitions being kept, so it can be checked before committing.

The GUI runs the CLI **as a subprocess** rather than linking it. USB transfers here really can wedge — a bad command once left a board printing `INVALID direction` with the transfer stuck until timeout. A separate process means that hangs something we can kill, and cancelling is just killing it. It also keeps GUI work away from the hardware-verified flashing path.

### Opening in Android Studio

This repo's `pubspec.yaml` lives in `gui/`, not the repo root. For Android Studio's Flutter plugin to recognise the project, **open the `gui/` folder itself** (`File > Open` → select `gui/`). Opening the repo root will not be recognised as a Flutter project.

1. Make sure the Flutter/Dart plugin is installed, and set the Flutter SDK path under `Settings > Languages & Frameworks > Flutter`.
2. Opening `gui/` auto-creates a "main.dart" run/debug configuration from `lib/main.dart`.
3. In the device dropdown, pick **"macOS (desktop)"** / **"Windows (desktop)"** / **"Linux (desktop)"** — there is no `android/` or `ios/` folder, so pick a desktop target rather than an emulator.
4. The GUI looks for `target/release/aw-tool` (`aw-tool.exe` on Windows) inside the checkout on its own (`locate()` in `gui/lib/aw_tool.dart`), so build it once from a terminal before running from Android Studio.

   ```bash
   cargo build --release --features vendored
   ```

   To automate that, add a "Before launch" step: create an External Tool under `Settings > Tools > External Tools` (Program `cargo`, Arguments `build --release --features vendored`, Working directory `$ProjectFileDir$/..`), then add "Run External tool" to the configuration's Before launch list under `Run > Edit Configurations`.

### macOS

```bash
./scripts/build-app.sh
# output: gui/build/macos/Build/Products/Release/Allwinner Flasher.app (~37 MB)
```

What the script does: build the helper with `--features vendored`, check no Homebrew link survived, build the Flutter release, copy the helper into `Contents/Resources/`, re-sign. Adding a file to the bundle invalidates the signature Flutter produced, and macOS refuses to launch a bundle whose signature no longer matches.

During development the app finds `target/release/aw-tool` in the checkout on its own, so just run it:

```bash
cargo build --release && cd gui && flutter run -d macos
```

**The App Sandbox is off** (`macos/Runner/*.entitlements`). This is an internal tool: inside the sandbox it would need the `com.apple.security.device.usb` entitlement, and the bundled helper would inherit the sandbox too. That is also why `FilePicker.skipEntitlementsChecks()` is called — file_picker's own pre-flight check would otherwise block the dialog. Going the App Store route means reversing all three together.

The app is **Apple Silicon (arm64) only**. Flutter would happily emit a universal binary, but cargo builds the bundled helper for the host architecture alone, so a universal app would launch on an Intel Mac and fail the moment it ran the helper. Matching the architectures keeps the bundle honest (`ARCHS` in `gui/macos/Runner/Configs/Release.xcconfig`). To go universal: install rustup (the Homebrew toolchain carries no x86_64 std), `rustup target add x86_64-apple-darwin`, and `lipo -create` the two helpers together.

### Windows

```powershell
.\scripts\build-app-windows.ps1
# output: gui\build\windows\x64\runner\Release\aw_flasher.exe (aw-tool.exe sits next to it)
```

What the script does: build the helper with `--features vendored`, build the Flutter release, copy the helper (`aw-tool.exe`) into the same folder as the app executable. Unlike macOS there is no bundle structure or code signature to preserve, so there is no re-signing step.

During development the app finds `target\release\aw-tool.exe` in the checkout on its own:

```powershell
cargo build --release
cd gui
flutter run -d windows
```

**There is no code signature.** Windows SmartScreen may show a "Windows protected your PC" warning on first launch — "More info → Run anyway" gets past it. Same reason as macOS's Gatekeeper (no publisher signature).

**Real hardware needs the WinUSB driver bound first** — see "USB driver" above.

### Linux

*Building and running on Linux has not been verified in this repo yet.* The platform scaffold (`gui/linux/`) has been generated, so in principle:

```bash
cargo build --release --features vendored
cd gui && flutter build linux --release
cp ../target/release/aw-tool build/linux/x64/release/bundle/aw-tool
```

(The exact bundle path may vary with the Flutter/CMake version.)

## Releasing

Bump the version with `scripts/bump-version.sh patch|minor|major` (it bumps `gui/pubspec.yaml` and `Cargo.toml` together, build number +1), merge the PR, then tag the merge commit on `main`.

Packaging and GitHub release automation exist for macOS/Windows/Linux — run a
platform locally, or push a `v*.*.*` tag to have
[.github/workflows/release.yml](../.github/workflows/release.yml) build all
three in parallel (`AllwinnerFlasher-<version>-macos-arm64.dmg`, `-windows-x64-setup.exe`, `-linux-x64.tar.gz`, plus `SHA256SUMS.txt`) and publish them as **one** GitHub release (if any platform
fails, no release is created at all). macOS is signed with a Developer ID and
notarized in CI (secrets already registered — see
[docs/release-ci.md](release-ci.md) for details).

The local scripts still only ad-hoc sign:

```bash
./scripts/release.sh v0.1.0             # macOS: build → package → draft GitHub release
./scripts/release.sh v0.1.0 --publish   # publish instead of drafting
```

Three artifacts land in `dist/`:

| Artifact | Contents |
|---|---|
| `Allwinner-Flasher-<tag>-macos-arm64.zip` | The app (~16 MB) |
| `aw-tool-<tag>-macos-arm64.tar.gz` | The CLI on its own |
| `SHA256SUMS` | Checksums |

The script refuses a dirty working tree, checks the helper's architecture, verifies the signature still validates after a round-trip through the archive, then pushes the tag and calls `gh release create`.

It uses `ditto -c -k --keepParent`, not `zip`. `zip(1)` does not preserve a bundle's symlinks and extended attributes, so the code signature breaks on extraction.

`scripts/release-windows.ps1` does the same job for Windows — build, compile
the [Inno Setup](windows-installer.md) installer, upload to the same
tag's GitHub release.

```powershell
.\scripts\release-windows.ps1 v0.1.0            # build → package → draft/update the GitHub release
.\scripts\release-windows.ps1 v0.1.0 -Publish   # publish instead of drafting
```

### Gatekeeper

> Release DMGs built by CI are Developer ID signed and notarized and need none of this. This section only applies to ad-hoc signed apps from the local scripts (`scripts/build-app.sh`, `scripts/release.sh`).

**The app is not signed or notarised with an Apple Developer ID** — it is ad-hoc signed. Downloaded, Gatekeeper refuses to run it, which was checked rather than assumed:

```
$ spctl -a -vvv -t exec "Allwinner Flasher.app"
Allwinner Flasher.app: rejected
```

Whoever receives it has to clear the quarantine attribute once, before first launch:

```bash
xattr -dr com.apple.quarantine "/Applications/Allwinner Flasher.app"
```

Doing it properly means joining the Apple Developer Program ($99/year) for a Developer ID Application certificate, signing with `codesign --options runtime --timestamp`, then `notarytool submit --wait` and `stapler staple`. At that point the ad-hoc `--sign -` in `build-app.sh` becomes the certificate name.

## Driving it from a GUI (`--json`)

Every command takes `--json` and then writes **newline-delimited JSON** to stdout instead of prose: one object per line, each tagged with an `event` key. It exists so a front-end can run this as a subprocess and parse stdout.

```
{"event":"step","step":"fel_found","message":"bootstrap: FEL device found (soc_id=0x1890)"}
{"event":"plan","partitions":12,"total_bytes":1150000000,"mbr_bytes":16384,...}
{"event":"partition_begin","index":6,"total":12,"name":"super","bytes":1021182504}
{"event":"progress","name":"super","written":104857600,"total":1024458752}
{"event":"partition_end","index":6,"total":12,"name":"super","format":"sparse","seconds":74.3}
{"event":"done"}
```

| Event | Purpose |
|---|---|
| `plan` | Total byte count up front, so a progress bar can be sized |
| `step` | Named milestones (`fel_found`, `mbr`, `boot0`, `reboot`, …) |
| `partition_begin` / `partition_end` | Partition boundaries |
| `progress` | Byte progress **within** a partition (throttled to 100 ms) |
| `done` / `error` | Termination. `error` carries the anyhow context chain as `causes` |
| `probe`, `items`, `partitions`, … | Structured results from the query commands |

`progress` reaches inside a partition because of `super`: about 1 GB written in 64 KB chunks. With partition-level events alone the bar would sit on one step for **over a minute** of the 1:40 run and look stuck.

Failures are never propagated as an exception — they always arrive as a final `error` line (with exit code 1), so a front-end never has to parse stderr separately.

## How it works

```
FEL (boot ROM)
  ├─ fes1.fex   (work_mode=0x10 patched) → uploaded to 0x4C000 and run → DRAM init
  └─ u-boot.fex (work_mode=0x10 patched) → uploaded to 0x4A000000 and run
       ↓ USB re-enumerates
EFEX
  ├─ set the erase flag → erase every partition
  ├─ write MBR/GPT
  ├─ write each partition (sparse images are expanded on the way)
  ├─ BOOT1 (boot_package.fex) / BOOT0 (boot0_*.fex)
  └─ reboot
```

Both primers are taken straight out of the image and patched, so no files need preparing, and there is no dependency on external `sunxi-tools`.

## Things to know

**`super.fex` is not a raw image.** It is an Android sparse container (magic `0xED26FF3A`) and has to be expanded as it is written. Writing it verbatim puts the sparse header where liblp's metadata belongs: the kernel boots, but first-stage init cannot find the dynamic partitions and reboots to the bootloader before mounting anything. The tool detects this by magic, but when dealing with a new partition, check whether the payload is a container even if the filename matches the partition name.

**There are two address spaces.** Partition offsets appear in an `addrlo` space and a GPT LBA space, always `0xa000` sectors (20 MB) apart.

- `sys_partition.fex` and this tool's sector arguments → **addrlo space** (e.g. super `0x90400`)
- `mmc read` / `part list` in a U-Boot shell → **GPT LBA space** (e.g. super `0x9a400`)

Mixing the two while comparing against a device dump leads straight to wrong conclusions.

**Primers are not what gets written.** The fes1/u-boot copies with `work_mode` patched only ever live in DRAM. The BOOT0/BOOT1 written to storage must be the untouched originals.

## Porting to T507

The pipeline is shared, but these are SoC-specific and have to be re-checked against that SDK.

- **Primer load addresses** — for T527, fes1 `0x4C000` and u-boot `0x4A000000`. The evidence is `CONFIG_FES1_RUN_ADDR` in `include/configs/<soc>.h`, `fes/fes1.lds`, and `CONFIG_SYS_TEXT_BASE` in `u-boot.lds` / `.config`. Override with `--fes1-addr` / `--uboot-addr`.
  > Loading fes1 at the wrong address (for instance `0x44000`, the boot0 address `sunxi-fel spl` assumes) leaves the board unresponsive. Power-cycling recovers it.
- **BOOT0 variant** — check the medium with `efex-query-storage` and pick `boot0_sdcard.fex` or `boot0_nand.fex`.

## Credits

- App icon source: an [Icons8](https://icons8.com) Sticker glyph (`gui/assets/icon/source_glyph.svg`; paid plan, so no in-app credit). `gui/tool/icon/generate_icons.py` puts it on the shared plate from [application-release-templates](https://github.com/jejezz/application-release-templates) and writes the macOS, Windows and Linux icons in one go (`cd gui && python3 tool/icon/generate_icons.py`, needs Pillow).
- Font: [SeoulNamsan](https://www.seoul.go.kr/seoul/font.do) (Seoul Metropolitan Government)
- USB: [libusb](https://libusb.info) (LGPL-2.1) — full text in the app's "Open Source Licenses" page

## Documentation

The full investigation — protocol evidence (where in the vendor sources), hardware logs, how things were verified, and **the hypotheses that were ruled out** — is in [`docs/T527-T507-FEL-EFEX-기술조사.md`](T527-T507-FEL-EFEX-기술조사.md) (Korean). Start there when adding a new SoC or chasing odd behaviour.

## Layout

| File | Role |
|---|---|
| `src/imagewty.rs` | IMAGEWTY container parser |
| `src/sunxi_head.rs` | `work_mode` patch (offset 224) + `mksunxiboot` checksum |
| `src/fel.rs` | FEL USB protocol |
| `src/efex.rs` | EFEX protocol (from the vendor's `usb_efex.h`/`.c`) |
| `src/bootstrap.rs` | Automatic FEL→EFEX entry |
| `src/sparse.rs` | Android sparse image parser |
| `src/sys_partition.rs` | `sys_partition.fex` parser + offset computation |
| `src/event.rs` | Progress reporter (prose and NDJSON) |
| `gui/lib/aw_tool.dart` | Runs the CLI, parses JSON events |
| `gui/lib/flasher_model.dart` | Device polling, image loading, flash state |
| `gui/lib/main.dart` | UI |
| `gui/lib/l10n/*.arb` | Korean and English strings (generated files are not committed) |
| `gui/lib/about/` | About dialog (common `about_dialog.dart` + app wording in `allwinner_about.dart`), macOS app menu, extra license registration |
| `gui/lib/troubleshoot.dart` | Windows USB driver troubleshooting dialog |
| `gui/lib/settings/` | Theme/language settings and their menus, migration of the old language file |
| `scripts/bump-version.sh` | Release version bump (`gui/pubspec.yaml` + `Cargo.toml`) |
| `installer/windows/app.iss` | Windows Inno Setup script ([windows-installer.md](windows-installer.md)) |
| `tool/readme/` | README screenshot and check tools (application-release-templates) |
| `scripts/build-app.sh` | Build the macOS `.app`, bundle the helper, re-sign |
| `scripts/build-app-windows.ps1` | Build for Windows, bundle the helper (`aw-tool.exe`) |
| `scripts/release.sh` | Package a macOS release (ad-hoc signed) and publish it — local only |
| `scripts/release-windows.ps1` | Package a Windows release (Inno Setup) and publish it — local only |
| `.github/workflows/release.yml` | On a tag push, build macOS (signed+notarized)/Windows/Linux in parallel and publish one GitHub release ([docs/release-ci.md](release-ci.md)) |
| `gui/tool/icon/generate_icons.py` | Generates the macOS, Windows and Linux app icons from the source glyph |
