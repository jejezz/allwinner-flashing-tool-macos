import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';

import 'aw_tool.dart';

enum Phase { idle, loadingImage, flashing, succeeded, failed }

/// Everything the UI reads. Owns the device poll, the loaded image and the
/// running flash.
class FlasherModel extends ChangeNotifier {
  FlasherModel();

  AwTool? _tool;
  String? toolError;

  Phase phase = Phase.idle;
  DeviceStatus device = DeviceStatus.disconnected;

  String? imagePath;
  String? _sysPartitionPath;
  List<PartitionInfo> partitions = [];

  /// Full format (erase_flag=1) versus overwrite-only.
  bool fullFormat = true;
  bool rebootWhenDone = true;

  set formatFully(bool v) {
    fullFormat = v;
    notifyListeners();
  }

  set rebootAfter(bool v) {
    rebootWhenDone = v;
    notifyListeners();
  }

  final List<String> log = [];
  String? errorMessage;

  // Progress, in bytes, so the bar tracks real work rather than step count —
  // `super` alone is most of a flash.
  int _totalBytes = 0;
  int _doneBytes = 0;
  int _currentBytes = 0;
  double _currentFraction = 0;
  String? currentPartition;
  int currentIndex = 0;
  int partitionCount = 0;

  AwRun? _run;
  Timer? _poll;

  bool get busy => phase == Phase.flashing || phase == Phase.loadingImage;
  bool get canFlash =>
      _tool != null &&
      imagePath != null &&
      _sysPartitionPath != null &&
      device.connected &&
      !busy;

  double get overallProgress {
    if (_totalBytes == 0) return 0;
    final done = _doneBytes + _currentBytes * _currentFraction;
    return (done / _totalBytes).clamp(0.0, 1.0);
  }

  double get partitionProgress => _currentFraction;

  void start() {
    try {
      _tool = AwTool.locate();
      // Which helper this build ended up using is the first thing worth
      // knowing when a flash misbehaves — bundled copy or a stale local build.
      debugPrint('aw-tool: ${_tool!.executable}');
    } on AwToolMissing catch (e) {
      toolError = e.message;
      notifyListeners();
      return;
    }
    _poll = Timer.periodic(const Duration(milliseconds: 1500), (_) => _probe());
    _probe();
  }

  @override
  void dispose() {
    _poll?.cancel();
    super.dispose();
  }

  /// Poll for a connected board.
  ///
  /// Skipped while a flash is running: the CLI holds an exclusive claim on the
  /// USB interface, so a concurrent probe would both fail and pointlessly
  /// contend with the transfer in flight.
  Future<void> _probe() async {
    if (busy || _tool == null) return;
    try {
      final events = await _tool!.collect(['probe']);
      final probe = events.where((e) => e.event == 'probe').firstOrNull;
      final state = switch (probe?.raw['state']) {
        'fel' => DeviceState.fel,
        'efex' => DeviceState.efex,
        _ => DeviceState.none,
      };
      final next = DeviceStatus(
        state,
        socId: probe?.raw['soc_id'] as int?,
        mode: probe?.raw['mode'] as int?,
      );
      if (next.state != device.state || next.socId != device.socId) {
        device = next;
        notifyListeners();
      }
    } on ProcessException {
      // The helper vanished mid-session; the next poll will report it again.
    }
  }

  /// Load an image: pull `sys_partition.fex` out of it and read the partition
  /// table, so the user can see what is about to be written before committing.
  Future<void> loadImage(String path) async {
    if (_tool == null) return;
    phase = Phase.loadingImage;
    imagePath = path;
    partitions = [];
    _sysPartitionPath = null;
    errorMessage = null;
    notifyListeners();

    try {
      final dir = await Directory.systemTemp.createTemp('aw_flasher');
      final fex = '${dir.path}/sys_partition.fex';

      final extract = await _tool!.collect([
        'extract',
        path,
        'sys_partition.fex',
        '--out',
        fex,
      ]);
      _failIfError(extract, 'sys_partition.fex 추출');

      final listed = await _tool!.collect(['list-partitions', fex]);
      _failIfError(listed, '파티션 목록 읽기');

      final data = listed.where((e) => e.event == 'partitions').firstOrNull;
      final raw = (data?.raw['partitions'] as List?) ?? const [];
      partitions = raw
          .cast<Map<String, dynamic>>()
          .map(PartitionInfo.new)
          .toList();
      _sysPartitionPath = fex;
      phase = Phase.idle;
    } catch (e) {
      errorMessage = '$e';
      phase = Phase.failed;
    }
    notifyListeners();
  }

  void _failIfError(List<AwEvent> events, String what) {
    final err = events.where((e) => e.event == 'error').firstOrNull;
    if (err != null) {
      throw Exception('$what 실패: ${err.message}');
    }
  }

  Future<void> flash() async {
    if (!canFlash) return;

    phase = Phase.flashing;
    log.clear();
    errorMessage = null;
    _totalBytes = 0;
    _doneBytes = 0;
    _currentBytes = 0;
    _currentFraction = 0;
    currentPartition = null;
    currentIndex = 0;
    partitionCount = 0;
    notifyListeners();

    try {
      final run = await _tool!.start([
        'flash-all',
        imagePath!,
        _sysPartitionPath!,
        '--erase-flag',
        fullFormat ? '1' : '0',
        if (rebootWhenDone) '--reboot',
      ]);
      _run = run;

      await for (final e in run.events) {
        _apply(e);
        notifyListeners();
      }

      final code = await run.exitCode;
      if (phase == Phase.flashing) {
        // The stream ended without a `done` or `error` event — the process was
        // killed, or died without reporting.
        phase = Phase.failed;
        errorMessage ??= code == 0
            ? '플래싱이 결과를 보고하지 않고 종료했습니다.'
            : '플래싱이 중단되었습니다 (종료 코드 $code).\n'
                '기록이 끝나지 않은 상태라 보드는 부팅되지 않습니다. 다시 플래싱하세요.';
      }
    } catch (e) {
      phase = Phase.failed;
      errorMessage = '$e';
    } finally {
      _run = null;
      notifyListeners();
    }
  }

  void _apply(AwEvent e) {
    switch (e.event) {
      case 'plan':
        _totalBytes = e.intOr('total_bytes', 0);
        partitionCount = e.intOr('partitions', 0);
        log.add(
          '계획: 파티션 $partitionCount개, ${_mb(_totalBytes)} MB',
        );
      case 'step':
        if (e.message != null) log.add(e.message!);
      case 'log':
        if (e.message != null) log.add(e.message!);
      case 'partition_begin':
        currentPartition = e.name;
        currentIndex = e.intOr('index', 0);
        partitionCount = e.intOr('total', partitionCount);
        _currentBytes = e.intOr('bytes', 0);
        _currentFraction = 0;
      case 'progress':
        final total = e.intOr('total', 0);
        _currentFraction = total == 0 ? 0 : e.intOr('written', 0) / total;
      case 'partition_end':
        _doneBytes += e.intOr('bytes', 0);
        _currentBytes = 0;
        _currentFraction = 0;
        log.add(
          "[${e.intOr('index', 0)}/${e.intOr('total', 0)}] ${e.name} "
          "(${e.raw['format']}, ${e.doubleOr('seconds', 0).toStringAsFixed(1)}s)",
        );
        currentPartition = null;
      case 'error':
        phase = Phase.failed;
        errorMessage = e.message;
        log.add('오류: ${e.message}');
      case 'done':
        phase = Phase.succeeded;
        _currentFraction = 0;
        currentPartition = null;
    }
  }

  /// Stop a running flash. The board is left mid-write and will not boot.
  void cancel() {
    _run?.cancel();
  }

  static int _mb(int bytes) => bytes ~/ (1024 * 1024);
}

extension _FirstOrNull<T> on Iterable<T> {
  T? get firstOrNull => isEmpty ? null : first;
}
