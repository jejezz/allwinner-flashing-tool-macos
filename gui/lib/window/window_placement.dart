import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:ui';

import 'package:flutter/foundation.dart';
import 'package:screen_retriever/screen_retriever.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:window_manager/window_manager.dart';

/// 모니터 하나의 작업 영역(작업 표시줄 제외)을 **물리 픽셀**로 나타낸 것. 모든 모니터가 같은 가상 화면
/// 좌표계를 쓰므로 모니터마다 배율이 달라도 서로 비교할 수 있다.
///
/// 왜 물리 픽셀인가: screen_retriever는 위치를 "그 모니터의 배율"로 나눈 값으로 주고, window_manager는
/// 그 값에 "창이 지금 있는 모니터의 배율"을 곱해서 쓴다. 두 모니터의 배율이 다르면 그 차이만큼 창이 한쪽으로
/// 쏠려서 일부가 화면 밖으로 잘린다. 그래서 위치와 크기를 직접 물리 픽셀로 계산하고, 마지막에 창이 있는 모니터의
/// 배율로만 나눠서 넘긴다.
class DisplayArea {
  const DisplayArea({required this.workArea, required this.scale, this.primary = false});

  final Rect workArea;
  final double scale;
  final bool primary;

  factory DisplayArea.fromDisplay(Display d, {bool primary = false}) {
    final scale = (d.scaleFactor ?? 1).toDouble();
    final size = d.visibleSize ?? d.size;
    final pos = d.visiblePosition ?? Offset.zero;
    return DisplayArea(
      workArea: Rect.fromLTWH(pos.dx * scale, pos.dy * scale, size.width * scale, size.height * scale),
      scale: scale,
      primary: primary,
    );
  }
}

/// 마지막으로 기억한 창 위치와 크기 (물리 픽셀).
class SavedPlacement {
  const SavedPlacement({required this.bounds, required this.scale, this.maximized = false});

  final Rect bounds;

  /// 저장할 때 창이 있던 모니터의 배율. 복원할 때 모니터 배율이 달라졌다면 크기를 환산한다.
  final double scale;
  final bool maximized;

  Map<String, Object> toJson() => {
    'x': bounds.left,
    'y': bounds.top,
    'w': bounds.width,
    'h': bounds.height,
    'scale': scale,
    'max': maximized,
  };

  static SavedPlacement? fromJson(Object? json) {
    if (json is! Map) return null;
    double? number(String k) => json[k] is num ? (json[k] as num).toDouble() : null;
    final x = number('x'), y = number('y'), w = number('w'), h = number('h'), s = number('scale');
    if (x == null || y == null || w == null || h == null || s == null) return null;
    if (![x, y, w, h, s].every((v) => v.isFinite) || w <= 0 || h <= 0 || s <= 0) return null;
    return SavedPlacement(bounds: Rect.fromLTWH(x, y, w, h), scale: s, maximized: json['max'] == true);
  }
}

/// 창 배치 계산 (화면에 아무것도 하지 않는 순수 함수라서 테스트할 수 있다).
abstract final class PlacementMath {
  /// 제목 표시줄을 잡고 끌 수 있으려면 이만큼은 어느 모니터 위에 걸쳐 있어야 한다.
  static const minVisible = Size(100, 50);

  /// [d]의 가운데에 논리 크기 [logicalSize]인 창을 놓는다. 작업 영역보다 크면 줄인다.
  static Rect centerOn(DisplayArea d, Size logicalSize) {
    final w = math.min(logicalSize.width * d.scale, d.workArea.width);
    final h = math.min(logicalSize.height * d.scale, d.workArea.height);
    return Rect.fromLTWH(
      d.workArea.left + (d.workArea.width - w) / 2,
      d.workArea.top + (d.workArea.height - h) / 2,
      w,
      h,
    );
  }

  static DisplayArea? primaryOf(List<DisplayArea> displays) =>
      displays.where((d) => d.primary).firstOrNull ?? displays.firstOrNull;

  /// 저장한 위치를 지금 연결된 모니터에 맞춰 복원한다. 걸쳐 있는 모니터가 없으면(떼어낸 모니터 등) null.
  /// 반환하는 사각형은 항상 한 모니터의 작업 영역 안에 완전히 들어온다.
  static Rect? restore(SavedPlacement saved, List<DisplayArea> displays) {
    DisplayArea? best;
    var bestArea = 0.0;
    for (final d in displays) {
      final i = saved.bounds.intersect(d.workArea);
      if (i.width < minVisible.width || i.height < minVisible.height) continue;
      if (i.width * i.height > bestArea) {
        best = d;
        bestArea = i.width * i.height;
      }
    }
    if (best == null) return null;

    final k = best.scale / saved.scale; // 모니터 배율이 달라졌어도 논리 크기는 같게
    final work = best.workArea;
    final w = math.min(saved.bounds.width * k, work.width);
    final h = math.min(saved.bounds.height * k, work.height);
    final left = saved.bounds.left.clamp(work.left, work.right - w);
    final top = saved.bounds.top.clamp(work.top, work.bottom - h);
    return Rect.fromLTWH(left, top, w, h);
  }
}

/// 마지막 창 위치를 기억하고 다시 켤 때 복원한다 (Windows). 기억한 곳이 없거나 그 모니터가 없어졌으면
/// 주 모니터 가운데에 연다.
class WindowPlacement with WindowListener {
  WindowPlacement(this._prefs, {required this.defaultSize, this.isDocked});

  /// Windows에서만 쓴다. 다른 OS는 window_manager의 `center`가 올바르게 동작한다.
  static bool get enabled => Platform.isWindows;

  static const _key = 'window_placement';

  final SharedPreferences _prefs;
  final Size defaultSize;

  /// 도킹처럼 일반 창이 아닌 상태에서는 저장하지 않는다. 다음 실행 때 도킹 전 크기로 돌아와야 하기 때문이다.
  final bool Function()? isDocked;

  Timer? _debounce;
  SavedPlacement? _last;

  Future<List<DisplayArea>> _displays() async {
    final all = await screenRetriever.getAllDisplays();
    final primary = await screenRetriever.getPrimaryDisplay();
    return [for (final d in all) DisplayArea.fromDisplay(d, primary: d.id == primary.id)];
  }

  SavedPlacement? _load() {
    final raw = _prefs.getString(_key);
    if (raw == null) return null;
    try {
      return SavedPlacement.fromJson(jsonDecode(raw));
    } on FormatException {
      return null;
    }
  }

  /// 창을 보이기 전에 부른다. 목표 모니터로 먼저 옮겨 배율이 바뀌기를 기다린 뒤 최종 크기와 위치를 정한다
  /// (배율이 다른 모니터로 옮기면 Windows가 창 크기를 한 번 더 조정하기 때문).
  Future<void> placeInitially() async {
    try {
      final displays = await _displays();
      if (displays.isEmpty) return;
      final saved = _load();
      final restored = saved == null ? null : PlacementMath.restore(saved, displays);
      final target = restored ?? PlacementMath.centerOn(PlacementMath.primaryOf(displays)!, defaultSize);
      final display = displays.firstWhere(
        (d) => d.workArea.contains(target.center),
        orElse: () => PlacementMath.primaryOf(displays)!,
      );

      // 1단계: 목표 모니터 안으로 옮긴다.
      var dpr = windowManager.getDevicePixelRatio();
      await windowManager.setPosition(Offset((target.left + 40) / dpr, (target.top + 40) / dpr));
      await _waitForScale(display.scale);

      // 2단계: 그 모니터의 배율로 최종 위치와 크기를 정한다.
      dpr = windowManager.getDevicePixelRatio();
      await windowManager.setBounds(
        Rect.fromLTWH(target.left / dpr, target.top / dpr, target.width / dpr, target.height / dpr),
      );
      _last = SavedPlacement(bounds: target, scale: dpr, maximized: false);
      if (restored != null && saved!.maximized) {
        await windowManager.maximize();
        _last = SavedPlacement(bounds: target, scale: dpr, maximized: true);
      }
    } catch (e) {
      // 배치에 실패해도 창은 떠야 한다.
      debugPrint('window placement failed: $e');
    }
  }

  Future<void> _waitForScale(double scale) async {
    for (var i = 0; i < 12; i++) {
      if ((windowManager.getDevicePixelRatio() - scale).abs() < 0.01) return;
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
  }

  void attach() => windowManager.addListener(this);

  void detach() {
    windowManager.removeListener(this);
    _debounce?.cancel();
  }

  void _scheduleSave() {
    _debounce?.cancel();
    _debounce = Timer(const Duration(milliseconds: 500), saveNow);
  }

  @override
  void onWindowMoved() => _scheduleSave();

  @override
  void onWindowResized() => _scheduleSave();

  @override
  void onWindowMaximize() => _scheduleSave();

  @override
  void onWindowUnmaximize() => _scheduleSave();

  /// 지금 위치를 저장한다. 최대화 중이면 최대화했다는 사실만 새로 적고, 크기와 위치는 최대화 직전의 값을
  /// 그대로 둔다 (복원했을 때 "최대화 해제"가 원래 크기로 돌아가도록).
  Future<void> saveNow() async {
    _debounce?.cancel();
    try {
      if (isDocked?.call() ?? false) return;
      if (await windowManager.isMinimized() || await windowManager.isFullScreen()) return;
      if (await windowManager.isMaximized()) {
        final last = _last;
        if (last != null) _last = SavedPlacement(bounds: last.bounds, scale: last.scale, maximized: true);
      } else {
        final dpr = windowManager.getDevicePixelRatio();
        final b = await windowManager.getBounds();
        _last = SavedPlacement(
          bounds: Rect.fromLTWH(b.left * dpr, b.top * dpr, b.width * dpr, b.height * dpr),
          scale: dpr,
        );
      }
      final last = _last;
      if (last != null) await _prefs.setString(_key, jsonEncode(last.toJson()));
    } catch (e) {
      debugPrint('window placement save failed: $e');
    }
  }
}
