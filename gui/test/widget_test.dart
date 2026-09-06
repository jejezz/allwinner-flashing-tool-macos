import 'package:aw_flasher/aw_tool.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('AwEvent', () {
    test('reads the fields the progress UI depends on', () {
      final e = AwEvent({
        'event': 'partition_end',
        'index': 6,
        'total': 12,
        'name': 'super',
        'bytes': 1021182504,
        'format': 'sparse',
        'seconds': 74.3,
      });

      expect(e.event, 'partition_end');
      expect(e.name, 'super');
      expect(e.intOr('index', 0), 6);
      expect(e.doubleOr('seconds', 0), closeTo(74.3, 0.001));
    });

    test('falls back rather than throwing on a missing or mistyped field', () {
      final e = AwEvent({'event': 'progress', 'written': 'nope'});
      expect(e.intOr('written', -1), -1);
      expect(e.intOr('total', 0), 0);
      expect(e.name, isNull);
    });
  });

  group('PartitionInfo', () {
    test('keeps a null size for the partition that fills the remainder', () {
      final p = PartitionInfo({
        'name': 'userdata',
        'start_sector': 7405568,
        'size_sectors': null,
        'downloadfile': 'userdata.fex',
      });

      expect(p.name, 'userdata');
      expect(p.sizeSectors, isNull);
      expect(p.downloadFile, 'userdata.fex');
    });

    test('tolerates a partition with nothing to flash', () {
      final p = PartitionInfo({'name': 'empty', 'start_sector': 0});
      expect(p.downloadFile, isNull);
    });
  });

  group('DeviceStatus', () {
    test('renders the SoC id in the hex form the docs use', () {
      const s = DeviceStatus(DeviceState.fel, socId: 0x1890);
      expect(s.label, contains('0x1890'));
      expect(s.connected, isTrue);
    });

    test('disconnected is not connected', () {
      expect(DeviceStatus.disconnected.connected, isFalse);
    });
  });
}
