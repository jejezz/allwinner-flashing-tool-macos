// From jejezz/application-release-templates common/ @ conventions-v1.
// conventions/about-dialog.md §4 — 정보 창이 열리고 버전·저작권·라이선스
// 화면이 동작하는지. aw_flasher를 앱의 pubspec name으로 바꾼다.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:aw_flasher/about/about_dialog.dart';
import 'package:aw_flasher/app_identity.dart';
import 'package:aw_flasher/l10n/app_localizations.dart';

Widget _host(Locale locale) => MaterialApp(
      locale: locale,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: const Scaffold(
        body: AppAboutDialog(version: '1.4.2', buildNumber: '37', tagline: 'Tagline', description: 'Description'),
      ),
    );

void main() {
  testWidgets('shows name, version, copyright and license (ko)', (tester) async {
    await tester.pumpWidget(_host(const Locale('ko')));
    expect(find.text(AppIdentity.displayName), findsOneWidget);
    expect(find.text('버전 1.4.2 (빌드 37)'), findsOneWidget);
    expect(find.text(AppIdentity.copyright), findsOneWidget);
    expect(find.text(AppIdentity.licenseName), findsOneWidget);
  });

  testWidgets('English strings', (tester) async {
    await tester.pumpWidget(_host(const Locale('en')));
    expect(find.text('Version 1.4.2 (build 37)'), findsOneWidget);
    expect(find.text('Open Source Licenses'), findsOneWidget);
  });

  testWidgets('open-source licenses page opens', (tester) async {
    await tester.pumpWidget(_host(const Locale('en')));
    await tester.tap(find.text('Open Source Licenses'));
    await tester.pumpAndSettle();
    expect(find.byType(LicensePage), findsOneWidget);
  });
}
