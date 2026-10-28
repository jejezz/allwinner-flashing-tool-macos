import 'package:aw_flasher/aw_tool.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('DiskInfo reads what list-disks emits', () {
    final d = DiskInfo({
      'id': r'\\.\PhysicalDrive2',
      'display_name': 'SanDisk Ultra',
      'size_bytes': 32000000000,
      'removable': true,
      'is_system_disk': false,
    });
    expect(d.shortId, 'PhysicalDrive2');
    expect(d.sizeGB, closeTo(32.0, 0.001));
    expect(d.removable, isTrue);
    expect(d.isSystemDisk, isFalse);
  });

  test('macOS ids drop the /dev/ prefix', () {
    expect(DiskInfo({'id': '/dev/disk4'}).shortId, 'disk4');
  });

  test('a disk missing its safety fields is treated as unsafe', () {
    final d = DiskInfo({'id': 'x'});
    expect(d.isSystemDisk, isTrue);
    expect(d.removable, isFalse);
  });
}
