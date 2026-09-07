import 'package:aw_flasher/about.dart';
import 'package:aw_flasher/l10n/app_localizations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// Pumps a host app in `locale` and hands back a context inside it.
Future<BuildContext> _host(WidgetTester tester, Locale locale) async {
  late BuildContext captured;
  await tester.pumpWidget(
    MaterialApp(
      locale: locale,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      home: Builder(
        builder: (context) {
          captured = context;
          return const SizedBox();
        },
      ),
    ),
  );
  return captured;
}

void main() {
  testWidgets('both languages resolve, and only those two', (tester) async {
    expect(
      AppLocalizations.supportedLocales.map((l) => l.languageCode).toSet(),
      {'en', 'ko'},
    );

    final ko = AppLocalizations.of(await _host(tester, const Locale('ko')));
    expect(ko.flashStart, '플래싱 시작');

    final en = AppLocalizations.of(await _host(tester, const Locale('en')));
    expect(en.flashStart, 'Start flashing');
  });

  testWidgets('an unsupported system language falls back to English',
      (tester) async {
    final ja = AppLocalizations.of(await _host(tester, const Locale('ja')));
    expect(ja.flashStart, 'Start flashing');
  });

  testWidgets('placeholders are filled, not left as literal braces',
      (tester) async {
    final ko = AppLocalizations.of(await _host(tester, const Locale('ko')));

    expect(ko.deviceFelWithSoc('1890'), contains('0x1890'));
    expect(ko.confirmPartial(3), contains('3'));
    expect(ko.progressOverall('47'), contains('47'));
    expect(ko.imageSummary(10, 12), allOf(contains('10'), contains('12')));
    // A missing substitution would leave the placeholder name behind.
    expect(ko.errorInterrupted(1), isNot(contains('{')));
  });

  testWidgets('the About dialog builds and shows the purpose text',
      (tester) async {
    for (final (locale, needle) in [
      (const Locale('ko'), 'PhoenixSuit'),
      (const Locale('en'), 'PhoenixSuit'),
    ]) {
      final context = await _host(tester, locale);
      showAboutSheet(context);
      await tester.pumpAndSettle();

      expect(find.text('Allwinner Flasher'), findsOneWidget);
      expect(find.textContaining(needle), findsOneWidget);
      expect(find.text('© 2026 jyahn'), findsOneWidget);
      expect(find.textContaining(kAppVersion), findsOneWidget);

      await tester.tap(find.text('OK'));
      await tester.pumpAndSettle();
    }
  });
}
