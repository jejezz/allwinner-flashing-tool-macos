import 'package:aw_flasher/aw_tool.dart';
import 'package:aw_flasher/flasher_model.dart';
import 'package:aw_flasher/l10n/app_localizations.dart';
import 'package:aw_flasher/sdcard_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

DiskInfo _disk(String id, String name,
        {bool removable = true, bool system = false}) =>
    DiskInfo({
      'id': id,
      'display_name': name,
      'size_bytes': 32000000000,
      'removable': removable,
      'is_system_disk': system,
    });

Future<FlasherModel> _open(WidgetTester tester) async {
  final model = FlasherModel()
    ..sdDisks = [
      _disk(r'\\.\PhysicalDrive0', 'Internal NVMe', removable: false, system: true),
      _disk(r'\\.\PhysicalDrive1', 'External SSD', removable: false),
      _disk(r'\\.\PhysicalDrive2', 'SanDisk Ultra'),
    ];
  await tester.pumpWidget(MaterialApp(
    locale: const Locale('en'),
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    supportedLocales: AppLocalizations.supportedLocales,
    home: Scaffold(body: SdCardDialog(model: model)),
  ));
  return model;
}

bool _enabled(WidgetTester tester, String label) =>
    tester.widget<FilledButton>(find.widgetWithText(FilledButton, label)).onPressed != null;

void main() {
  testWidgets('the system disk cannot be selected', (tester) async {
    await _open(tester);
    expect(find.text('System disk'), findsOneWidget);
    await tester.tap(find.text('Internal NVMe'));
    await tester.pump();
    expect(_enabled(tester, 'Next'), isFalse);
  });

  testWidgets('a removable disk goes on to a retype-the-name confirmation', (tester) async {
    await _open(tester);
    expect(_enabled(tester, 'Next'), isFalse);

    await tester.tap(find.text('SanDisk Ultra'));
    await tester.pump();
    expect(_enabled(tester, 'Next'), isTrue);

    await tester.tap(find.text('Next'));
    await tester.pump();
    expect(find.text('Write to this SD card?'), findsOneWidget);
    expect(_enabled(tester, 'Write'), isFalse);

    await tester.enterText(find.byType(TextField), 'PhysicalDrive1');
    await tester.pump();
    expect(_enabled(tester, 'Write'), isFalse);

    await tester.enterText(find.byType(TextField), 'physicaldrive2');
    await tester.pump();
    expect(_enabled(tester, 'Write'), isTrue);
  });

  testWidgets('a non-removable disk needs an explicit acknowledgement', (tester) async {
    await _open(tester);
    await tester.tap(find.text('External SSD'));
    await tester.pump();
    expect(_enabled(tester, 'Next'), isFalse);

    await tester.tap(find.byType(Checkbox));
    await tester.pump();
    expect(_enabled(tester, 'Next'), isTrue);
  });

  testWidgets('switching disks clears the acknowledgement', (tester) async {
    await _open(tester);
    await tester.tap(find.text('External SSD'));
    await tester.pump();
    await tester.tap(find.byType(Checkbox));
    await tester.pump();

    await tester.tap(find.text('SanDisk Ultra'));
    await tester.pump();
    await tester.tap(find.text('External SSD'));
    await tester.pump();
    expect(_enabled(tester, 'Next'), isFalse);
  });
}
