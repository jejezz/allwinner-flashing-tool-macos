// From jejezz/application-release-templates common/ @ conventions-v1.
//
// app_updater 패키지의 UpdateStateStore 를 shared_preferences 로 구현한다.
// 저장하는 것은 두 가지뿐이다: 마지막 자동 확인 시각, 건너뛴 버전.

import 'package:app_updater/app_updater.dart';
import 'package:shared_preferences/shared_preferences.dart';

class PrefsUpdateStateStore implements UpdateStateStore {
  PrefsUpdateStateStore(this._prefs);

  final SharedPreferences _prefs;

  @override
  Future<String?> read(String key) async => _prefs.getString(key);

  @override
  Future<void> write(String key, String value) => _prefs.setString(key, value);
}
