// From jejezz/application-release-templates common/ @ conventions-v1.
//
// conventions/identity.md의 값을 앱 코드에 반영하는 유일한 곳이다.
// displayName은 macos/Runner/Configs/AppInfo.xcconfig의 PRODUCT_NAME과 같아야
// 하고, 데스크톱 릴리스 워크플로의 check 잡이 둘을 비교한다.

abstract final class AppIdentity {
  /// 표시 이름. 번역하지 않는다 (conventions/localization.md §2).
  static const displayName = 'Allwinner Flasher';

  static const repositoryUrl = 'https://github.com/jejezz/allwinner-flashing-tool-macos';

  static const copyrightHolder = 'Jongyun Ahn';

  /// 첫 릴리스 연도. 해마다 바꾸지 않는다.
  static const firstReleaseYear = 2026;

  static const licenseName = 'MIT License';

  /// 256px 사본 — tool/icon/generate_icons.py가 만든다.
  static const iconAsset = 'assets/icon/app_icon.png';

  static const copyright = 'Copyright © $firstReleaseYear $copyrightHolder';
}
