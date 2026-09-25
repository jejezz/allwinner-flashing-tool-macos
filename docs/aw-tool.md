# aw-tool — CLI와 개발 문서

[English](aw-tool.en.md) · 앱 소개와 설치는 [README](../README.ko.md)

> v1.1.3까지 저장소 README였던 문서다. conventions-v1 적용 때 README를 앱 소개용으로 새로 쓰면서, CLI 사용법·빌드·드라이버·검증 상태 같은 개발 내용은 여기로 옮겼다.

Allwinner T507/T527 보드를 플래싱하는 도구. macOS · Windows · Linux에서 동작하며, 벤더 도구인 PhoenixSuit을 대체하는 오픈소스 구현이다. FEL/EFEX USB 프로토콜을 직접 구현했다.

CLI와 GUI 두 가지로 쓸 수 있다. FEL 모드에 들어간 보드에 명령 하나를 실행하면 부트스트랩부터 전체 퓨징, 재부팅까지 끝난다.

```bash
aw-tool flash-all firmware.img sys_partition.fex --reboot
```

| | |
|---|---|
| `aw-tool` (Rust) | CLI. 프로토콜 구현 전체가 여기 있다 |
| `gui/` (Flutter) | 데스크톱 앱 (macOS / Windows / Linux). CLI를 서브프로세스로 실행하고 진행률을 표시한다 |

<img src="images/gui-ko.png" width="620" alt="Allwinner Flasher 메인 화면">

시스템 언어에 따라 한국어와 영어를 지원한다.

<img src="images/gui-en.png" width="620" alt="Allwinner Flasher main window in English">

## 상태

**CLI** (프로토콜 자체는 OS와 무관하다. 아래 실기 검증은 macOS 호스트에서 진행했다)

| 항목 | 상태 |
|---|---|
| T527 (sun55iw3 / A523) 전체 플래싱 → Android 정상 부팅 | 실기 검증 완료 |
| 검증한 이미지 | `pluto_lobby`, `pluto_wallpad` 2종 |
| 소요 시간 | 약 1분 40초 (부트스트랩 + 약 1.1 GB 기록 + 재부팅) |
| T507 | **미검증** — 아래 "T507로 옮길 때" 참조 |
| 저장 매체 | eMMC/SD(`storage type = 2`)에서 검증. NAND 미검증 |
| `--json` 이벤트 · `probe` · 종료 코드 | 오프라인 명령으로 검증 (`gui/test/aw_tool_integration_test.dart`가 실제 바이너리를 구동) |

**GUI**

| 항목 | macOS | Windows | Linux |
|---|---|---|---|
| 빌드 · 실행 · 번들된 helper 사용 | 확인 (릴리즈 `.app`, Homebrew 링크 0) | 확인 (릴리즈 빌드, vendored libusb 정적 링크) | 미확인 — 플랫폼 스캐폴딩만 생성됨, 빌드 미시도 |
| CLI로 FEL 장치 인식 (`fel-version`) | 실기 검증 완료 | 실기 검증 완료 (2026-09-19, WinUSB 바인딩 후 — 관리자 권한 불필요) | **미검증** |
| 실기 보드로 전체 플래싱 | 실기 검증 완료 (2026-09-07) | **미검증** — CLI 인식까지는 확인, `flash-all`은 아직 | **미검증** |
| GUI 장치 감지 · 이미지 선택 · 진행률 · 완료 화면 | 위 플래싱 과정에서 확인 | 미검증 | 미검증 |
| 실패 화면 · 중단 버튼 · 파티션 골라 쓰기 | 실기 미검증 | 미검증 | 미검증 |

미검증 항목은 모두 실제로 그 상황을 만들어야 도달한다(중단, 실패, 선택 기록). 선택 기능의 CLI 쪽은 실제 wallpad 이미지로 파티션 12개 → 10개 필터링과 세 가지 거부 조건(전체 포맷과 병용, 이름 오타, `--only`/`--skip` 동시 사용)을 확인했다 — 이 부분은 OS와 무관하다.

## 요구 사항

**공통**

- Rust 툴체인 (1.98로 빌드 확인)
- GUI를 빌드할 경우 Flutter (3.47로 확인)

**macOS**

- Apple Silicon (GUI 앱은 arm64 전용, 아래 "GUI" 참조)
- libusb — `brew install libusb` (배포용 빌드에는 불필요, 아래 "빌드" 참조)

**Windows**

- Visual Studio Build Tools의 "C++를 사용한 데스크톱 개발" 워크로드 (Rust MSVC 타겟과 libusb 소스 빌드에 필요한 `cl.exe`를 제공한다)
- **반드시** [Zadig](https://zadig.akeo.ie/)로 보드의 FEL/EFEX USB 인터페이스를 **WinUSB** 드라이버로 바인딩해야 한다 (아래 "USB 드라이버" 참조). 장치 관리자에 장치가 정상으로 보여도, 이 바인딩 없이는 `aw-tool`이 장치를 아예 찾지 못한다 — 실기로 확인된 필수 단계다.

**Linux**

- libusb1 개발 헤더와 빌드 도구 (`build-essential`, `libusb-1.0-0-dev` 등) — *이 저장소에서 실기 빌드는 아직 검증되지 않았다*
- USB 접근 권한을 위한 udev 규칙 (아래 "USB 드라이버" 참조)

드라이버/규칙이 갖춰지면 USB 접근에 `sudo`는 필요 없다.

## 빌드

```bash
cargo build --release
# 산출물: target/release/aw-tool        (Windows: target\release\aw-tool.exe)
```

다른 머신에 배포할 바이너리(예: GUI 앱에 동봉)는 libusb를 정적 링크해야 합니다.

- **macOS** — 기본 빌드는 Homebrew의 `libusb-1.0.0.dylib`을 동적 링크하므로, libusb가 없는 머신에서 실행되지 않습니다.
- **Linux** — 기본 빌드는 배포판의 libusb를 동적 링크할 것으로 예상됩니다 (미검증).
- **Windows** — 시스템 표준 위치에 libusb가 없어서, `libusb1-sys`가 vcpkg로 찾지 못하면 소스 빌드로 자동 폴백합니다. 이 저장소를 빌드한 환경(vcpkg 미설정)에서는 플래그 없이도 정적 링크된 바이너리가 나왔습니다. 다만 머신마다 달라질 수 있으니 배포용은 항상 아래처럼 명시적으로 지정합니다.

```bash
cargo build --release --features vendored
```

`vendored`는 libusb를 소스에서 함께 빌드해 정적으로 링크합니다. macOS 결과물은 시스템 프레임워크(CoreFoundation, IOKit, Security, libSystem, libiconv)만, Windows 결과물은 MSVC 런타임만 링크합니다.

## USB 드라이버

FEL과 EFEX 모드 모두 VID:PID `1f3a:efe8`를 쓴다.

**macOS** — 별도 드라이버 설정이 필요 없다.

**Windows — 반드시 필요, 건너뛸 수 없다.** 장치 관리자에 보드가 "범용 직렬 버스 컨트롤러" 아래 `USB Device(VID_1f3a&PID_efe8)`처럼 **정상으로 인식된 것처럼 보여도**, 붙어있는 게 Microsoft 기본 드라이버라면 libusb는 그 장치를 열거조차 못 한다. 그 상태에서 실행하면:

```
> aw-tool.exe fel-version
Error: Allwinner USB FEL device (1f3a:efe8) not found
```

이 에러가 나면 100% 이 문제다. 고치는 법:

1. 보드를 FEL(또는 EFEX) 모드로 연결한 상태를 유지한다.
2. [Zadig](https://zadig.akeo.ie/)를 실행한다 (관리자 권한 불필요 — 실기로 확인).
3. **Options → List All Devices**를 켠다. 켜지 않으면 이미 기본 드라이버가 붙은 장치는 목록에 안 보인다.
4. VID `1f3a` / PID `efe8`에 해당하는 장치를 찾아 오른쪽 드라이버를 **WinUSB**로 지정하고 **Replace Driver**를 누른다.
5. 장치 관리자에서 해당 장치가 "범용 직렬 버스 장치(Universal Serial Bus devices)" 아래로 옮겨갔는지 확인한다.

한 번 바인딩하면 이후에는 재부팅해도 자동으로 인식된다. 보드를 재플래싱해서 다른 VID/PID로 재열거되는 경우가 아니라면 다시 할 필요는 없다.

**Linux** — udev 규칙으로 일반 사용자 권한을 허용한다.

```bash
echo 'SUBSYSTEM=="usb", ATTR{idVendor}=="1f3a", ATTR{idProduct}=="efe8", MODE="0666"' | sudo tee /etc/udev/rules.d/99-allwinner.rules
sudo udevadm control --reload-rules && sudo udevadm trigger
```

규칙을 추가하지 않으면 `sudo`로 실행해야 한다.

## 사용법

(아래 명령 예시는 `aw-tool`로 표기한다. Windows PowerShell에서는 `.\aw-tool.exe`.)

### 1. 보드를 FEL 모드로

FEL 버튼/핀을 누른 채 전원을 연결한다. 확인:

```bash
aw-tool fel-version
# AWUSBFEX soc=00001890(A523) ...
```

### 2. 파티션 스펙 꺼내기

`sys_partition.fex`는 이미지 안에 들어 있다.

```bash
aw-tool extract firmware.img sys_partition.fex --out sys_partition.fex
aw-tool list-partitions sys_partition.fex     # 오프라인 확인 (하드웨어 불필요)
```

### 3. 플래싱

```bash
aw-tool flash-all firmware.img sys_partition.fex --reboot
```

FEL 상태면 자동으로 EFEX까지 진입한다. 이미 EFEX면 그 단계를 건너뛴다.

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

주요 옵션:

| 옵션 | 뜻 |
|---|---|
| `--reboot` | 완료 후 재부팅 |
| `--erase-flag 0` | 전체 포맷 대신 덮어쓰기만 |
| `--skip env_a,misc` | 해당 파티션은 건드리지 않음 (`--erase-flag 0` 필요) |
| `--only super,boot_a` | 해당 파티션만 기록 (`--erase-flag 0` 필요) |
| `--boot0-item boot0_nand.fex` | NAND 보드용 BOOT0 선택 (기본값은 `boot0_sdcard.fex`) |
| `--skip-larger-than <bytes>` | 큰 파티션 건너뛰기 (부분 테스트용) |
| `--no-bootstrap` | 자동 FEL→EFEX 진입 끄기 |

### 파티션 골라 쓰기

보드가 스스로 바꿔 놓은 파티션(`env_a`, `misc` 등)을 남긴 채 나머지만 갱신할 때 쓴다.

```bash
aw-tool flash-all firmware.img sys_partition.fex --erase-flag 0 --skip env_a,misc --reboot
```

```
keeping 'env_a' (not selected)
keeping 'misc' (not selected)
plan: MBR(65536 B) + 10 partitions (1017 MB total) + BOOT1(1376256 B) + BOOT0(69632 B)
      선택된 파티션만 기록: bootloader_a, boot_a, vendor_boot_a, ...
```

**`--erase-flag 0`이 반드시 필요하며, 없으면 명령이 거부된다.** 전체 포맷(`erase_flag=1`)은 기록 전에 모든 파티션을 지우므로, 빼놓은 파티션은 보존되는 게 아니라 **빈 채로 남는다.** 보존 의도와 정반대 결과라 조합 자체를 막았다.

`sys_partition.fex`에 없는 이름을 주면 에러가 나고 알려진 이름 목록을 보여준다. 조용히 무시하면 `--skip env`(실제 이름은 `env_a`) 같은 오타가 바로 지키려던 파티션을 덮어쓰게 된다.

MBR / BOOT0 / BOOT1은 선택과 무관하게 항상 기록된다. 사용자 데이터가 아니라 펌웨어에 속하는 영역이다.

## 명령 목록

**이미지 다루기 (오프라인)**

| 명령 | 설명 |
|---|---|
| `list` | IMAGEWTY 이미지의 아이템 목록 |
| `extract` | 아이템 하나 꺼내기 |
| `list-partitions` | `sys_partition.fex`의 파티션별 시작 섹터 계산 |
| `patch-workmode` | `work_mode` 패치 + 체크섬 재계산 |

**장치 상태 확인**

| 명령 | 설명 |
|---|---|
| `probe` | 지금 연결된 것이 `fel` / `efex` / `none` 중 무엇인지 한 번에 보고 |
| `fel-version` | FEL 모드인지, 어떤 SoC인지 |
| `efex-verify-dev` | EFEX 모드인지 |
| `efex-query-storage` | 부팅한 저장 매체 종류 (BOOT0 변종 선택에 사용) |

`probe`는 장치가 없어도 종료 코드 0으로 `none`을 보고합니다. 연결 대기 중 폴링하는 용도라 "없음"이 정상 상태이기 때문입니다.

**플래싱 (파괴적)**

| 명령 | 설명 |
|---|---|
| `bootstrap` | FEL → EFEX 진입만 |
| `flash-all` | 전체 퓨징 — **주 사용 명령** |
| `flash-partition` | 파티션 하나만 |
| `flash-mbr` / `flash-boot1` / `flash-boot0` | 개별 대상 기록 |
| `flash-set-erase-flag` | erase 플래그 설정 |

> `flash-*` 명령은 장치 내용을 지운다. 되돌릴 수 없다.

## GUI

`gui/`의 Flutter 데스크톱 앱 (macOS / Windows / Linux). 이미지를 고르면 파티션 목록을 미리 보여주고, 보드 연결을 감지해 플래싱 버튼을 활성화하며, 진행률과 로그를 표시한다.

화면 언어와 테마는 기본적으로 시스템 설정을 따른다. 언어는 한국어와 영어만 있어서, 시스템 언어가 한국어면 한국어, 그 밖의 모든 언어는 영어로 뜬다. 헤더의 테마 버튼(시스템 / 라이트 / 다크)과 언어 버튼(시스템 설정 따르기 / 한국어 / English)으로 직접 고를 수 있고, 고른 값은 `shared_preferences`의 `theme_mode` / `app_locale`에 저장돼 다음 실행에도 유지된다. v1.1.3까지 쓰던 언어 설정 파일(`<설정 폴더>/AllwinnerFlasher/language`)은 첫 실행 때 한 번 옮기고 지운다 (`gui/lib/settings/legacy_language_file.dart`).

**전체 포맷을 끄면 파티션 목록에 체크박스가 생긴다.** 체크를 해제한 파티션은 그대로 남는다(위 "파티션 골라 쓰기" 참조). 전체 포맷 모드에서는 체크박스가 잠기고 모두 기록된다 — 포맷은 어차피 전부 지우기 때문이다. 확인 대화상자가 유지할 파티션 이름을 그대로 보여주므로 실행 전에 확인할 수 있다.

GUI는 CLI를 **서브프로세스로** 실행한다. FFI로 링크하지 않는 이유는 USB 전송이 실제로 멈출 수 있기 때문이다 — 잘못된 명령이 보드를 `INVALID direction` 상태로 만들고 전송이 타임아웃까지 걸린 적이 있다. 별도 프로세스면 그 hang은 kill로 끝나고, 취소도 프로세스 종료로 처리된다. 검증이 끝난 플래싱 경로를 GUI 작업이 건드리지 않는 이점도 있다.

### Android Studio에서 열기

이 리포의 `pubspec.yaml`은 리포 루트가 아니라 `gui/`에 있다. Android Studio의 Flutter 플러그인이 프로젝트를 인식하려면 **`gui/` 폴더 자체를 열어야** 한다 (`File > Open` → `gui/` 선택). 리포 루트를 열면 Flutter 프로젝트로 인식되지 않는다.

1. Flutter/Dart 플러그인이 설치되어 있는지 확인하고, `Settings > Languages & Frameworks > Flutter`에서 Flutter SDK 경로를 지정한다.
2. `gui/`를 열면 `lib/main.dart`를 보고 "main.dart" Run/Debug 구성이 자동으로 생긴다.
3. 상단 디바이스 드롭다운에서 **"macOS (desktop)"** / **"Windows (desktop)"** / **"Linux (desktop)"** 을 고른다 — `android/`, `ios/` 폴더가 없으므로 에뮬레이터가 아니라 데스크톱 타깃을 선택해야 한다.
4. GUI는 `target/release/aw-tool`(Windows는 `aw-tool.exe`)을 저장소 안에서 자동으로 찾으므로(`gui/lib/aw_tool.dart`의 `locate()`), Android Studio에서 실행하기 전에 터미널에서 한 번은 직접 빌드해둬야 한다.

   ```bash
   cargo build --release --features vendored
   ```

   매번 수동으로 하기 번거로우면 Run Configuration의 "Before launch"에 이 명령을 실행하는 External Tool을 추가할 수 있다. `Settings > Tools > External Tools`에서 Program `cargo`, Arguments `build --release --features vendored`, Working directory `$ProjectFileDir$/..`로 도구를 만든 뒤, `Run > Edit Configurations`의 Before launch에 "Run External tool"로 추가한다.

### macOS

```bash
./scripts/build-app.sh
# 산출물: gui/build/macos/Build/Products/Release/Allwinner Flasher.app (약 37 MB)
```

이 스크립트가 하는 일: `--features vendored`로 helper 빌드 → Homebrew 링크가 남았는지 검사 → Flutter 릴리즈 빌드 → helper를 `Contents/Resources/`에 복사 → 재서명. 번들에 파일을 넣으면 Flutter가 만든 서명이 깨지고 macOS가 실행을 거부하므로 재서명이 필요하다.

개발 중에는 앱이 저장소의 `target/release/aw-tool`을 자동으로 찾으므로 그냥 실행하면 된다.

```bash
cargo build --release && cd gui && flutter run -d macos
```

**App Sandbox를 끈 상태다** (`macos/Runner/*.entitlements`). 사내 도구 전제이며, 샌드박스 안에서는 USB 접근에 `com.apple.security.device.usb` 엔타이틀먼트가 필요하고 번들된 helper도 샌드박스를 상속한다. 이 때문에 `file_picker`의 사전 검사도 통과하지 못해 `FilePicker.skipEntitlementsChecks()`를 호출한다. App Store 배포로 방향을 바꾸려면 이 세 가지를 함께 되돌려야 한다.

앱은 **Apple Silicon(arm64) 전용**이다. Flutter는 유니버설 바이너리를 만들 수 있지만 번들된 helper는 cargo가 호스트 아키텍처로만 빌드하므로, 유니버설 앱은 Intel 맥에서 실행은 되고 helper를 부르는 순간 실패한다. 아키텍처를 맞춰 두는 편이 정직하다(`gui/macos/Runner/Configs/Release.xcconfig`의 `ARCHS`). 유니버설로 바꾸려면 rustup을 설치하고(Homebrew 툴체인에는 x86_64 std가 없다) `rustup target add x86_64-apple-darwin` 후 두 벌을 `lipo -create`로 합치면 된다.

### Windows

```powershell
.\scripts\build-app-windows.ps1
# 산출물: gui\build\windows\x64\runner\Release\aw_flasher.exe (+ 옆에 aw-tool.exe)
```

이 스크립트가 하는 일: `--features vendored`로 helper 빌드 → Flutter 릴리즈 빌드 → helper(`aw-tool.exe`)를 실행 파일과 같은 폴더에 복사. macOS와 달리 번들 구조나 코드 서명이 없어 재서명 단계는 없다.

개발 중에는 앱이 저장소의 `target\release\aw-tool.exe`를 자동으로 찾는다.

```powershell
cargo build --release
cd gui
flutter run -d windows
```

**코드 서명이 없다.** 첫 실행 시 Windows SmartScreen이 "Windows에서 PC를 보호했습니다" 경고를 띄울 수 있다 — "추가 정보 → 실행"으로 넘어갈 수 있다. macOS의 Gatekeeper와 같은 이유(발급사 서명이 없음)다.

**실기 연결은 WinUSB 드라이버 바인딩이 먼저다.** 위 "USB 드라이버" 참조.

### Linux

*이 저장소에서 아직 빌드·실행이 검증되지 않았다.* 플랫폼 스캐폴딩(`gui/linux/`)은 생성돼 있으므로, 원칙적으로는 아래와 같이 진행하면 된다.

```bash
cargo build --release --features vendored
cd gui && flutter build linux --release
cp ../target/release/aw-tool build/linux/x64/release/bundle/aw-tool
```

(정확한 번들 경로는 Flutter/CMake 버전에 따라 달라질 수 있다.)

## 릴리즈

버전은 `scripts/bump-version.sh patch|minor|major`로 올린다 (`gui/pubspec.yaml`과 `Cargo.toml`을 함께 올리고 build number는 +1). PR을 병합한 뒤 `main`의 병합 커밋에 태그를 단다.

macOS/Windows 모두 패키징·GitHub 배포가 자동화돼 있다 — 로컬 스크립트로 개별
플랫폼을 낼 수도 있고, `v*.*.*` 태그를 push하면
[.github/workflows/release.yml](../.github/workflows/release.yml)이 macOS/Windows/Linux
세 산출물(`AllwinnerFlasher-<버전>-macos-arm64.dmg`, `-windows-x64-setup.exe`, `-linux-x64.tar.gz`)과 `SHA256SUMS.txt`를 병렬로 빌드해서 **하나의 GitHub 릴리즈로 묶어** 올린다 (하나라도
실패하면 릴리즈 자체가 생성되지 않는다). macOS는 CI에서 Developer ID로
서명·공증까지 마친다 (시크릿 등록 완료 — 자세한 내용은
[docs/release-ci.md](release-ci.md) 참고).

릴리즈 산출물은 CI만 만든다. 로컬에서는 `scripts/build-app.sh`(macOS) / `scripts/build-app-windows.ps1`(Windows)로 개발 빌드만 만든다 — v1.1.3까지 쓰던 `scripts/release.sh` / `release-windows.ps1`은 CI와 다른 이름의 산출물로 GitHub 릴리즈를 만들 수 있어서 conventions-v1 적용 때 지웠다.

### Gatekeeper

> CI가 만든 릴리즈 DMG는 Developer ID로 서명·공증돼 있어 아래 절차가 필요 없다. 이 절은 로컬 스크립트(`scripts/build-app.sh`)로 만든 ad-hoc 서명 앱에만 해당한다.

**이 앱은 Apple Developer ID로 서명·공증되지 않았다(ad-hoc 서명).** 내려받은 상태에서는 Gatekeeper가 실행을 막는다 — 실제로 확인했다:

```
$ spctl -a -vvv -t exec "Allwinner Flasher.app"
Allwinner Flasher.app: rejected
```

받는 쪽에서 첫 실행 전에 한 번 격리 속성을 지워야 한다.

```bash
xattr -dr com.apple.quarantine "/Applications/Allwinner Flasher.app"
```

제대로 공증하려면 Apple Developer Program(연 $99)에 가입해 Developer ID Application 인증서를 받고, `codesign --options runtime --timestamp`로 서명한 뒤 `notarytool submit --wait`과 `stapler staple`을 거쳐야 한다. 그때는 `build-app.sh`의 ad-hoc 서명(`--sign -`)을 인증서 이름으로 바꾸면 된다.

## GUI 연동 (`--json`)

모든 명령에 `--json`을 붙이면 산문 대신 **줄 단위 JSON**을 stdout으로 냅니다. 한 줄에 객체 하나, 각 객체에 `event` 키가 있습니다. GUI가 이 툴을 서브프로세스로 띄우고 stdout을 파싱하는 것을 전제로 한 출력입니다.

```
{"event":"step","step":"fel_found","message":"bootstrap: FEL device found (soc_id=0x1890)"}
{"event":"plan","partitions":12,"total_bytes":1150000000,"mbr_bytes":16384,...}
{"event":"partition_begin","index":6,"total":12,"name":"super","bytes":1021182504}
{"event":"progress","name":"super","written":104857600,"total":1024458752}
{"event":"partition_end","index":6,"total":12,"name":"super","format":"sparse","seconds":74.3}
{"event":"done"}
```

| 이벤트 | 용도 |
|---|---|
| `plan` | 진행바 전체 크기를 미리 잡을 수 있게 총 바이트 수를 먼저 알림 |
| `step` | 이름 붙은 단계 경계 (`fel_found`, `mbr`, `boot0`, `reboot` 등) |
| `partition_begin` / `partition_end` | 파티션 단위 경계 |
| `progress` | 파티션 **내부** 바이트 진행률 (100 ms 간격으로 스로틀) |
| `done` / `error` | 종료. `error`는 anyhow 컨텍스트 체인을 `causes` 배열로 함께 제공 |
| `probe`, `items`, `partitions` 등 | 조회 명령의 구조화된 결과 |

`progress`가 파티션 내부까지 내려가는 이유는 `super` 때문입니다. 약 1 GB를 64 KB 청크로 쓰기 때문에, 파티션 단위 이벤트만 있으면 전체 1분 40초 중 **1분 넘게 진행바가 한 칸에 멈춰** 있어 멈춘 것처럼 보입니다.

실패는 예외로 전파되지 않고 항상 마지막 줄의 `error` 이벤트로 나옵니다(종료 코드는 1). 프론트엔드가 stderr를 따로 파싱할 필요가 없습니다.

## 동작 방식

```
FEL (boot ROM)
  ├─ fes1.fex   (work_mode=0x10 패치) → 0x4C000 에 업로드·실행 → DRAM 초기화
  └─ u-boot.fex (work_mode=0x10 패치) → 0x4A000000 에 업로드·실행
       ↓ USB 재열거
EFEX
  ├─ erase 플래그 설정 → 파티션 전체 erase
  ├─ MBR/GPT 기록
  ├─ 파티션별 기록 (sparse 이미지는 확장하며 기록)
  ├─ BOOT1 (boot_package.fex) / BOOT0 (boot0_*.fex)
  └─ 재부팅
```

두 프라이머 모두 이미지에서 직접 꺼내 패치하므로 별도 파일 준비가 필요 없다. 외부 `sunxi-tools` 의존도 없다.

## 알아둘 점

**`super.fex`는 raw 이미지가 아니다.** Android sparse 컨테이너(매직 `0xED26FF3A`)라 확장해서 기록해야 한다. 그대로 쓰면 파티션 선두에 liblp 메타데이터 대신 sparse 헤더가 놓여, 커널은 부팅하지만 first-stage init이 dynamic partition을 찾지 못하고 마운트 전에 bootloader로 재부팅한다. 툴이 매직으로 자동 판별하지만, 새 파티션을 다룰 때는 파일명이 파티션 이름과 같더라도 컨테이너 여부를 먼저 확인할 것.

**주소 공간이 두 가지다.** 파티션 오프셋은 `addrlo` 공간과 GPT LBA 공간 두 가지로 표기되며 항상 `0xa000` 섹터(20 MB) 차이가 난다.

- `sys_partition.fex`와 이 툴의 섹터 인자 → **addrlo 공간** (예: super `0x90400`)
- U-Boot 셸의 `mmc read` / `part list` → **GPT LBA 공간** (예: super `0x9a400`)

실기를 덤프해 비교할 때 이 둘을 섞으면 엉뚱한 결론이 나온다.

**프라이머와 기록본은 다르다.** `work_mode`를 패치한 fes1/u-boot는 DRAM에만 올라가는 프라이머다. 저장소에 기록하는 BOOT0/BOOT1은 반드시 패치하지 않은 원본이어야 한다.

## T507로 옮길 때

파이프라인 자체는 공통이지만 아래는 SoC별로 다르므로 해당 SDK에서 다시 확인해야 한다.

- **프라이머 로드 주소** — T527은 fes1 `0x4C000`, u-boot `0x4A000000`. 근거는 `include/configs/<soc>.h`의 `CONFIG_FES1_RUN_ADDR`, `fes/fes1.lds`, `u-boot.lds` / `.config`의 `CONFIG_SYS_TEXT_BASE`. `--fes1-addr` / `--uboot-addr`로 지정할 수 있다.
  > 잘못된 주소(예: `sunxi-fel spl`이 가정하는 boot0용 `0x44000`)로 fes1을 올리면 보드가 응답하지 않는다. 전원 재인가로 복구된다.
- **BOOT0 변종** — `efex-query-storage`로 매체를 확인해 `boot0_sdcard.fex` / `boot0_nand.fex` 선택.

## 크레딧

- 앱 아이콘 원본: [Icons8](https://icons8.com) Sticker 글리프(`gui/assets/icon/source_glyph.svg`, 유료 플랜이라 앱 안 표기는 하지 않는다). `gui/tool/icon/generate_icons.py`가 [application-release-templates](https://github.com/jejezz/application-release-templates)의 공통 판에 올려 macOS·Windows·Linux 아이콘을 한 번에 만든다 (`cd gui && python3 tool/icon/generate_icons.py`, Pillow 필요).
- 글꼴: [서울남산체](https://www.seoul.go.kr/seoul/font.do) (서울특별시)
- USB: [libusb](https://libusb.info) (LGPL-2.1) — 앱의 "오픈소스 라이선스" 화면에 원문이 있다

## 문서

조사 과정 전체 — 프로토콜 근거(벤더 소스 위치), 실기 로그, 검증 방법, **배제한 가설들** — 은 [`docs/T527-T507-FEL-EFEX-기술조사.md`](T527-T507-FEL-EFEX-기술조사.md)에 있다. 새 SoC 대응이나 이상 동작을 쫓을 때 먼저 볼 것.

## 구성

| 파일 | 역할 |
|---|---|
| `src/imagewty.rs` | IMAGEWTY 컨테이너 파서 |
| `src/sunxi_head.rs` | `work_mode` 패치(offset 224) + `mksunxiboot` 체크섬 |
| `src/fel.rs` | FEL USB 프로토콜 |
| `src/efex.rs` | EFEX 프로토콜 (벤더 `usb_efex.h`/`.c` 기반) |
| `src/bootstrap.rs` | FEL→EFEX 자동 진입 |
| `src/sparse.rs` | Android sparse 이미지 파서 |
| `src/sys_partition.rs` | `sys_partition.fex` 파서 + 오프셋 계산 |
| `src/event.rs` | 진행 리포터 (산문 / NDJSON 양쪽) |
| `gui/lib/aw_tool.dart` | CLI 실행 + JSON 이벤트 파싱 |
| `gui/lib/flasher_model.dart` | 장치 폴링, 이미지 로딩, 플래싱 상태 |
| `gui/lib/main.dart` | UI |
| `gui/lib/l10n/*.arb` | 한국어 · 영어 문자열 (generated 파일은 커밋하지 않음) |
| `gui/lib/about/` | 정보 창 (공통 `about_dialog.dart` + 앱 문구 `allwinner_about.dart`), macOS 앱 메뉴, 추가 라이선스 등록 |
| `gui/lib/troubleshoot.dart` | Windows USB 드라이버 문제 해결 대화상자 |
| `gui/lib/settings/` | 테마·언어 설정 저장과 전환 메뉴, 예전 언어 파일 이전 |
| `scripts/bump-version.sh` | 릴리즈 버전 올림 (`gui/pubspec.yaml` + `Cargo.toml`) |
| `installer/windows/app.iss` | Windows Inno Setup 스크립트 ([windows-installer.md](windows-installer.md)) |
| `tool/readme/` | README 스크린샷·검사 도구 (application-release-templates) |
| `scripts/build-app.sh` | macOS `.app` 빌드 + helper 동봉 + 재서명 |
| `scripts/build-app-windows.ps1` | Windows 빌드 + helper(`aw-tool.exe`) 동봉 |
| `.github/workflows/release.yml` | 태그 push 시 macOS(서명·공증)/Windows/Linux를 병렬 빌드해 하나의 GitHub 릴리즈로 게시 ([docs/release-ci.md](release-ci.md)) |
| `gui/tool/icon/generate_icons.py` | 원본 글리프로 macOS·Windows·Linux 앱 아이콘 생성 |
