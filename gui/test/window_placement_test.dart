// From jejezz/application-release-templates common/.
// 창 배치 계산 (window-placement.md). aw_flasher를 앱의 pubspec name으로 바꾼다.

import 'dart:ui';

import 'package:aw_flasher/window/window_placement.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:screen_retriever/screen_retriever.dart';

// 가상 화면(물리 픽셀): 왼쪽 주 모니터 2560x1440(작업 영역 1400 높이, 배율 150%),
// 그 오른쪽에 이어 붙은 1920x1080 모니터(배율 100%).
final _left = DisplayArea(workArea: const Rect.fromLTWH(0, 0, 2560, 1400), scale: 1.5, primary: true);
final _right = DisplayArea(workArea: const Rect.fromLTWH(2560, 0, 1920, 1040), scale: 1.0);
final _both = [_left, _right];
const _size = Size(1200, 720);

bool _inside(Rect inner, Rect outer) =>
    inner.left >= outer.left && inner.top >= outer.top && inner.right <= outer.right && inner.bottom <= outer.bottom;

void main() {
  group('centerOn', () {
    test('keeps the same logical size on each monitor (physical = logical × scale)', () {
      final l = PlacementMath.centerOn(_left, _size);
      expect((l.width, l.height), (1800, 1080)); // 150%
      final r = PlacementMath.centerOn(_right, _size);
      expect((r.width, r.height), (1200, 720)); // 100%
    });

    test('is centred inside the chosen monitor, never spilling to the neighbour', () {
      final r = PlacementMath.centerOn(_right, _size);
      expect(r.center, _right.workArea.center);
      expect(_inside(r, _right.workArea), isTrue);
      final l = PlacementMath.centerOn(_left, _size);
      expect(_inside(l, _left.workArea), isTrue);
      expect(l.right, lessThanOrEqualTo(_right.workArea.left));
    });

    test('shrinks to the work area on a small monitor', () {
      const small = DisplayArea(workArea: Rect.fromLTWH(0, 0, 1000, 600), scale: 1.0, primary: true);
      final r = PlacementMath.centerOn(small, _size);
      expect((r.width, r.height), (1000, 600));
    });
  });

  group('restore', () {
    test('a saved window on the right monitor comes back on the right monitor, fully visible', () {
      final saved = SavedPlacement(bounds: const Rect.fromLTWH(3000, 100, 1200, 720), scale: 1.0);
      final r = PlacementMath.restore(saved, _both)!;
      expect(r, saved.bounds);
      expect(_inside(r, _right.workArea), isTrue);
    });

    test('a window hanging off the right edge is pulled back inside (the reported symptom)', () {
      final saved = SavedPlacement(bounds: const Rect.fromLTWH(3900, 200, 1200, 720), scale: 1.0);
      final r = PlacementMath.restore(saved, _both)!;
      expect(_inside(r, _right.workArea), isTrue);
      expect(r.width, 1200);
      expect(r.right, _right.workArea.right);
    });

    test('a window straddling two monitors goes to the one it mostly covers', () {
      final saved = SavedPlacement(bounds: const Rect.fromLTWH(2000, 100, 1800, 1000), scale: 1.5);
      final r = PlacementMath.restore(saved, _both)!;
      // 오른쪽 모니터에 걸친 면적이 더 넓다 → 오른쪽으로, 배율이 150%→100%라 크기는 2/3로.
      expect(_inside(r, _right.workArea), isTrue);
      expect(r.width, closeTo(1200, 0.01));
      expect(r.height, closeTo(1000 / 1.5, 0.01));
    });

    test('a removed monitor (saved position now off every screen) gives null', () {
      final saved = SavedPlacement(bounds: const Rect.fromLTWH(6000, 100, 1200, 720), scale: 1.0);
      expect(PlacementMath.restore(saved, _both), isNull);
      expect(PlacementMath.restore(saved, [_left]), isNull);
    });

    test('barely touching a monitor is not enough to count as visible', () {
      final saved = SavedPlacement(bounds: const Rect.fromLTWH(4470, 100, 1200, 720), scale: 1.0);
      expect(PlacementMath.restore(saved, _both), isNull); // 10px만 걸침
    });

    test('a window bigger than the monitor shrinks to fit', () {
      final saved = SavedPlacement(bounds: const Rect.fromLTWH(0, 0, 5000, 3000), scale: 1.5);
      final r = PlacementMath.restore(saved, [_left])!;
      expect((r.width, r.height), (2560, 1400));
    });
  });

  group('primaryOf', () {
    test('prefers the primary display, falls back to the first, null when none', () {
      expect(PlacementMath.primaryOf([_right, _left]), _left);
      expect(PlacementMath.primaryOf([_right]), _right);
      expect(PlacementMath.primaryOf([]), isNull);
    });
  });

  group('SavedPlacement json', () {
    test('round trips and rejects bad data', () {
      final s = SavedPlacement(bounds: const Rect.fromLTWH(10, 20, 300, 400), scale: 1.25, maximized: true);
      final back = SavedPlacement.fromJson(s.toJson())!;
      expect((back.bounds, back.scale, back.maximized), (s.bounds, 1.25, true));
      expect(SavedPlacement.fromJson('nope'), isNull);
      expect(SavedPlacement.fromJson({'x': 0, 'y': 0, 'w': 0, 'h': 5, 'scale': 1}), isNull);
      expect(SavedPlacement.fromJson({'x': 0, 'y': 0, 'w': 5, 'h': 5}), isNull);
      expect(SavedPlacement.fromJson({'x': double.nan, 'y': 0, 'w': 5, 'h': 5, 'scale': 1}), isNull);
    });
  });

  test('DisplayArea.fromDisplay turns screen_retriever units (÷ own scale) back into physical pixels', () {
    // screen_retriever: 오른쪽 모니터(배율 1.0)의 작업 영역 시작 x = 2560/1.0, 왼쪽(1.5)은 0.
    final d = Display(
      id: '2',
      size: const Size(1920, 1080),
      visiblePosition: const Offset(2560, 0),
      visibleSize: const Size(1920, 1040),
      scaleFactor: 1.0,
    );
    final a = DisplayArea.fromDisplay(d);
    expect(a.workArea, const Rect.fromLTWH(2560, 0, 1920, 1040));
    final l = Display(
      id: '1',
      size: const Size(1707, 960),
      visiblePosition: const Offset(0, 0),
      visibleSize: const Size(1707, 933),
      scaleFactor: 1.5,
    );
    expect(DisplayArea.fromDisplay(l, primary: true).workArea.width, closeTo(2560.5, 1));
  });
}
