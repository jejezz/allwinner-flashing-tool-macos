# Allwinner Flasher

릴리스·버전·패키징·정보 창·아이콘·라이선스·UI/UX·글꼴·언어·테마는
https://github.com/jejezz/application-release-templates/tree/main/conventions
규약을 따른다. 이 앱에 적용된 규약 버전: conventions-v1 (미적용: README 데모 GIF, 공통 theme/app_theme.dart — 앱 자체 lib/theme.dart 유지)

## 이 저장소가 템플릿과 다른 점

- Flutter 앱은 `gui/`에, Rust CLI(`aw-tool`)는 저장소 루트에 있다. 릴리스
  워크플로·설치 스크립트·README 도구는 이 배치에 맞춰 경로를 조정했다
  (`.github/workflows/release.yml`, `installer/windows/app.iss`,
  `tool/readme/`의 `APP` / `app` 변수).
- 버전은 `gui/pubspec.yaml`과 `Cargo.toml`이 같아야 한다 — `scripts/bump-version.sh`가
  둘을 함께 올리고, CI `check` 잡이 비교한다.
- macOS는 Apple Silicon(arm64) 전용이라 산출물이 `*-macos-arm64.dmg`다
  (`gui/macos/Runner/Configs/Release.xcconfig`의 `ARCHS`).
- 한 번 릴리스한 값이라 바꾸지 않는다: macOS bundle id `art.zoomon.awflasher`,
  Windows Inno Setup `AppId` `{B7B6E1A0-6E6D-4C3E-9C1A-1F3AEFE80001}`.
- CLI·빌드·USB 드라이버·실기 검증 현황은 `docs/aw-tool.md`, 프로토콜 조사는
  `docs/T527-T507-FEL-EFEX-기술조사.md`.
