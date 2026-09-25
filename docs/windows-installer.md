# Windows 인스톨러 만들기

`scripts/build-app-windows.ps1`은 `gui\build\windows\x64\runner\Release\`에 실행 파일 폴더를 만들 뿐, 더블클릭 한 번으로 설치되는 인스톨러(`setup.exe`)는 만들지 않는다. 그 폴더를 Inno Setup 인스톨러로 패키징하는 절차를 정리한 것이 이 문서다. 보통은 태그를 push해서 GitHub Actions로 만들고([release-ci.md](release-ci.md)), 손으로 할 때는 `scripts/release-windows.ps1`(아래 단계를 자동화한 것)을 쓴다.

**빌드 스크립트는 이미 저장소에 있다** — [`installer/windows/app.iss`](../installer/windows/app.iss). [application-release-templates](https://github.com/jejezz/application-release-templates) `desktop/installer/windows/app.iss`(conventions-v1)를 이 저장소 경로(`gui/`)에 맞춘 것이고, `AllwinnerFlasher-<version>-windows-x64-setup.exe`를 만든다. (v1.1.3까지 있던 WiX `.wxs`는 한 번도 컴파일해 보지 못해 규약에 따라 지웠다.)

서명 인증서 없이 만드므로, macOS의 Gatekeeper처럼 Windows SmartScreen이 처음 실행 시 "Windows에서 PC를 보호했습니다" 경고를 띄운다. "추가 정보 → 실행"으로 넘어갈 수 있다.

## 0. 먼저 릴리즈 빌드를 만든다

```powershell
.\scripts\build-app-windows.ps1
```

`gui\build\windows\x64\runner\Release\`에 `aw_flasher.exe`, `aw-tool.exe`, `flutter_windows.dll`, `data\`가 갖춰진 상태가 되어야 한다. 인스톨러는 이 폴더 전체를 그대로 담는다.

## 1. Inno Setup

### 준비

[Inno Setup](https://jrsoftware.org/isdl.php)을 설치한다 (무료).

```powershell
winget install JRSoftware.InnoSetup
```

### 스크립트

이미 저장소에 있다 — [`installer/windows/app.iss`](../installer/windows/app.iss). 버전·이름·실행 파일 이름은 컴파일할 때 `/D` 옵션으로 넘기므로 파일을 고칠 일이 없다 (넘기지 않으면 버전이 `0.0.0`으로 찍혀 손으로 만든 빌드임이 드러난다).

`AppId`(고유 GUID)는 그 안에 이미 고정되어 있다 — 앱을 갈아엎는 게 아니라면 절대 바꾸지 않는다. 바꾸면 Windows가 이전 버전과 다른 앱으로 인식해서, 제어판 "프로그램 추가/제거"에서 업그레이드/제거가 깨진다.

### 컴파일

```powershell
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" /DMyAppVersion=1.1.4 /DMyAppNumericVersion=1.1.4 installer\windows\app.iss
```

`dist\AllwinnerFlasher-<version>-windows-x64-setup.exe`가 만들어진다.

### 설치 테스트

더블클릭해서 마법사가 정상적으로 뜨는지 확인한다. 사람 손 안 대고 빠르게 확인하려면:

```powershell
.\dist\AllwinnerFlasher-1.1.4-windows-x64-setup.exe /VERYSILENT /DIR="$env:TEMP\aw-test-install"
& "$env:TEMP\aw-test-install\aw_flasher.exe"
```

앱이 뜨고 (보드가 연결되어 있다면) 잘 인식하는지 확인한 뒤, 테스트 설치 폴더는 지운다.

## GitHub 릴리즈에 올리기

빌드한 인스톨러(`.exe`)를 macOS 릴리즈와 같은 태그에 올린다. macOS `scripts/release.sh`는 macOS 산출물만 다루므로, Windows 인스톨러는 아래처럼 수동으로 추가한다.

**GitHub CLI로 (권장 — 설치돼 있지 않으면 `winget install GitHub.cli`)**

macOS `release.sh`가 이미 그 버전의 릴리즈를 만들어 놓은 상태라면:

```powershell
gh release upload v0.1.0 dist\AllwinnerFlasher-1.1.4-windows-x64-setup.exe
```

아직 릴리즈 자체가 없다면 (Windows만 먼저 낼 때): (gh install -> winget install GitHub.cli)

```powershell
gh release create v0.1.0 dist\AllwinnerFlasher-1.1.4-windows-x64-setup.exe --title v0.1.0 --draft
```

`--draft`를 빼면 바로 공개된다. 초안으로 만들었다면 GitHub 웹에서 내용을 확인한 뒤 "Publish release"를 누른다.

체크섬도 같이 올려두면 좋다 (macOS 릴리즈의 `SHA256SUMS`와 같은 방식):

```powershell
Get-FileHash dist\AllwinnerFlasher-1.1.4-windows-x64-setup.exe -Algorithm SHA256 |
    ForEach-Object { "$($_.Hash.ToLower())  $(Split-Path $_.Path -Leaf)" } |
    Out-File -Append dist\SHA256SUMS -Encoding ascii
gh release upload v0.1.0 dist\SHA256SUMS --clobber
```

**GitHub 웹 UI로 (gh 설치 없이)**

1. https://github.com/jejezz/allwinner-flashing-tool-macos/releases 에서 해당 태그의 릴리즈를 연다 (없으면 "Draft a new release").
2. "Attach binaries" 영역에 `dist\AllwinnerFlasher-1.1.4-windows-x64-setup.exe`를 드래그 앤 드롭.
3. 초안이면 "Publish release".

## 배포 노트에 적어둘 것

macOS 릴리즈 노트가 격리 해제(`xattr`) 안내를 맨 앞에 두는 것처럼, Windows 릴리즈 노트에도 SmartScreen 경고가 뜬다는 것과 "추가 정보 → 실행"으로 넘어가면 된다는 것, 그리고 실기기를 인식시키려면 Zadig로 WinUSB 바인딩이 필요하다는 것([README.md](../README.md)의 "USB 드라이버" 참조)을 적어두는 게 좋다 — 둘 다 처음 쓰는 사람이 "고장났다"고 오해하기 쉬운 지점이다.
