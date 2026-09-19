import 'dart:io';

/// Remembers the language the user picked manually, across launches. Null
/// means "follow the system language" — the default, and what every user
/// gets until they touch the language button.
///
/// Written by hand rather than via a package (shared_preferences etc.): the
/// app has no other persisted state, and one small text file is simpler than
/// a new dependency for it.
class LanguageSetting {
  const LanguageSetting._();

  static File? _file() {
    final dir = _configDir();
    if (dir == null) return null;
    return File('${dir.path}/language');
  }

  static Directory? _configDir() {
    if (Platform.isWindows) {
      final appData = Platform.environment['APPDATA'];
      return appData == null ? null : Directory('$appData/AllwinnerFlasher');
    }
    if (Platform.isMacOS) {
      final home = Platform.environment['HOME'];
      return home == null
          ? null
          : Directory('$home/Library/Application Support/AllwinnerFlasher');
    }
    // Linux and anything else POSIX-ish.
    final home = Platform.environment['HOME'];
    if (home == null) return null;
    final xdgConfig = Platform.environment['XDG_CONFIG_HOME'];
    return Directory('${xdgConfig ?? '$home/.config'}/AllwinnerFlasher');
  }

  /// The saved language code ('ko'/'en'), or null to follow the system.
  static String? load() {
    try {
      final file = _file();
      if (file == null || !file.existsSync()) return null;
      final code = file.readAsStringSync().trim();
      return code.isEmpty ? null : code;
    } catch (_) {
      // A missing or unreadable file just means "use the system default".
      return null;
    }
  }

  static void save(String? code) {
    try {
      final file = _file();
      if (file == null) return;
      if (code == null) {
        if (file.existsSync()) file.deleteSync();
        return;
      }
      file.parent.createSync(recursive: true);
      file.writeAsStringSync(code);
    } catch (_) {
      // Not being able to remember the choice isn't worth surfacing to the
      // user — worst case they pick it again next launch.
    }
  }
}
