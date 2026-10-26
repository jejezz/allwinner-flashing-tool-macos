// From jejezz/application-release-templates common/.
//
// 창을 처음 띄울 때 위치를 정한다 — 듀얼 모니터(특히 모니터마다 배율이 다른 Windows)에서
// `center: true`와 저장 좌표 복원이 창을 화면 밖에 놓던 문제를 피한다.
// 규칙과 이유는 conventions/window-placement.md.
//
// 쓰는 법 (main.dart):
//   const size = Size(1200, 720);
//   await windowManager.waitUntilReadyToShow(
//     const WindowOptions(size: size, minimumSize: Size(960, 600), center: false, title: ...),
//     () async {
//       await placeWindowAtStartup(size);   // show() 전에
//       await windowManager.show();
//       await windowManager.focus();
//       LastDisplaySaver().start();         // 선택: 마지막에 쓰던 모니터를 기억
//     },
//   );
//
// 의존성: window_manager, screen_retriever, shared_preferences.

import 'dart:async';
import 'dart:io' show Platform;
import 'dart:math' as math;
import 'dart:ui';

import 'package:screen_retriever/screen_retriever.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:window_manager/window_manager.dart';

const _kDisplay = 'window_display';

/// 창을 처음 띄울 위치를 정해 옮긴다. `show()` 전에 부른다.
///
/// - Windows이고 모니터가 둘 이상이면: 마지막에 쓰던 모니터(없으면 주 모니터) 가운데.
/// - 그 밖(모니터 하나, macOS, Linux)이면: `windowManager.center()`.
Future<void> placeWindowAtStartup(Size size) async {
  if (Platform.isWindows) {
    try {
      final displays = await screenRetriever.getAllDisplays();
      if (displays.length > 1) {
        final saved = (await SharedPreferences.getInstance()).getString(_kDisplay);
        final primary = await screenRetriever.getPrimaryDisplay();
        final target = displays.firstWhere((d) => d.name == saved, orElse: () => primary);
        await centerWindowOnDisplay(target, size);
        return;
      }
    } catch (_) {
      // 모니터 정보를 못 읽으면 아래 기본 동작으로.
    }
  }
  await windowManager.center();
}

/// 창을 [target] 모니터 가운데에 놓는다.
///
/// `window_manager.setBounds`는 넘긴 좌표에 "창이 그 순간 있는 모니터의 배율"을
/// 곱한다(실측 확인). 창은 주 모니터에서 만들어지므로, 배율이 다른 보조 모니터
/// 가운데 좌표를 그대로 넘기면 배율 비율만큼 불어나 모니터 밖에 놓인다(주 175%,
/// 보조 100%에서 3565가 6238이 되어 보조 모니터 오른쪽 바깥에 놓였다). 그래서
/// 1차는 주 모니터 배율로 나눈 값으로 [target] 안에 먼저 올리고, 창이 그
/// 모니터의 배율로 넘어간 뒤 2차로 [target] 배율 기준 좌표를 준다.
/// (`center: true`/`windowManager.center()`도 같은 계열의 문제로 배율이 다른
/// 듀얼 모니터에서 믿을 수 없다.)
Future<void> centerWindowOnDisplay(Display target, Size size) async {
  final primary = await screenRetriever.getPrimaryDisplay();
  final primaryScale = (primary.scaleFactor ?? 1).toDouble();
  final targetScale = (target.scaleFactor ?? 1).toDouble();
  final origin = target.visiblePosition ?? Offset.zero;
  final area = target.visibleSize ?? target.size;
  final logical = Offset(
    origin.dx + math.max(0.0, (area.width - size.width) / 2),
    origin.dy + math.max(0.0, (area.height - size.height) / 2),
  );
  final ratio = targetScale / primaryScale;
  await windowManager.setBounds(
    Rect.fromLTWH(logical.dx * ratio, logical.dy * ratio, size.width, size.height),
  );
  // 창이 새 모니터의 DPI로 넘어갈 시간을 플랫폼 채널 왕복 한 번으로 확보한다.
  await windowManager.getBounds();
  await windowManager.setBounds(Rect.fromLTWH(logical.dx, logical.dy, size.width, size.height));
}

/// [bounds](window_manager가 돌려주는 값)의 중심이 놓인 모니터의 이름. window_manager의
/// 좌표는 "창이 지금 있는 모니터의 배율"이 적용된 값이라, 모니터마다 "이 모니터의
/// 배율로 환산해 그 모니터 영역 안에 들어가는가"를 따져 어느 모니터인지 찾는다.
/// 모니터가 하나뿐이거나 찾지 못하면 null.
String? displayNameForBounds(Rect bounds, List<Display> displays) {
  if (displays.length < 2) return null;
  final center = bounds.center;
  for (final d in displays) {
    final scale = (d.scaleFactor ?? 1).toDouble();
    final origin = d.visiblePosition ?? Offset.zero;
    final area = d.visibleSize ?? d.size;
    final physical = Rect.fromLTWH(
      origin.dx * scale,
      origin.dy * scale,
      area.width * scale,
      area.height * scale,
    );
    if (physical.contains(Offset(center.dx * scale, center.dy * scale))) return d.name;
  }
  return null;
}

/// 창을 옮기거나 크기를 바꾸면 잠시 뒤 지금 있는 모니터의 이름을 저장한다
/// ([placeWindowAtStartup]이 다음 실행 때 그 모니터에 연다). Windows에서만 동작한다.
class LastDisplaySaver with WindowListener {
  Timer? _timer;

  void start() {
    if (Platform.isWindows) windowManager.addListener(this);
  }

  void dispose() {
    _timer?.cancel();
    windowManager.removeListener(this);
  }

  void _schedule() {
    _timer?.cancel();
    _timer = Timer(const Duration(milliseconds: 400), _save);
  }

  Future<void> _save() async {
    try {
      if (await windowManager.isMaximized() ||
          await windowManager.isFullScreen() ||
          await windowManager.isMinimized()) {
        return;
      }
      final name = displayNameForBounds(
        await windowManager.getBounds(),
        await screenRetriever.getAllDisplays(),
      );
      if (name != null) {
        await (await SharedPreferences.getInstance()).setString(_kDisplay, name);
      }
    } catch (_) {}
  }

  @override
  void onWindowMoved() => _schedule();

  @override
  void onWindowResized() => _schedule();
}
