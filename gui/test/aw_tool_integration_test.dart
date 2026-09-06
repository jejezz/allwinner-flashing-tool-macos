import 'dart:io';

import 'package:aw_flasher/aw_tool.dart';
import 'package:flutter_test/flutter_test.dart';

/// Drives the real `aw-tool` binary, so the event schema the UI relies on is
/// checked against the CLI that actually produces it rather than a mock that
/// would keep passing after the schema changed.
///
/// Only the commands that touch no hardware are exercised here.
void main() {
  final repoRoot = Directory.current.parent.path;
  final binary = ['release', 'debug']
      .map((p) => File('$repoRoot/target/$p/aw-tool'))
      .where((f) => f.existsSync())
      .firstOrNull;

  if (binary == null) {
    // Nothing to test against; `cargo build` has not been run in this checkout.
    return;
  }

  final tool = AwTool(binary.path);

  test('probe reports a state even with no board attached', () async {
    final events = await tool.collect(['probe']);
    final probe = events.where((e) => e.event == 'probe').firstOrNull;

    expect(probe, isNotNull, reason: 'probe emitted no probe event');
    expect(
      probe!.raw['state'],
      anyOf('fel', 'efex', 'none'),
    );
    expect(events.last.event, 'done');
  });

  test('a failing command ends with an error event, not a silent stop',
      () async {
    final events = await tool.collect(['list', '/nonexistent.img']);
    final error = events.where((e) => e.event == 'error').firstOrNull;

    expect(error, isNotNull);
    expect(error!.message, isNotNull);
    expect(events.any((e) => e.event == 'done'), isFalse);
  });

  test('list-partitions parses into the model the UI renders', () async {
    final dir = await Directory.systemTemp.createTemp('aw_tool_test');
    final fex = File('${dir.path}/sys_partition.fex');
    await fex.writeAsString('''
[mbr]
size = 16384

[partition]
    name         = bootloader_a
    size         = 32768
    downloadfile = "bootloader.fex"

[partition]
    name         = userdata
    downloadfile = "userdata.fex"
''');

    final events = await tool.collect(['list-partitions', fex.path]);
    final data = events.where((e) => e.event == 'partitions').firstOrNull;
    expect(data, isNotNull);

    final parts = (data!.raw['partitions'] as List)
        .cast<Map<String, dynamic>>()
        .map(PartitionInfo.new)
        .toList();

    expect(parts.map((p) => p.name), ['bootloader_a', 'userdata']);
    // First partition starts where the MBR ends: 16384 KB / 512 B.
    expect(parts.first.startSector, 32768);
    // The trailing partition declares no size — it fills the remainder.
    expect(parts.last.sizeSectors, isNull);

    await dir.delete(recursive: true);
  });
}

extension<T> on Iterable<T> {
  // `this.` is load-bearing: bare `isEmpty` resolves to flutter_test's matcher
  // of the same name, not to the iterable's property.
  T? get firstOrNull => this.isEmpty ? null : first;
}
