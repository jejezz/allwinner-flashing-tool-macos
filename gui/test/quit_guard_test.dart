// Quitting while the board is being written leaves it unable to boot, so the
// app refuses to quit mid-flash — Cmd+Q, the window's close button and the
// updater's quit all arrive as an exit request.

import 'dart:ui' show AppExitResponse;

import 'package:aw_flasher/flasher_model.dart';
import 'package:aw_flasher/l10n/app_localizations.dart';
import 'package:aw_flasher/main.dart';
import 'package:aw_flasher/settings/app_settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  Future<FlasherModel> pumpApp(WidgetTester tester) async {
    final settings = await AppSettings.load();
    final model = FlasherModel();
    await tester.pumpWidget(FlasherApp(settings: settings, model: model));
    await tester.pump();
    return model;
  }

  testWidgets('quits normally when idle', (tester) async {
    await pumpApp(tester);
    expect(await tester.binding.handleRequestAppExit(), AppExitResponse.exit);
    await tester.pump();
    expect(find.byType(AlertDialog), findsNothing);
  });

  testWidgets('refuses to quit while flashing and says why', (tester) async {
    final model = await pumpApp(tester);
    model.phase = Phase.flashing;

    expect(await tester.binding.handleRequestAppExit(), AppExitResponse.cancel);
    await tester.pump();
    final l10n = AppLocalizations.of(tester.element(find.byType(Scaffold).first));
    expect(find.text(l10n.quitBlockedTitle), findsOneWidget);
    expect(find.text(l10n.quitBlockedBody), findsOneWidget);

    await tester.tap(find.text(l10n.commonClose));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);

    // Once the flash is over, quitting works again.
    model.phase = Phase.succeeded;
    expect(await tester.binding.handleRequestAppExit(), AppExitResponse.exit);
  });

  testWidgets('loading an image is also work in progress', (tester) async {
    final model = await pumpApp(tester);
    model.phase = Phase.loadingImage;
    expect(model.busy, isTrue);
    expect(await tester.binding.handleRequestAppExit(), AppExitResponse.cancel);
  });
}
