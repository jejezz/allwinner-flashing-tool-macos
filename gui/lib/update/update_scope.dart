// From jejezz/application-release-templates common/ @ conventions-v1.
//
// UpdateService 를 위젯 트리 위쪽에 걸어 두고 어디서든 꺼내 쓰게 한다.
// 정보 창(showAppAboutDialog)이 이것을 읽어 "업데이트 확인" 단추를 스스로 붙이므로,
// 정보 창을 여는 곳이 여럿(앱 메뉴 · 탭 바 · 트레이 …)이어도 각각 연결할 필요가 없다.
//
// main.dart: runApp(UpdateScope(service: updates, child: App(...))) — MaterialApp **위**에 둔다.

import 'package:flutter/widgets.dart';

import 'update_service.dart';

class UpdateScope extends InheritedWidget {
  const UpdateScope({super.key, required this.service, required super.child});

  /// null 이면 업데이트 확인을 쓰지 않는다 (모바일, UPDATE_SERVER 가 빈 값).
  final UpdateService? service;

  static UpdateService? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<UpdateScope>()?.service;

  @override
  bool updateShouldNotify(UpdateScope oldWidget) => service != oldWidget.service;
}
