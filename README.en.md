# aw-tool

[한국어](README.md)

Flashes Allwinner T507/T527 boards **from macOS**. The vendor tool, PhoenixSuit, is Windows-only, so this implements the FEL/EFEX USB protocol directly.

There is a CLI and a GUI. Point either at a board sitting in FEL mode and one command carries it from bootstrap through the full fusing pass to reboot.

```bash
aw-tool flash-all firmware.img sys_partition.fex --reboot
```

| | |
|---|---|
| `aw-tool` (Rust) | The CLI. The whole protocol implementation lives here |
| `gui/` (Flutter) | macOS app. Runs the CLI as a subprocess and shows progress |

<img src="docs/images/gui-en.png" width="620" alt="Allwinner Flasher main window">

The app follows the system language; Korean and English are supported.

## Status

**CLI**

| Item | Status |
|---|---|
| T527 (sun55iw3 / A523) full flash → Android boots normally | Verified on hardware |
| Images verified | `pluto_lobby`, `pluto_wallpad` |
| Wall time | ~1 min 40 s (bootstrap + ~1.1 GB written + reboot) |
| T507 | **Unverified** — see "Porting to T507" below |
| Storage | Verified on eMMC/SD (`storage type = 2`). NAND unverified |
| `--json` events · `probe` · exit codes | Verified offline (`gui/test/aw_tool_integration_test.dart` drives the real binary) |

**GUI**

| Item | Status |
|---|---|
| Builds, runs, uses the bundled helper | Confirmed (release `.app`, zero Homebrew links) |
| Full T527 flash through the GUI | Verified on hardware (2026-09-07) |
| Device detection · image selection · progress · completion screens | Exercised by the flash above |
| Failure screen | **Unverified on hardware** |
| Stop button | **Unverified on hardware** |
| Per-partition selection (checkboxes / `--skip`) | **Unverified on hardware** — the CLI filtering and its guards were checked offline against a real image |

Everything still unverified needs that situation to actually occur (a cancel, a failure, a selective write). On the selection feature, the CLI side was checked against the real wallpad image: 12 partitions filtered down to 10, plus all three rejection cases (combined with a full format, a misspelled name, `--only` and `--skip` together).

## Requirements

- macOS, Apple Silicon (the app is arm64-only)
- Rust toolchain (built with 1.98)
- libusb — `brew install libusb` (not needed for distributable builds, see below)
- Flutter for the GUI (checked with 3.47.1)

No `sudo` is needed for USB access.

## Build

```bash
cargo build --release
# output: target/release/aw-tool
```

A binary you ship to another machine (bundled into the GUI `.app`, say) has to link libusb statically. The default build links Homebrew's `libusb-1.0.0.dylib`, which is absent on most machines and makes every command fail at launch.

```bash
cargo build --release --features vendored
```

`vendored` builds libusb from source alongside. The resulting binary links only macOS system frameworks (CoreFoundation, IOKit, Security, libSystem, libiconv).

## Usage

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

The Flutter macOS app in `gui/`. Pick an image and it previews the partition table; it detects the board and enables the flash button, then shows progress and a log.

The interface language follows the system setting, in Korean and English. To run just this app in another language:

```bash
defaults write com.europa.awflasher AppleLanguages -array en   # undo: defaults delete ...
```

**Turning off Full format puts checkboxes on the partition list.** Unchecked partitions are left as they are (see "Writing selected partitions"). In full-format mode the checkboxes are locked and everything is written — a format wipes it all anyway. The confirmation dialog spells out the partitions being kept, so it can be checked before committing.

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

The GUI runs the CLI **as a subprocess** rather than linking it. USB transfers here really can wedge — a bad command once left a board printing `INVALID direction` with the transfer stuck until timeout. A separate process means that hangs something we can kill, and cancelling is just killing it. It also keeps GUI work away from the hardware-verified flashing path.

## Releasing

```bash
./scripts/release.sh v0.1.0             # build → package → draft GitHub release
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

### Gatekeeper

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

## Documentation

The full investigation — protocol evidence (where in the vendor sources), hardware logs, how things were verified, and **the hypotheses that were ruled out** — is in [`docs/T527-T507-FEL-EFEX-기술조사.md`](docs/T527-T507-FEL-EFEX-기술조사.md) (Korean). Start there when adding a new SoC or chasing odd behaviour.

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
| `gui/lib/about.dart` | About dialog |
| `scripts/build-app.sh` | Build the `.app`, bundle the helper, re-sign |
| `scripts/release.sh` | Package a release and publish it |
