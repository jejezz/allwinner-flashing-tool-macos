import 'dart:io' show Platform;

import 'package:flutter/material.dart';
import 'package:window_manager/window_manager.dart';

/// 창이 포커스를 받으면 앱 바 배경이 서서히 강조색으로 물들고, 잃으면 서서히 빠진다 (ui-ux.md §4).
/// `AppBar(flexibleSpace: const FocusTint())`로 쓴다. 데스크톱에서만 그린다 — 모바일에는 창 포커스가 없다.
/// OS가 그리는 타이틀 바와 신호등 버튼은 건드리지 않는다.
class FocusTint extends StatefulWidget {
  const FocusTint({super.key});

  @override
  State<FocusTint> createState() => _FocusTintState();
}

class _FocusTintState extends State<FocusTint> with WindowListener {
  static final bool _desktop = Platform.isMacOS || Platform.isWindows || Platform.isLinux;

  bool _focused = true;

  @override
  void initState() {
    super.initState();
    if (!_desktop) return;
    windowManager.addListener(this);
    // 플러그인이 없는 환경(위젯 테스트)에서는 실패해도 처음 값(포커스됨)을 그대로 쓴다.
    windowManager.isFocused().then((v) {
      if (mounted) setState(() => _focused = v);
    }).catchError((Object _) {});
  }

  @override
  void dispose() {
    if (_desktop) windowManager.removeListener(this);
    super.dispose();
  }

  @override
  void onWindowFocus() => setState(() => _focused = true);

  @override
  void onWindowBlur() => setState(() => _focused = false);

  @override
  Widget build(BuildContext context) {
    if (!_desktop) return const SizedBox.shrink();
    final scheme = Theme.of(context).colorScheme;
    return AnimatedContainer(
      duration: const Duration(milliseconds: 300),
      curve: Curves.easeOut,
      decoration: BoxDecoration(
        color: _focused ? scheme.primary.withValues(alpha: 0.12) : Colors.transparent,
        border: Border(
          bottom: BorderSide(color: _focused ? scheme.primary.withValues(alpha: 0.45) : Colors.transparent, width: 1.5),
        ),
      ),
    );
  }
}
