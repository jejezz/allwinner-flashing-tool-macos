# GitHub Actions 릴리즈 워크플로

`scripts/release.sh`(macOS)와 `scripts/release-windows.ps1`(Windows)를 로컬이 아니라
GitHub Actions에서 실행해서, 태그를 하나 push하면 두 플랫폼 산출물이 자동으로
같은 GitHub 릴리즈에 올라가게 한 것이 이 문서가 다루는 워크플로다.

| 파일 | 역할 |
|---|---|
| [`.github/workflows/release-macos.yml`](../.github/workflows/release-macos.yml) | `scripts/release.sh`를 macOS 러너에서 실행 |
| [`.github/workflows/release-windows.yml`](../.github/workflows/release-windows.yml) | `scripts/release-windows.ps1`를 Windows 러너에서 실행 |

새 로직을 워크플로 YAML 안에 다시 짜지 않고, 로컬에서 그대로 쓰던 스크립트를
CI에서도 그대로 호출한다 — 로컬에서 확인한 동작과 CI의 동작이 갈라지지 않는다.

## 등록해야 할 것

**시크릿 없음.** 이 프로젝트는 지금 ad-hoc 서명만 하고(macOS: `codesign --sign -`,
Windows: 서명 없음), Apple Developer ID 서명·공증이나 Windows 코드 서명 인증서를
쓰지 않는다. 따라서 GitHub Secrets에 등록할 것이 없고, 두 워크플로 모두 저장소가
기본 제공하는 `GITHUB_TOKEN`만으로 동작한다.

`GITHUB_TOKEN`으로 태그를 push하고 릴리즈를 만들려면 쓰기 권한이 있어야 하는데,
저장소 Settings에서 "Read and write permissions"를 켜 두는 대신 각 워크플로
파일에 `permissions: contents: write`를 직접 선언해 뒀다 — 저장소 기본 설정을
바꿀 필요가 없다.

## 실행 방법

**(1) `v*` 태그 push — 자동**

```bash
git tag v0.1.2
git push origin v0.1.2
```

두 워크플로가 모두 시작되고, 각각 `--publish` 없이 실행되므로(초안) 결과를 확인한
뒤 GitHub 웹에서 "Publish release"를 눌러야 공개된다.

> **주의**: `push.tags` 트리거는 **태그가 가리키는 커밋에 들어 있는 워크플로
> 파일**을 기준으로 동작한다. 이 워크플로가 아직 없던 시점의 커밋에 태그를
> 찍었다면 아무 것도 실행되지 않는다 — 이 변경이 `main`에 머지된 뒤의 커밋에
> 새로 태그를 찍어야 한다.

**(2) Actions 탭에서 수동 실행 — `workflow_dispatch`**

GitHub 저장소 → Actions 탭 → `Release · macOS` 또는 `Release · Windows` → **Run
workflow**. `tag`(필수, 예: `v0.1.2`)를 입력하고, 초안 없이 바로 공개하려면
`publish` 체크박스를 켠다. 태그가 아직 없으면 각 스크립트가 실행 중에 만들어
push한다 — `scripts/release.sh` / `scripts/release-windows.ps1`가 로컬에서 하던
것과 동일하다.

macOS/Windows를 각각 따로 수동 실행할 수도 있다 (예: Windows 인스톨러만 다시
올리고 싶을 때).

**태그를 찍기 전에** `Cargo.toml`과 `gui/pubspec.yaml`의 버전을 올려 둘 것 —
태그는 GitHub 릴리즈 이름과 (Windows 인스톨러의 표시 버전)만 정하고, Flutter/Rust
빌드 자체의 버전 문자열은 정하지 않는다.

## 두 워크플로가 서로 기다리지 않는 이유

같은 태그 push 한 번에 macOS/Windows 워크플로가 동시에 시작되는데, 두 러너가
언제 끝날지는 보장되지 않는다. 먼저 `gh release create`에 성공한 쪽이 그 릴리즈의
노트(설명)를 쓰고, 나중에 끝난 쪽은 `gh release view`로 릴리즈가 이미 있는 것을
확인하고 자기 산출물만 업로드한다 (`scripts/release-windows.ps1`의 30초 대기 후
재확인 로직 참고). 아직 릴리즈가 없으면 그 플랫폼만의 릴리즈를 만든다 — Windows를
먼저/단독으로 내는 경우([windows-installer.md](windows-installer.md) 참고)와 같은
분기다.

## 트러블슈팅

| 증상 | 원인 | 대처 |
|---|---|---|
| macOS 잡이 "working tree is dirty"로 실패 | `actions/checkout` 이후 무언가 트래킹된 파일을 건드림 | 원인이 된 스텝을 찾아 제거하거나 `.gitignore`에 추가 |
| Windows 잡이 "ISCC.exe not found" | `choco install innosetup` 스텝이 실패했거나 건너뜀 | 워크플로 로그에서 해당 스텝 확인 |
| `gh release create`가 두 잡 모두에서 실패(경합) | 매우 드물게 두 잡이 거의 동시에 릴리즈 생성을 시도 | 재실행하면 한쪽은 이미 만들어진 릴리즈를 찾아 업로드로 넘어간다 |
| `v*` 태그를 push했는데 Actions에 아무 실행도 안 뜸 | 태그가 가리키는 커밋에 이 워크플로 자체가 없음 | 이 변경이 `main`에 머지된 뒤의 커밋에 새로 태그를 찍는다 |
| Rust `vendored` 빌드 실패 (libusb 컴파일 에러) | 러너 이미지가 바뀌어 C 컴파일러/빌드 도구가 없어짐 | macOS는 Xcode Command Line Tools, Windows는 Visual Studio Build Tools가 러너 기본 이미지에 있는지 확인 (`windows-latest`/`macos-26`은 기본 포함) |

## 나중에 서명·공증이 필요해지면

- **macOS**: Apple Developer ID Application 인증서를 발급받아
  `scripts/build-app.sh`의 `codesign --sign -`를 `codesign --sign "<Developer ID
  Application: ...>"`로 바꾸고, `xcrun notarytool submit` 단계를 추가한다. 인증서와
  App Store Connect API Key는 CI 환경에 시크릿으로 주입해야 한다 (예: base64로
  인코딩한 `.p12`).
- **Windows**: Authenticode 코드 서명 인증서(`.pfx`)를 발급받아
  `scripts/release-windows.ps1`의 ISCC 컴파일 이후 `signtool sign /f cert.pfx
  /p <password> /fd sha256 /tr <timestamp-server> /td sha256`을 인스톨러 `.exe`에
  실행한다. 인증서는 base64로 인코딩해 시크릿으로 주입하고, CI에서 파일로 복원한
  뒤 사용한다.

두 경우 모두 인증서 발급·비용은 사람이 결정할 문제라 이 워크플로는 건드리지
않았다 — 실제로 필요해지는 시점에 그때 기준으로 다시 설계하는 편이 낫다.
