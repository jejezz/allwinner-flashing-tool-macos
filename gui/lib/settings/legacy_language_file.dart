import 'dart:io';

import 'package:flutter/widgets.dart';

import 'app_settings.dart';

/// Versions up to 1.1.3 remembered a manually picked language in a plain text
/// file (`<config dir>/AllwinnerFlasher/language`) instead of
/// shared_preferences. Moves that choice to [AppSettings.localeKey] once and
/// deletes the file, so upgrading doesn't reset the language.
Future<void> migrateLegacyLanguageFile(AppSettings settings) async {
  try {
    final file = _legacyFile();
    if (file == null || !file.existsSync()) return;
    final code = file.readAsStringSync().trim();
    // A language already stored under the new key wins over the old file.
    if (settings.locale == null && (code == 'ko' || code == 'en')) {
      await settings.setLocale(Locale(code));
    }
    file.deleteSync();
    final dir = file.parent;
    if (dir.listSync().isEmpty) dir.deleteSync();
  } catch (_) {
    // An unreadable leftover just means the user picks the language again.
  }
}

File? _legacyFile() {
  final env = Platform.environment;
  final String? dir;
  if (Platform.isWindows) {
    final appData = env['APPDATA'];
    dir = appData == null ? null : '$appData/AllwinnerFlasher';
  } else if (Platform.isMacOS) {
    final home = env['HOME'];
    dir = home == null
        ? null
        : '$home/Library/Application Support/AllwinnerFlasher';
  } else {
    final home = env['HOME'];
    final xdg = env['XDG_CONFIG_HOME'];
    dir = home == null && xdg == null
        ? null
        : '${xdg ?? '$home/.config'}/AllwinnerFlasher';
  }
  return dir == null ? null : File('$dir/language');
}
