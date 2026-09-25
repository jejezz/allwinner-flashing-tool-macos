// From jejezz/application-release-templates common/ @ conventions-v1.
//
// 테마 모드와 언어 설정 (conventions/theming.md §1·§3, localization.md §3·§5).
// 상태 관리 라이브러리 없이 ChangeNotifier 하나로 두고, AppSettingsScope로
// 위젯 트리에 내린다. 앱이 Riverpod 등을 이미 쓴다면 이 클래스를 그대로
// 감싸서 쓰면 된다 — 저장 키와 동작만 같으면 된다.

import 'package:flutter/material.dart';
import 'package:shared_preferences/shared_preferences.dart';

class AppSettings extends ChangeNotifier {
  AppSettings._(this._prefs, this._themeMode, this._locale);

  /// 저장 키 — 모든 앱 공통 (localization.md §5). 바꾸지 않는다.
  static const themeModeKey = 'theme_mode';
  static const localeKey = 'app_locale';

  final SharedPreferences _prefs;
  ThemeMode _themeMode;
  Locale? _locale;

  /// 저장된 값을 읽는다. [legacyKeys]는 예전 앱이 쓰던 키를 새 키로 한 번
  /// 옮긴다 — 키를 그냥 바꾸면 사용자 설정이 사라진다.
  /// 예: `AppSettings.load(legacyKeys: {'locale': AppSettings.localeKey})`
  static Future<AppSettings> load({Map<String, String> legacyKeys = const {}}) async {
    final prefs = await SharedPreferences.getInstance();
    for (final MapEntry(key: old, value: current) in legacyKeys.entries) {
      final value = prefs.getString(old);
      if (value != null && !prefs.containsKey(current)) await prefs.setString(current, value);
      await prefs.remove(old);
    }
    final mode = ThemeMode.values.asNameMap()[prefs.getString(themeModeKey)] ?? ThemeMode.system;
    final code = prefs.getString(localeKey);
    return AppSettings._(prefs, mode, code == null ? null : Locale(code));
  }

  ThemeMode get themeMode => _themeMode;

  /// null = 시스템 언어를 따른다.
  Locale? get locale => _locale;

  Future<void> setThemeMode(ThemeMode mode) async {
    if (mode == _themeMode) return;
    _themeMode = mode;
    notifyListeners();
    await _prefs.setString(themeModeKey, mode.name);
  }

  Future<void> setLocale(Locale? locale) async {
    if (locale == _locale) return;
    _locale = locale;
    notifyListeners();
    if (locale == null) {
      await _prefs.remove(localeKey);
    } else {
      await _prefs.setString(localeKey, locale.languageCode);
    }
  }

  /// MaterialApp.localeResolutionCallback — OS 언어가 한국어면 한국어,
  /// 그 밖의 모든 언어는 영어 (localization.md §3). 이 콜백이 없으면
  /// Flutter는 지원 목록의 첫 언어(ko)로 떨어진다.
  static Locale resolveLocale(Locale? device, Iterable<Locale> supported) =>
      device?.languageCode == 'ko' ? const Locale('ko') : const Locale('en');
}

/// `AppSettingsScope.of(context)`로 설정을 읽고, 바뀌면 다시 그린다.
class AppSettingsScope extends InheritedNotifier<AppSettings> {
  const AppSettingsScope({super.key, required AppSettings settings, required super.child})
      : super(notifier: settings);

  static AppSettings of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<AppSettingsScope>()!.notifier!;
}
