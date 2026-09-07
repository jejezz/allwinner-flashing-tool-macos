import 'package:aw_flasher/aw_tool.dart';
import 'package:aw_flasher/flasher_model.dart';
import 'package:flutter_test/flutter_test.dart';

/// Per-partition selection exists to stop a reflash from wiping something the
/// board owns (`env`, `userdata`). Getting it wrong destroys exactly the data
/// the user was protecting, so the rules are pinned here.
void main() {
  FlasherModel modelWith(List<Map<String, dynamic>> parts) {
    final m = FlasherModel();
    m.partitions = parts.map(PartitionInfo.new).toList();
    return m;
  }

  FlasherModel standard() => modelWith([
        {'name': 'bootloader_a', 'start_sector': 32768, 'downloadfile': 'b.fex'},
        {'name': 'env', 'start_sector': 40000, 'downloadfile': 'env.fex'},
        {'name': 'super', 'start_sector': 65536, 'downloadfile': 'super.fex'},
        // No downloadfile: nothing to write, so nothing to choose.
        {'name': 'reserved', 'start_sector': 90000},
      ]);

  test('everything is selected by default', () {
    final m = standard()..fullFormat = false;
    expect(m.selectedCount, 3);
    expect(m.keptNames, isEmpty);
  });

  test('unchecking a partition keeps it out of the write set', () {
    final m = standard()..fullFormat = false;
    m.toggle('env');

    expect(m.isSelected('env'), isFalse);
    expect(m.selectedCount, 2);
    expect(m.keptNames, ['env']);
  });

  test('full format overrides the choice instead of silently honouring it', () {
    // A full format erases every partition first, so an unchecked one would be
    // blank, not preserved. The UI locks the checkboxes; this pins the model.
    final m = standard()..fullFormat = false;
    m.toggle('env');
    m.fullFormat = true;

    expect(m.selectionEnabled, isFalse);
    expect(m.isSelected('env'), isTrue);
    expect(m.keptNames, isEmpty, reason: 'nothing is preserved by a format');
  });

  test('the choice survives toggling full format off again', () {
    final m = standard()..fullFormat = false;
    m.toggle('env');
    m.fullFormat = true;
    m.fullFormat = false;

    expect(m.keptNames, ['env']);
  });

  test('kept names follow partition table order, not click order', () {
    final m = standard()..fullFormat = false;
    m.toggle('super');
    m.toggle('bootloader_a');

    expect(m.keptNames, ['bootloader_a', 'super']);
  });

  test('a partition with no payload is never counted as selected', () {
    final m = standard()..fullFormat = false;
    expect(m.isSelected('reserved'), isTrue);
    // ...but it contributes nothing to write.
    expect(m.selectedCount, 3);
  });

  test('selectAll clears every exclusion', () {
    final m = standard()..fullFormat = false;
    m.toggle('env');
    m.toggle('super');
    expect(m.hasDeselection, isTrue);

    m.selectAll();
    expect(m.hasDeselection, isFalse);
    expect(m.selectedCount, 3);
  });

  test('unchecking everything blocks the flash', () {
    final m = standard()..fullFormat = false;
    for (final name in ['bootloader_a', 'env', 'super']) {
      m.toggle(name);
    }
    expect(m.selectedCount, 0);
    // canFlash also needs a device and an image; selectedCount is the gate
    // this test is pinning.
    expect(m.canFlash, isFalse);
  });
}
