import 'dart:convert';
import 'dart:io';

/// Wrapper around the `aw-tool` CLI.
///
/// The GUI drives the CLI as a subprocess rather than linking the Rust code in.
/// USB work here can genuinely wedge — a bad command has left a board printing
/// `INVALID direction` with the transfer stuck until timeout — and a separate
/// process means that hangs a process we can kill, not the UI. It also keeps
/// the hardware-verified flashing path untouched by GUI changes.
class AwTool {
  AwTool(this.executable);

  final String executable;

  /// Find the helper binary: bundled inside the .app for a release build,
  /// otherwise the cargo build output of the enclosing checkout so the app can
  /// be run straight from `flutter run`.
  static AwTool locate() {
    final exeDir = File(Platform.resolvedExecutable).parent;

    // Release: Contents/MacOS/aw_flasher -> Contents/Resources/aw-tool
    final bundled = File(
      '${exeDir.parent.path}/Resources/aw-tool',
    );
    if (bundled.existsSync()) return AwTool(bundled.path);

    // Development: walk up looking for the cargo target dir.
    for (var dir = exeDir; dir.parent.path != dir.path; dir = dir.parent) {
      for (final profile in ['release', 'debug']) {
        final candidate = File('${dir.path}/target/$profile/aw-tool');
        if (candidate.existsSync()) return AwTool(candidate.path);
      }
    }

    throw AwToolMissing(
      'aw-tool 실행 파일을 찾지 못했습니다.\n'
      '개발 중이라면 저장소 루트에서 `cargo build --release`를 먼저 실행하세요.',
    );
  }

  /// Run a command to completion and return every event it emitted.
  Future<List<AwEvent>> collect(List<String> args) async {
    final run = await start(args);
    final events = await run.events.toList();
    await run.exitCode;
    return events;
  }

  /// Start a command, streaming its JSON events as they arrive.
  Future<AwRun> start(List<String> args) async {
    final process = await Process.start(executable, [...args, '--json']);
    return AwRun._(process);
  }
}

class AwToolMissing implements Exception {
  AwToolMissing(this.message);
  final String message;
  @override
  String toString() => message;
}

/// A running `aw-tool` invocation.
class AwRun {
  AwRun._(this._process) {
    _process.stderr.transform(utf8.decoder).listen(_stderr.write);
    events = _process.stdout
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .map(_parse)
        .where((e) => e != null)
        .cast<AwEvent>()
        .asBroadcastStream();
  }

  final Process _process;
  final StringBuffer _stderr = StringBuffer();

  late final Stream<AwEvent> events;

  Future<int> get exitCode => _process.exitCode;

  /// Anything the tool wrote to stderr. Normally empty: failures arrive as an
  /// `error` event on stdout.
  String get stderrText => _stderr.toString();

  /// Kill the run. The device is left wherever the interrupted command put it,
  /// which for a partial flash means it will not boot until reflashed.
  void cancel() => _process.kill();

  static AwEvent? _parse(String line) {
    if (line.trim().isEmpty) return null;
    try {
      final decoded = jsonDecode(line);
      if (decoded is Map<String, dynamic> && decoded['event'] is String) {
        return AwEvent(decoded);
      }
    } on FormatException {
      // Not our JSON — a stray line from a library underneath us. Dropping it
      // is better than derailing a flash in progress.
    }
    return null;
  }
}

/// One newline-delimited JSON event. Kept as a map rather than a class per
/// event type: the CLI owns the schema, and unknown fields should pass through
/// rather than fail to parse.
class AwEvent {
  AwEvent(this.raw);

  final Map<String, dynamic> raw;

  String get event => raw['event'] as String;
  String? get message => raw['message'] as String?;
  String? get step => raw['step'] as String?;
  String? get name => raw['name'] as String?;

  int intOr(String key, int fallback) {
    final v = raw[key];
    return v is int ? v : fallback;
  }

  double doubleOr(String key, double fallback) {
    final v = raw[key];
    return v is num ? v.toDouble() : fallback;
  }
}

/// What is on the other end of the USB cable.
enum DeviceState {
  /// Boot ROM recovery mode — where a board must start for a full flash.
  fel,

  /// Already past the bootstrap, ready for flashing commands.
  efex,

  none,
}

class DeviceStatus {
  const DeviceStatus(this.state, {this.socId, this.mode});

  final DeviceState state;
  final int? socId;
  final int? mode;

  static const disconnected = DeviceStatus(DeviceState.none);

  bool get connected => state != DeviceState.none;

  String get label => switch (state) {
        DeviceState.fel => socId == null
            ? 'FEL 모드'
            : 'FEL 모드 (soc 0x${socId!.toRadixString(16).padLeft(4, '0')})',
        DeviceState.efex => 'EFEX 모드',
        DeviceState.none => '장치 없음',
      };
}

/// A partition as computed from `sys_partition.fex`.
class PartitionInfo {
  PartitionInfo(Map<String, dynamic> m)
      : name = m['name'] as String? ?? '?',
        startSector = m['start_sector'] as int? ?? 0,
        sizeSectors = m['size_sectors'] as int?,
        downloadFile = m['downloadfile'] as String?;

  final String name;
  final int startSector;

  /// Null for the trailing partition, which fills whatever is left.
  final int? sizeSectors;

  /// Null when nothing is flashed to this partition.
  final String? downloadFile;
}
