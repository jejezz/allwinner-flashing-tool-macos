# GitHub Actions 릴리즈 워크플로

`v*.*.*` 태그를 push하면 macOS/Windows/Linux 세 산출물을 병렬로 빌드해서 하나의
GitHub 릴리즈로 묶어 올리는 워크플로다.
[`.github/workflows/release.yml`](../.github/workflows/release.yml) 파일
하나로 구성돼 있으며,
[application-release-templates](https://github.com/jejezz/application-release-templates)
저장소 `desktop/` 템플릿(conventions-v1)을
이 저장소의 실제 빌드 시스템에 맞게 적용한 것이다 — 순수 Flutter 앱을 가정한
템플릿과 달리, 이 저장소는 Rust CLI 헬퍼(`aw-tool`)를 각 플랫폼 빌드에 동봉해야
하고, GUI가 저장소 루트가 아니라 `gui/`에 있다.

| 잡 | 내용 |
|---|---|
| `check` | 태그 = `gui/pubspec.yaml` 버전 = `Cargo.toml` 버전인지, `app_identity.dart`의 표시 이름이 `PRODUCT_NAME`과 같은지, README가 규약을 따르는지, 번역이 빠짐없는지 확인 |
| `build-macos` | `cargo build --features vendored` → `flutter build macos` → 헬퍼 동봉 → Developer ID 서명 → DMG(`AllwinnerFlasher-<버전>-macos-arm64.dmg`) → 공증 |
| `build-windows` | `cargo build --features vendored` → `flutter build windows` → 헬퍼 동봉 → Inno Setup(`installer/windows/app.iss`) → `AllwinnerFlasher-<버전>-windows-x64-setup.exe` |
| `build-linux` | `cargo build`(vendored 아님 — [aw-tool.md](aw-tool.md) "Linux" 참고) → `flutter build linux` → 헬퍼·아이콘·`.desktop`·`install.sh` 동봉 → `AllwinnerFlasher-<버전>-linux-x64.tar.gz` |
| `release` | 태그 push일 때만, 위 세 잡이 **모두** 성공한 뒤 실행 — 아티팩트와 `SHA256SUMS.txt`를 모아 `.github/release-notes-header.md` + 자동 생성 노트로 한 번에 공개 |

`workflow_dispatch`(Actions 탭의 "Run workflow")로 돌리면 `check`와 세 빌드만
돌고 릴리스는 만들지 않는다 — 워크플로를 고친 뒤 태그 전에 한 번 돌려 본다.
Flutter 버전은 `FLUTTER_VERSION`(3.47.1)으로 고정돼 있다.

**하나라도 실패하면 릴리즈 자체가 생성되지 않는다.** 플랫폼별로 따로 릴리즈를
만들지 않기 때문에 일부 자산만 올라간 릴리즈가 남는 일이 없다.

**릴리즈는 초안 없이 바로 공개된다.** 릴리즈 산출물은 이 워크플로만 만든다 — 로컬의
`scripts/build-app.sh` / `scripts/build-app-windows.ps1`은 개발 빌드용이다 (v1.1.3까지
있던 `scripts/release.sh` / `release-windows.ps1`은 CI와 다른 이름으로 릴리즈를 만들 수
있어 지웠다).

## 필요한 시크릿

| 플랫폼 | 시크릿 | 비고 |
|---|---|---|
| macOS | `MACOS_CERTIFICATE_P12_BASE64` | Developer ID Application 인증서를 `.p12`로 내보낸 뒤 `base64 -i cert.p12 \| pbcopy` |
| macOS | `MACOS_CERTIFICATE_PASSWORD` | 위 `.p12` 내보낼 때 지정한 암호 |
| macOS | `MACOS_KEYCHAIN_PASSWORD` | CI가 빌드 중에만 쓰는 임시 키체인 암호 — 아무 문자열이나 새로 만들어서 등록 |
| macOS | `APPLE_ID` | 노터라이즈용 Apple ID 이메일 |
| macOS | `APPLE_ID_PASSWORD` | **앱 암호(app-specific password)** — [appleid.apple.com](https://appleid.apple.com)에서 발급. 계정 비밀번호 아님 |
| macOS | `APPLE_TEAM_ID` | [developer.apple.com/account](https://developer.apple.com/account) → Membership details의 10자리 Team ID |
| Windows | 없음 | 서명하지 않는 인스톨러 — SmartScreen이 "확인되지 않은 게시자" 경고를 띄우지만 "추가 정보 → 실행"으로 넘어갈 수 있음 |
| Linux | 없음 | tarball만 생성, 서명 개념 자체가 없음 |

**위 macOS 시크릿 6개는 이미 저장소에 등록돼 있다** (`gh secret list`로 확인함,
2026-09-21). 태그를 push하면 `build-macos`가 실제 Developer ID로 서명하고
공증까지 마친 DMG를 만든다 — 로컬의 `scripts/build-app.sh`는
여전히 ad-hoc(`codesign --sign -`)로만 서명하지만, CI 산출물은 그것과 다르다.

인증서가 만료되거나 바뀌는 등 문제가 생겨 서명을 잠시 끄고 싶다면,
`release.yml`의 `build-macos` 잡에서 "Import Developer ID certificate", "Sign
app", "Notarize DMG" 세 스텝을 지우면 된다 — `flutter build macos --release`가
만드는 ad-hoc 서명 그대로 DMG를 만들어 올리고, 받는 사람 Mac에서는 Gatekeeper가
"확인되지 않은 개발자" 경고를 띄운다.

## 실행 방법

```bash
git tag v0.1.2
git push origin v0.1.2
```

`v*.*.*` 형태의 태그를 push하면 자동으로 시작된다. 수동 실행(`workflow_dispatch`)은
지원하지 않는다 — 템플릿과 동일하게, 세 플랫폼 잡을 하나의 태그 이벤트로 묶어서
`release` 잡이 정확히 그 태그에 대해서만 한 번 돌게 하기 위함이다.

**태그를 찍기 전에** `Cargo.toml`과 `gui/pubspec.yaml`의 버전을 올려 둘 것 —
태그는 GitHub 릴리즈 이름과 (macOS DMG/Windows 인스톨러의 표시 버전)만
정하고, Flutter/Rust 빌드 자체의 버전 문자열은 정하지 않는다.

> **주의**: `push.tags` 트리거는 **태그가 가리키는 커밋에 들어 있는 워크플로
> 파일**을 기준으로 동작한다. 이 워크플로가 아직 없던 시점의 커밋에 태그를
> 찍었다면 아무 것도 실행되지 않는다 — `main`에 머지된 뒤의 커밋에 새로
> 태그를 찍어야 한다.

## 트러블슈팅

| 증상 | 원인 | 대처 |
|---|---|---|
| `build-macos`가 "Import Developer ID certificate"에서 실패 | 시크릿이 비었거나, `.p12`/암호가 서로 안 맞거나, 인증서가 만료됨 | `gh secret list`로 6개가 다 있는지 확인, `.p12`를 다시 내보내 base64로 재등록. 급하면 위 "필요한 시크릿" 마지막 문단대로 서명 관련 3스텝을 지운다 |
| `notarytool submit`이 "Invalid credentials" | `APPLE_ID_PASSWORD`가 일반 계정 비밀번호이거나 앱 암호가 만료/폐기됨 | appleid.apple.com에서 앱 암호를 새로 발급해서 재등록 |
| 노터라이즈 상태가 `Accepted`가 아니라 `Invalid` | 서명 자체는 됐지만 Apple이 내용을 거부함(하드닝/시크릿 타임스탬프 등) — 이제 이 워크플로가 자동으로 실패 처리하고 바로 다음 "Show notarization log" 스텝이 사유를 출력한다 | 그 스텝의 로그(`xcrun notarytool log`)를 읽고 원인 수정. 예: 번들에 얹은 loose 실행 파일(`aw-tool`)은 `--deep` 서명만으로는 secure timestamp가 안 붙을 수 있어 별도로 서명해 둠(Sign app 스텝 참고) |
| `build-linux`가 실패해서 릴리즈 자체가 안 생김 | Linux 빌드가 이 저장소에서 아직 검증된 적 없음 | Actions 로그로 원인 확인. 급하면 `release` 잡의 `needs:`에서 `build-linux`를 빼고 재실행 |
| Windows 잡이 "ISCC.exe not found" 관련 에러 | `choco install innosetup` 스텝 실패, 또는 설치 경로가 워크플로가 가정한 `C:\Program Files (x86)\Inno Setup 6\`와 다름 | 워크플로 로그에서 해당 스텝 확인 |
| Windows 잡이 ISCC에서 "You may not specify more than one script filename." | Git Bash(MSYS)가 `/DMyAppVersion=...`처럼 `/`로 시작하는 인자를 Windows 경로로 잘못 변환함 | `Package installer` 스텝에 이미 `MSYS_NO_PATHCONV: 1`을 넣어 뒀다 — 이 스텝을 손대다 지웠다면 다시 넣는다 |
| `v*.*.*` 태그를 push했는데 Actions에 아무 실행도 안 뜸 | 태그가 가리키는 커밋에 이 워크플로 자체가 없음 | 이 변경이 `main`에 머지된 뒤의 커밋에 새로 태그를 찍는다 |
| Rust `vendored`(macOS/Windows) 또는 일반(Linux) 빌드 실패 | 러너 이미지가 바뀌어 C 컴파일러/빌드 도구가 없어짐 | macOS는 Xcode Command Line Tools, Windows는 Visual Studio Build Tools, Linux는 `libusb-1.0-0-dev`가 있는지 확인 |

## 나중에 Windows 코드 서명이 필요해지면

fastlane 생태계가 Windows를 사실상 지원하지 않으므로, 코드 서명
인증서(EV 인증서 권장 — SmartScreen 평판이 즉시 쌓임)로 `signtool.exe`를
직접 호출하는 스텝을 `build-windows` 잡에 추가해야 한다. 지금은 이게
필요해지는 시점에, 그때 쓸 인증서를 기준으로 다시 설계하는 편이 낫다.
