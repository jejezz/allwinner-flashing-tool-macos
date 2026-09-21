# GitHub Actions 릴리즈 워크플로

`v*.*.*` 태그를 push하면 macOS/Windows/Linux 세 산출물을 병렬로 빌드해서 하나의
GitHub 릴리즈로 묶어 올리는 워크플로다.
[`.github/workflows/release.yml`](../.github/workflows/release.yml) 파일
하나로 구성돼 있으며,
[application-release-templates](https://github.com/jejezz/application-release-templates)
저장소 `desktop/` 템플릿(portside-flutter에서 서명·공증까지 실제 검증됨)을
이 저장소의 실제 빌드 시스템에 맞게 적용한 것이다 — 순수 Flutter 앱을 가정한
템플릿과 달리, 이 저장소는 Rust CLI 헬퍼(`aw-tool`)를 각 플랫폼 빌드에 동봉해야
하고, GUI가 저장소 루트가 아니라 `gui/`에 있다.

| 잡 | 내용 |
|---|---|
| `build-macos` | `cargo build --features vendored` → `flutter build macos` → 헬퍼 동봉 → Developer ID 서명 → DMG 패키징 → 공증 |
| `build-windows` | `cargo build --features vendored` → `flutter build windows` → 헬퍼 동봉 → Inno Setup(`installer/windows/aw-flasher.iss`)으로 인스톨러 컴파일 |
| `build-linux` | `cargo build`(vendored 아님 — README.md "Linux" 참고) → `flutter build linux` → 헬퍼 동봉 → tarball |
| `release` | 위 세 잡이 **모두** 성공한 뒤에만 실행 — 아티팩트를 모아 `gh release create --generate-notes`로 한 번에 공개 |

**하나라도 실패하면 릴리즈 자체가 생성되지 않는다.** 플랫폼별로 따로 릴리즈를
만들지 않기 때문에 일부 자산만 올라간 릴리즈가 남는 일이 없다 — 대신 Linux
빌드가 실패하면 이미 잘 도는 macOS/Windows 산출물도 릴리즈되지 않는다는 뜻이다.
이 저장소의 Linux 빌드는 [README.md](../README.md)에 아직 "미검증"이라고
적혀 있으므로, 태그를 처음 push했을 때 `build-linux`가 실패해 릴리즈가 막힐 수
있다 — 그 경우 Actions 로그로 원인을 고치거나, 급하면 `release` 잡의
`needs:`에서 `build-linux`를 빼고 릴리즈만 macOS/Windows로 먼저 낸 뒤 Linux는
따로 처리하는 것도 방법이다.

**릴리즈는 초안 없이 바로 공개된다** (`gh release create`에 `--draft` 없음,
템플릿 그대로). 이전 버전의 이 워크플로(2파일 구성, `scripts/release.sh`를
CI에서 그대로 호출)와 달리 이번 버전은 로컬 스크립트를 호출하지 않고 YAML
안에 직접 빌드·패키징 로직을 담고 있다 — `scripts/release.sh` /
`scripts/release-windows.ps1`은 여전히 로컬에서 손으로 릴리즈할 때 쓸 수
있지만(플랫폼별로 개별 릴리즈를 만듦), 태그를 push했을 때 도는 CI 경로와는
별개다.

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
공증까지 마친 DMG를 만든다 — 로컬의 `scripts/build-app.sh`/`scripts/release.sh`는
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
