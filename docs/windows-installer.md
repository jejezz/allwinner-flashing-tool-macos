# Windows 인스톨러 만들기

`scripts/build-app-windows.ps1`은 `gui\build\windows\x64\runner\Release\`에 실행 파일 폴더를 만들 뿐, 더블클릭 한 번으로 설치되는 인스톨러(`setup.exe`)나 `.msi`는 만들지 않는다. 그 폴더를 실제 배포용 인스톨러로 패키징해서 GitHub 릴리즈에 올리는 절차를 수동으로 정리한 것이 이 문서다 — 자동화 스크립트는 일부러 만들지 않았다. 새 버전을 낼 때마다 아래 단계를 직접 따라간다.

**빌드 스크립트는 이미 저장소에 있다.** 새로 만들 필요 없이, 아래 두 파일을 그대로 쓴다.

| 파일 | 용도 |
|---|---|
| [`installer/windows/aw-flasher.iss`](../installer/windows/aw-flasher.iss) | Inno Setup 스크립트 — `AllwinnerFlasherSetup-<version>.exe` 생성 |
| [`installer/windows/aw-flasher.wxs`](../installer/windows/aw-flasher.wxs) | WiX 스크립트 — `AllwinnerFlasher-<version>.msi` 생성 |

| | Inno Setup (`aw-flasher.iss`) | WiX Toolset (`aw-flasher.wxs`) |
|---|---|---|
| 결과물 | `.exe` 인스톨러 | 진짜 `.msi` |
| 이 문서에서의 검증 상태 | **실제로 컴파일·설치·실행까지 확인함** (2026-09-19, 이 저장소의 릴리즈 빌드로) | 컴파일이 **EULA 동의 절차에 막혀 못 해봄** — 아래 "방법 2" 참조 |
| 언제 쓰나 | 일반적인 배포 — 특별한 이유가 없으면 이거 | Group Policy/Intune 같은 기업 배포 도구가 `.msi`를 요구할 때 |

특별한 이유가 없다면 **Inno Setup**을 쓴다.

둘 다 서명 인증서 없이(ad-hoc) 만드므로, macOS의 Gatekeeper처럼 Windows SmartScreen이 처음 실행 시 "Windows에서 PC를 보호했습니다" 경고를 띄운다. "추가 정보 → 실행"으로 넘어갈 수 있다 — 이 저장소가 배포하는 원본 `.exe`와 같은 사정이다 ([README.md](../README.md)의 "GUI → Windows" 참조).

## 0. 먼저 릴리즈 빌드를 만든다

```powershell
.\scripts\build-app-windows.ps1
```

`gui\build\windows\x64\runner\Release\`에 `aw_flasher.exe`, `aw-tool.exe`, `flutter_windows.dll`, `data\`가 갖춰진 상태가 되어야 한다. 인스톨러는 이 폴더 전체를 그대로 담는다.

## 방법 1: Inno Setup

### 준비

[Inno Setup](https://jrsoftware.org/isdl.php)을 설치한다 (무료).

```powershell
winget install JRSoftware.InnoSetup
```

### 스크립트

이미 저장소에 있다 — [`installer/windows/aw-flasher.iss`](../installer/windows/aw-flasher.iss). 새로 만들 필요 없다. 새 버전을 낼 때는 그 파일 맨 위의 `MyAppVersion`을 `gui/pubspec.yaml`의 `version:`과 맞춰서 바꾸기만 하면 된다 (예: `0.1.0` → `0.2.0`).

`AppId`(고유 GUID)는 그 안에 이미 고정되어 있다 — 앱을 갈아엎는 게 아니라면 절대 바꾸지 않는다. 바꾸면 Windows가 이전 버전과 다른 앱으로 인식해서, 제어판 "프로그램 추가/제거"에서 업그레이드/제거가 깨진다.

### 컴파일

```powershell
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" installer\windows\aw-flasher.iss
```

`dist\AllwinnerFlasherSetup-<version>.exe`가 만들어진다.

### 설치 테스트

더블클릭해서 마법사가 정상적으로 뜨는지 확인한다. 사람 손 안 대고 빠르게 확인하려면:

```powershell
.\dist\AllwinnerFlasherSetup-0.1.0.exe /VERYSILENT /DIR="$env:TEMP\aw-test-install"
& "$env:TEMP\aw-test-install\aw_flasher.exe"
```

앱이 뜨고 (보드가 연결되어 있다면) 잘 인식하는지 확인한 뒤, 테스트 설치 폴더는 지운다.

## 방법 2: WiX Toolset로 진짜 `.msi` 만들기

기업 배포 도구가 `.msi`를 요구하는 경우가 아니면 위 Inno Setup으로 충분하다.

### 준비

.NET SDK가 있어야 한다 (`dotnet --version`으로 확인). 그 위에 WiX를 전역 도구로 설치한다.

```powershell
dotnet tool install --global wix
```

### 스크립트

이미 저장소에 있다 — [`installer/windows/aw-flasher.wxs`](../installer/windows/aw-flasher.wxs). 새 버전을 낼 때는 `<Package>`의 `Version="0.1.0"`을 `gui/pubspec.yaml`과 맞춰서 바꾼다. `UpgradeCode`와 `ShortcutComponent`의 `Guid`는 `AppId`와 같은 이유로 고정값이니 바꾸지 않는다.

`<Files Include>`가 `INSTALLFOLDER` 아래 파일들을 자동으로 하베스팅해서 컴포넌트 그룹을 만들어준다 (WiX v3의 `heat.exe` 없이도 됨).

### 빌드 — 여기서 막힌다: EULA 동의가 필요하다

```powershell
wix build installer\windows\aw-flasher.wxs -arch x64 -out dist\AllwinnerFlasher-0.1.0.msi
```

이 저장소에서 실제로 실행해보면 컴파일이 안 되고 아래 에러가 난다:

```
wix.exe : error WIX7015: You must accept the Open Source Maintenance Fee (OSMF) EULA to use WiX Toolset v7.
```

WiX v6부터 "Open Source Maintenance Fee"라는 게 생겼다 — 연 매출이 특정 기준(현재 $10,000)을 넘는 조직은 WiX 유지보수를 후원해야 하고, v7부터는 그 EULA에 동의하지 않으면 빌드 자체가 막힌다. **이건 라이선스/비용에 관한 결정이라 AI가 대신 동의하지 않았다** — 자세한 내용은 [wixtoolset.org/osmf](https://wixtoolset.org/osmf/)에서 직접 확인하고, 동의할지 결정하는 것은 사용자의 몫이다.

검토 후 진행하기로 했다면, 매번 묻지 않게 한 번만 동의해두는 방법:

```powershell
wix eula accept wix7
```

또는 빌드 스크립트/CI에서 매번 명시하는 방법:

```powershell
wix build installer\windows\aw-flasher.wxs -acceptEula wix7 -arch x64 -out dist\AllwinnerFlasher-0.1.0.msi
```

**그래서 이 방법은 이 문서에서 실제로 컴파일까지 확인하지 못했다.** 스크립트 자체(`<Files Include>` 하베스팅 문법 등)가 설치된 WiX 버전과 맞는지는 EULA에 동의하고 직접 빌드해봐야 확인된다. 막히면 [WiX 문서](https://wixtoolset.org/docs/)를 참고한다.

## GitHub 릴리즈에 올리기

빌드한 인스톨러(`.exe` 또는 `.msi`)를 macOS 릴리즈와 같은 태그에 올린다. macOS `scripts/release.sh`는 macOS 산출물만 다루므로, Windows 인스톨러는 아래처럼 수동으로 추가한다.

**GitHub CLI로 (권장 — 설치돼 있지 않으면 `winget install GitHub.cli`)**

macOS `release.sh`가 이미 그 버전의 릴리즈를 만들어 놓은 상태라면:

```powershell
gh release upload v0.1.0 dist\AllwinnerFlasherSetup-0.1.0.exe
```

아직 릴리즈 자체가 없다면 (Windows만 먼저 낼 때): (gh install -> winget install GitHub.cli)

```powershell
gh release create v0.1.0 dist\AllwinnerFlasherSetup-0.1.0.exe --title v0.1.0 --draft
```

`--draft`를 빼면 바로 공개된다. 초안으로 만들었다면 GitHub 웹에서 내용을 확인한 뒤 "Publish release"를 누른다.

체크섬도 같이 올려두면 좋다 (macOS 릴리즈의 `SHA256SUMS`와 같은 방식):

```powershell
Get-FileHash dist\AllwinnerFlasherSetup-0.1.0.exe -Algorithm SHA256 |
    ForEach-Object { "$($_.Hash.ToLower())  $(Split-Path $_.Path -Leaf)" } |
    Out-File -Append dist\SHA256SUMS -Encoding ascii
gh release upload v0.1.0 dist\SHA256SUMS --clobber
```

**GitHub 웹 UI로 (gh 설치 없이)**

1. https://github.com/jejezz/allwinner-flashing-tool-macos/releases 에서 해당 태그의 릴리즈를 연다 (없으면 "Draft a new release").
2. "Attach binaries" 영역에 `dist\AllwinnerFlasherSetup-0.1.0.exe`를 드래그 앤 드롭.
3. 초안이면 "Publish release".

## 배포 노트에 적어둘 것

macOS 릴리즈 노트가 격리 해제(`xattr`) 안내를 맨 앞에 두는 것처럼, Windows 릴리즈 노트에도 SmartScreen 경고가 뜬다는 것과 "추가 정보 → 실행"으로 넘어가면 된다는 것, 그리고 실기기를 인식시키려면 Zadig로 WinUSB 바인딩이 필요하다는 것([README.md](../README.md)의 "USB 드라이버" 참조)을 적어두는 게 좋다 — 둘 다 처음 쓰는 사람이 "고장났다"고 오해하기 쉬운 지점이다.
