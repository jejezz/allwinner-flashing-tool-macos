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

Future<FlasherModel> _open(WidgetTester tester, List<DiskInfo> disks) async {
  final model = FlasherModel()..sdDisks = disks;
  await tester.pumpWidget(MaterialApp(
    locale: const Locale('en'),
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    supportedLocales: AppLocalizations.supportedLocales,
    home: Scaffold(body: SdCardDialog(model: model)),
  ));
  return model;
}

final _mixed = [
  _disk(r'\\.\PhysicalDrive0', 'Internal NVMe', removable: false, system: true),
  _disk(r'\\.\PhysicalDrive1', 'External SSD', removable: false),
  _disk(r'\\.\PhysicalDrive2', 'SanDisk Ultra'),
];

bool _enabled(WidgetTester tester, String label) =>
    tester.widget<FilledButton>(find.widgetWithText(FilledButton, label)).onPressed != null;

void main() {
  testWidgets('only removable, non-system disks are listed', (tester) async {
    await _open(tester, _mixed);
    expect(find.text('SanDisk Ultra'), findsOneWidget);
    expect(find.text('Internal NVMe'), findsNothing);
    expect(find.text('External SSD'), findsNothing);
  });

  testWidgets('a removable flag on the system disk still hides it', (tester) async {
    await _open(tester, [
      _disk(r'\\.\PhysicalDrive0', 'Odd system disk', removable: true, system: true),
    ]);
    expect(find.text('Odd system disk'), findsNothing);
    expect(find.textContaining('No removable disks'), findsOneWidget);
  });

  testWidgets('with nothing removable the list says so and Next stays off', (tester) async {
    await _open(tester, _mixed.take(2).toList());
    expect(find.textContaining('No removable disks'), findsOneWidget);
    expect(_enabled(tester, 'Next'), isFalse);
  });

  testWidgets('picking a card leads to one erase warning, then Write', (tester) async {
    await _open(tester, _mixed);
    expect(_enabled(tester, 'Next'), isFalse);

    await tester.tap(find.text('SanDisk Ultra'));
    await tester.pump();
    expect(_enabled(tester, 'Next'), isTrue);

    await tester.tap(find.text('Next'));
    await tester.pump();
    expect(find.text('Write to this SD card?'), findsOneWidget);
    expect(find.textContaining('SanDisk Ultra'), findsOneWidget);
    expect(find.textContaining('cannot be recovered'), findsOneWidget);
    expect(find.byType(TextField), findsNothing);
    expect(_enabled(tester, 'Write'), isTrue);
  });

  testWidgets('a selected card that disappears can no longer be continued', (tester) async {
    final model = await _open(tester, _mixed);
    await tester.tap(find.text('SanDisk Ultra'));
    await tester.pump();
    expect(_enabled(tester, 'Next'), isTrue);

    model.sdDisks = _mixed.take(2).toList();
    model.notifyListeners();
    await tester.pump();
    expect(_enabled(tester, 'Next'), isFalse);
  });
}
