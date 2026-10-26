// From jejezz/application-release-templates common/.
// 창이 어느 모니터에 있는지 판정 (window-placement.md).
// aw_flasher를 앱의 pubspec name으로 바꾼다.

import 'dart:ui';

import 'package:flutter_test/flutter_test.dart';
import 'package:aw_flasher/window/window_placement.dart';
import 'package:screen_retriever/screen_retriever.dart';

// 실제 듀얼 모니터 환경: 주 모니터 2880x1800 @175%(논리 1646x974, 원점),
// 보조 모니터 2560x1440 @100%(screen_retriever 좌표상 x=2880부터).
const _primary = Display(
  id: 'p',
  name: r'\\.\DISPLAY1',
  size: Size(1646, 1029),
  visiblePosition: Offset(0, 0),
  visibleSize: Size(1646, 974),
  scaleFactor: 1.75,
);
const _secondary = Display(
  id: 's',
  name: r'\\.\DISPLAY3',
  size: Size(2560, 1440),
  visiblePosition: Offset(2880, 0),
  visibleSize: Size(2560, 1386),
  scaleFactor: 1.0,
);

void main() {
  test('보조 모니터 위에 있는 창은 보조 모니터로 판정한다', () {
    final bounds = Rect.fromLTWH(3565, 262, 1190, 861);
    expect(displayNameForBounds(bounds, [_primary, _secondary]), _secondary.name);
  });

  test('주 모니터 위에 있는 창(주 모니터 배율 좌표)은 주 모니터로 판정한다', () {
    final bounds = Rect.fromLTWH(228, 60, 1190, 861);
    expect(displayNameForBounds(bounds, [_primary, _secondary]), _primary.name);
  });

  test('모니터 순서가 달라도 같은 결과다', () {
    final bounds = Rect.fromLTWH(3565, 262, 1190, 861);
    expect(displayNameForBounds(bounds, [_secondary, _primary]), _secondary.name);
  });

  test('모니터가 하나뿐이거나 어느 모니터에도 안 걸치면 null', () {
    expect(displayNameForBounds(const Rect.fromLTWH(10, 10, 100, 100), [_primary]), isNull);
    expect(
      displayNameForBounds(const Rect.fromLTWH(9000, 9000, 100, 100), [_primary, _secondary]),
      isNull,
    );
  });
}
