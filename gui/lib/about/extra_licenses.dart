// From jejezz/application-release-templates common/ @ conventions-v1.
//
// pub 패키지가 아닌 것(번들 글꼴, 네이티브 라이브러리)의 라이선스를 오픈소스
// 라이선스 화면에 더한다 (conventions/licensing.md §2). main()에서
// runApp 전에 한 번 부른다.
//
// 원문 파일은 assets/licenses/에 두고 pubspec의 flutter.assets에 등록한다.
// 앱이 쓰지 않는 항목은 _entries에서 지운다.

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

const _entries = <(List<String> packages, String asset)>[
  (['SeoulNamsan'], 'assets/licenses/seoul-namsan.txt'),
  // aw-tool (the bundled Rust helper) links libusb through rusb.
  (['libusb'], 'assets/licenses/libusb-lgpl-2.1.txt'),
];

void registerExtraLicenses() {
  LicenseRegistry.addLicense(() async* {
    for (final (packages, asset) in _entries) {
      yield LicenseEntryWithLineBreaks(packages, await rootBundle.loadString(asset));
    }
  });
}
