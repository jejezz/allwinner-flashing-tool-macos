// From jejezz/application-release-templates common/ @ conventions-v1.
// 설정 저장 키·기존 키 이전·언어 해석 (localization.md §3·§5).
// aw_flasher를 앱의 pubspec name으로 바꾼다.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:aw_flasher/settings/app_settings.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  test('defaults to system theme and system language', () async {
    SharedPreferences.setMockInitialValues({});
    final s = await AppSettings.load();
    expect(s.themeMode, ThemeMode.system);
    expect(s.locale, isNull);
  });

  test('persists under theme_mode / app_locale', () async {
    SharedPreferences.setMockInitialValues({});
    final s = await AppSettings.load();
    await s.setThemeMode(ThemeMode.dark);
    await s.setLocale(const Locale('en'));
    final prefs = await SharedPreferences.getInstance();
    expect(prefs.getString('theme_mode'), 'dark');
    expect(prefs.getString('app_locale'), 'en');
    await s.setLocale(null);
    expect(prefs.containsKey('app_locale'), isFalse);
  });

  test('moves a legacy key once', () async {
    SharedPreferences.setMockInitialValues({'locale': 'en'});
    final s = await AppSettings.load(legacyKeys: {'locale': AppSettings.localeKey});
    final prefs = await SharedPreferences.getInstance();
    expect(s.locale, const Locale('en'));
    expect(prefs.containsKey('locale'), isFalse);
  });

  test('non-Korean system languages resolve to English', () {
    const supported = [Locale('ko'), Locale('en')];
    expect(AppSettings.resolveLocale(const Locale('ko', 'KR'), supported), const Locale('ko'));
    expect(AppSettings.resolveLocale(const Locale('ja'), supported), const Locale('en'));
    expect(AppSettings.resolveLocale(null, supported), const Locale('en'));
  });
}
