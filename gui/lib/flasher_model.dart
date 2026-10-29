import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';

import 'aw_tool.dart';
import 'l10n/app_localizations.dart';

enum Phase { idle, loadingImage, flashing, succeeded, failed }

/// Card fusing runs independently of the USB flash above: a different target
/// (a disk on this computer) with its own progress and result.
enum SdCardPhase { idle, enumerating, writing, succeeded, failed }

/// Everything the UI reads. Owns the device poll, the loaded image and the
/// running flash.
class FlasherModel extends ChangeNotifier {
  FlasherModel();

  AwTool? _tool;
  String? toolError;

  /// Supplied by the widget tree, which is where the locale lives. The few
  /// user-facing strings the model produces itself (log lines, its own error
  /// wording) go through this; everything else in the log comes from the CLI
  /// and stays in the CLI's own English.
  AppLocalizations? _l10n;

  set l10n(AppLocalizations value) => _l10n = value;

  Phase phase = Phase.idle;
  DeviceStatus device = DeviceStatus.disconnected;

  String? imagePath;
  String? _sysPartitionPath;
  List<PartitionInfo> partitions = [];

  /// Full format (erase_flag=1) versus overwrite-only.
  bool fullFormat = true;
  bool rebootWhenDone = true;

  /// Partitions the user has unchecked, to be left as they are on the board.
  /// Held as the exclusion set rather than the inclusion set so that loading a
  /// different image defaults to writing everything.
  final Set<String> _keep = {};

  /// Per-partition choice only means something in overwrite-only mode: a full
  /// format erases every partition first, so an unchecked one would end up
  /// blank rather than preserved. The CLI refuses the combination outright.
  bool get selectionEnabled => !fullFormat;

  bool isSelected(String name) => fullFormat || !_keep.contains(name);

  /// Partitions that have something to write and are checked.
  int get selectedCount => partitions
      .where((p) => p.downloadFile != null && isSelected(p.name))
      .length;

  bool get hasDeselection => selectionEnabled && _keep.isNotEmpty;

  /// Unchecked partitions that would otherwise have been written, in table
  /// order — what the confirmation dialog promises to leave alone.
  List<String> get keptNames => selectionEnabled
      ? partitions
            .where((p) => p.downloadFile != null && _keep.contains(p.name))
            .map((p) => p.name)
            .toList()
      : const [];

  void toggle(String name) {
    if (!selectionEnabled) return;
    if (!_keep.remove(name)) _keep.add(name);
    notifyListeners();
  }

  void selectAll() {
    _keep.clear();
    notifyListeners();
  }

  /// Every partition with something to write is checked.
  bool get allSelected =>
      partitions.every((p) => p.downloadFile == null || isSelected(p.name));

  /// Uncheck everything, so a single partition can then be ticked on its own.
  void deselectAll() {
    _keep
      ..clear()
      ..addAll(
        partitions.where((p) => p.downloadFile != null).map((p) => p.name),
      );
    notifyListeners();
  }

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

  bool get busy =>
      phase == Phase.flashing || phase == Phase.loadingImage || sdBusy;

  /// Partitions that actually have something to write.
  int get flashableCount =>
      partitions.where((p) => p.downloadFile != null).length;
  bool get canFlash =>
      _tool != null &&
      imagePath != null &&
      _sysPartitionPath != null &&
      device.connected &&
      selectedCount > 0 &&
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
    // A different image means a different partition table; carrying the old
    // exclusions over would silently protect names that may not exist here.
    _keep.clear();
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
      _failIfError(extract, _l10n?.stepExtract ?? 'extract');

      final listed = await _tool!.collect(['list-partitions', fex]);
      _failIfError(listed, _l10n?.stepListPartitions ?? 'list-partitions');

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
      throw Exception(
        _l10n?.failedStep(what, err.message ?? '') ??
            '$what failed: ${err.message}',
      );
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
      // `--skip` rather than `--only`: the exclusion set is what the user
      // actually chose, and it keeps the command short and readable in a log.
      final keeping = _keep.toList()..sort();
      final run = await _tool!.start([
        'flash-all',
        imagePath!,
        _sysPartitionPath!,
        '--erase-flag',
        fullFormat ? '1' : '0',
        if (rebootWhenDone) '--reboot',
        if (selectionEnabled && keeping.isNotEmpty) ...[
          '--skip',
          keeping.join(','),
        ],
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
            ? _l10n?.errorNoResult ?? 'Flashing exited without a result.'
            : _l10n?.errorInterrupted(code) ??
                  'Flashing was interrupted (exit code $code).';
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
          _l10n?.planLine(partitionCount, _mb(_totalBytes)) ??
              'plan: $partitionCount partitions, ${_mb(_totalBytes)} MB',
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
        log.add(_l10n?.errorPrefix(e.message ?? '') ?? 'error: ${e.message}');
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

  // --- Card fusing -------------------------------------------------------

  SdCardPhase sdCardPhase = SdCardPhase.idle;
  List<DiskInfo> sdDisks = [];
  String? sdCardError;

  /// Which safety rule the CLI refused on (`error.kind`), when it was one.
  String? sdCardErrorKind;
  String? sdCurrentRegion;

  /// Past the writing: the card is being read back and compared.
  bool sdVerifying = false;
  final Map<String, int> _sdVerified = {};

  int _sdTotalBytes = 0;
  int _sdDoneBytes = 0;
  int _sdCurrentBytes = 0;
  double _sdCurrentFraction = 0;
  AwRun? _sdRun;

  /// What the GUI offers as a target: removable media that is not the system
  /// disk, nothing else. The CLI's `--force` exists for experts; the window
  /// never shows an internal or external hard drive, so it cannot be picked.
  List<DiskInfo> get sdTargets =>
      sdDisks.where((d) => d.removable && !d.isSystemDisk).toList();

  bool get sdBusy =>
      sdCardPhase == SdCardPhase.writing ||
      sdCardPhase == SdCardPhase.enumerating;

  /// A loaded image is all card fusing needs — no board has to be connected.
  bool get canOpenSdCardDialog =>
      _tool != null && imagePath != null && !busy && partitions.isNotEmpty;

  /// Writing is the first half of the bar and reading back the second: the same
  /// bytes cross twice.
  double get sdCardProgress {
    if (_sdTotalBytes == 0) return 0;
    final verified = _sdVerified.values.fold(0, (a, b) => a + b);
    final done = sdVerifying
        ? _sdDoneBytes + verified
        : _sdDoneBytes + _sdCurrentBytes * _sdCurrentFraction;
    return (done / (_sdTotalBytes * 2)).clamp(0.0, 1.0);
  }

  Future<void> loadSdDisks() async {
    if (_tool == null || sdCardPhase == SdCardPhase.writing) return;
    sdCardPhase = SdCardPhase.enumerating;
    sdCardError = null;
    notifyListeners();
    try {
      final events = await _tool!.collect(['list-disks']);
      final err = events.where((e) => e.event == 'error').firstOrNull;
      if (err != null) {
        sdDisks = [];
        sdCardError = err.message;
      } else {
        final data = events.where((e) => e.event == 'disks').firstOrNull;
        sdDisks = ((data?.raw['disks'] as List?) ?? const [])
            .cast<Map<String, dynamic>>()
            .map(DiskInfo.new)
            .toList();
        // No `disks` event at all means the helper does not know the command —
        // an aw-tool built before card fusing existed. Saying "no disks found"
        // there would send the user looking at their card reader instead.
        if (data == null) {
          sdCardError =
              _l10n?.sdCardToolOutdated ??
              'This aw-tool does not support listing disks. Rebuild it.';
        }
      }
    } catch (e) {
      sdDisks = [];
      sdCardError = '$e';
    }
    sdCardPhase = SdCardPhase.idle;
    notifyListeners();
  }

  /// Write the loaded image to [disk]. [force] is the user's explicit
  /// acknowledgement that a disk not reported as removable really is their card.
  Future<void> startSdCardFlash(DiskInfo disk, {required bool force}) async {
    if (_tool == null || imagePath == null || busy) return;
    sdCardPhase = SdCardPhase.writing;
    sdCardError = null;
    sdCardErrorKind = null;
    sdCurrentRegion = null;
    sdVerifying = false;
    _sdVerified.clear();
    _sdTotalBytes = 0;
    _sdDoneBytes = 0;
    _sdCurrentBytes = 0;
    _sdCurrentFraction = 0;
    notifyListeners();

    try {
      final run = await _tool!.start([
        'flash-sdcard',
        imagePath!,
        disk.id,
        if (force) '--force',
      ]);
      _sdRun = run;
      await for (final e in run.events) {
        _applySdCard(e);
        notifyListeners();
      }
      final code = await run.exitCode;
      if (sdCardPhase == SdCardPhase.writing) {
        sdCardPhase = SdCardPhase.failed;
        sdCardError ??= code == 0
            ? _l10n?.errorNoResult ?? 'Writing exited without a result.'
            : run.stderrText.trim().isNotEmpty
            ? run.stderrText.trim()
            : 'exit code $code';
      }
    } catch (e) {
      sdCardPhase = SdCardPhase.failed;
      sdCardError = '$e';
    } finally {
      _sdRun = null;
      notifyListeners();
    }
  }

  void _applySdCard(AwEvent e) {
    switch (e.event) {
      case 'card_plan':
        _sdTotalBytes = e.intOr('total_bytes', 0);
      case 'partition_begin':
        sdCurrentRegion = e.name;
        _sdCurrentBytes = e.intOr('bytes', 0);
        _sdCurrentFraction = 0;
      case 'step':
        if (e.step == 'verifying') {
          sdVerifying = true;
          sdCurrentRegion = null;
        }
      case 'progress':
        final total = e.intOr('total', 0);
        final name = e.name ?? '';
        if (name.startsWith('verify:')) {
          sdVerifying = true;
          sdCurrentRegion = name.substring('verify:'.length);
          _sdVerified[name] = e.intOr('written', 0);
        } else {
          _sdCurrentFraction = total == 0 ? 0 : e.intOr('written', 0) / total;
        }
      case 'partition_end':
        _sdDoneBytes += e.intOr('bytes', 0);
        _sdCurrentBytes = 0;
        _sdCurrentFraction = 0;
        sdCurrentRegion = null;
      case 'error':
        sdCardPhase = SdCardPhase.failed;
        sdCardError = e.message;
        sdCardErrorKind = e.raw['kind'] as String?;
      case 'done':
        sdCardPhase = SdCardPhase.succeeded;
        _sdCurrentFraction = 0;
        sdCurrentRegion = null;
    }
  }

  /// Stop a running card write. The card is left half-written and will not
  /// boot until it is written again.
  void cancelSdCardFlash() => _sdRun?.cancel();

  void resetSdCard() {
    if (sdBusy) return;
    sdCardPhase = SdCardPhase.idle;
    sdCardError = null;
    sdCardErrorKind = null;
    notifyListeners();
  }

  static int _mb(int bytes) => bytes ~/ (1024 * 1024);
}

extension _FirstOrNull<T> on Iterable<T> {
  T? get firstOrNull => isEmpty ? null : first;
}
