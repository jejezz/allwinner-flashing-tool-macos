## 설치 / Install

| OS | 받을 파일 / File |
|---|---|
| macOS (12.0 이상, Apple Silicon) | `*-macos-arm64.dmg` — 열어서 앱을 Applications로 끌어다 놓으세요 |
| Windows 10/11 (x64) | `*-windows-x64-setup.exe` |
| Linux (x64) | `*-linux-x64.tar.gz` — 압축을 풀고 `./install.sh`를 실행하세요 (`--remove`로 제거) |

**Windows**: 설치 프로그램에 아직 코드 서명이 없어서 SmartScreen이 "Windows의 PC 보호" 창을 띄웁니다. **추가 정보 → 실행**을 누르면 설치가 진행됩니다.
The installer isn't code-signed yet, so SmartScreen shows "Windows protected your PC" — choose **More info → Run anyway**.

**Windows USB**: 보드를 처음 연결할 때 [Zadig](https://zadig.akeo.ie/)로 FEL 장치(`1f3a:efe8`)를 **WinUSB** 드라이버에 한 번 연결해야 합니다 (앱의 문제 해결 버튼 참고).
Bind the board in FEL mode (`1f3a:efe8`) to the **WinUSB** driver once with [Zadig](https://zadig.akeo.ie/) — see the troubleshoot button in the app.

**Linux**: `libusb-1.0-0`과 udev 규칙이 필요합니다 / needs `libusb-1.0-0` and a udev rule — [USB driver](https://github.com/jejezz/allwinner-flashing-tool-macos/blob/main/docs/aw-tool.en.md#usb-driver).

받은 파일은 `SHA256SUMS.txt`로 확인할 수 있습니다: `shasum -a 256 -c SHA256SUMS.txt --ignore-missing`

---
