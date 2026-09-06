import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import 'aw_tool.dart';
import 'flasher_model.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  // We ship without the App Sandbox (internal tool needing raw USB), so the
  // app does not declare the user-selected-file entitlements that file_picker
  // checks for. Without this, its own pre-flight check blocks the dialog.
  FilePicker.skipEntitlementsChecks();
  runApp(const FlasherApp());
}

class FlasherApp extends StatelessWidget {
  const FlasherApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Allwinner Flasher',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        colorSchemeSeed: Colors.indigo,
        useMaterial3: true,
      ),
      home: const HomePage(),
    );
  }
}

class HomePage extends StatefulWidget {
  const HomePage({super.key});

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  final model = FlasherModel();

  @override
  void initState() {
    super.initState();
    model.addListener(_onChange);
    model.start();
  }

  @override
  void dispose() {
    model.removeListener(_onChange);
    model.dispose();
    super.dispose();
  }

  void _onChange() => setState(() {});

  Future<void> _pickImage() async {
    final picked = await FilePicker.pickFile(
      dialogTitle: '펌웨어 이미지 선택',
      type: FileType.custom,
      allowedExtensions: ['img'],
    );
    final path = picked?.path;
    if (path != null) await model.loadImage(path);
  }

  Future<void> _confirmAndFlash() async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('보드를 플래싱할까요?'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              model.fullFormat
                  ? '저장소를 전체 포맷한 뒤 기록합니다.'
                  : '전체 포맷 없이 덮어쓰기만 합니다.',
            ),
            const SizedBox(height: 8),
            const Text(
              '보드의 기존 내용은 사라지며 되돌릴 수 없습니다. '
              '작업 중에는 케이블을 뽑거나 전원을 끊지 마세요.',
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('취소'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('플래싱'),
          ),
        ],
      ),
    );
    if (ok == true) await model.flash();
  }

  @override
  Widget build(BuildContext context) {
    if (model.toolError != null) {
      return Scaffold(
        body: Center(
          child: Padding(
            padding: const EdgeInsets.all(32),
            child: Text(
              model.toolError!,
              textAlign: TextAlign.center,
              style: Theme.of(context).textTheme.bodyLarge,
            ),
          ),
        ),
      );
    }

    return Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(20),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              _DeviceCard(model: model),
              const SizedBox(height: 12),
              _ImageCard(model: model, onPick: _pickImage),
              const SizedBox(height: 12),
              _OptionsRow(model: model),
              const SizedBox(height: 16),
              _ActionBar(model: model, onFlash: _confirmAndFlash),
              const SizedBox(height: 16),
              if (model.phase == Phase.flashing ||
                  model.phase == Phase.succeeded ||
                  model.phase == Phase.failed)
                _ProgressSection(model: model),
              const SizedBox(height: 12),
              Expanded(child: _LogPane(lines: model.log)),
            ],
          ),
        ),
      ),
    );
  }
}

class _DeviceCard extends StatelessWidget {
  const _DeviceCard({required this.model});

  final FlasherModel model;

  @override
  Widget build(BuildContext context) {
    final connected = model.device.connected;
    final scheme = Theme.of(context).colorScheme;
    return Card(
      child: ListTile(
        leading: Icon(
          connected ? Icons.usb : Icons.usb_off,
          color: connected ? scheme.primary : scheme.outline,
        ),
        title: Text(model.device.label),
        subtitle: Text(
          connected
              ? '연결됨'
              : 'FEL 버튼(또는 핀)을 누른 채 보드에 전원을 연결하세요.',
        ),
      ),
    );
  }
}

class _ImageCard extends StatelessWidget {
  const _ImageCard({required this.model, required this.onPick});

  final FlasherModel model;
  final VoidCallback onPick;

  @override
  Widget build(BuildContext context) {
    final path = model.imagePath;
    final loading = model.phase == Phase.loadingImage;
    return Card(
      child: Column(
        children: [
          ListTile(
            leading: const Icon(Icons.folder_open),
            title: Text(
              path == null ? '펌웨어 이미지를 선택하세요' : path.split('/').last,
              overflow: TextOverflow.ellipsis,
            ),
            subtitle: loading
                ? const Text('파티션 정보를 읽는 중…')
                : model.partitions.isEmpty
                    ? (path == null ? null : const Text('파티션 정보 없음'))
                    : Text(
                        '파티션 ${model.partitions.length}개 · '
                        '기록 대상 ${model.partitions.where((p) => p.downloadFile != null).length}개',
                      ),
            trailing: FilledButton.tonal(
              onPressed: model.busy ? null : onPick,
              child: const Text('선택'),
            ),
          ),
          if (model.partitions.isNotEmpty)
            _PartitionTable(partitions: model.partitions),
        ],
      ),
    );
  }
}

class _PartitionTable extends StatelessWidget {
  const _PartitionTable({required this.partitions});

  final List<PartitionInfo> partitions;

  @override
  Widget build(BuildContext context) {
    final mono = Theme.of(context)
        .textTheme
        .bodySmall
        ?.copyWith(fontFamily: 'Menlo', fontSize: 11);
    return SizedBox(
      height: 130,
      child: Scrollbar(
        child: ListView.builder(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
          itemCount: partitions.length,
          itemBuilder: (context, i) {
            final p = partitions[i];
            return Padding(
              padding: const EdgeInsets.symmetric(vertical: 1),
              child: Row(
                children: [
                  SizedBox(width: 150, child: Text(p.name, style: mono)),
                  SizedBox(
                    width: 120,
                    // addrlo space, as used by sys_partition.fex and the
                    // flashing commands — not the GPT LBA the U-Boot shell
                    // reports, which sits 0xa000 sectors higher.
                    child: Text(
                      'start 0x${p.startSector.toRadixString(16)}',
                      style: mono,
                    ),
                  ),
                  Expanded(
                    child: Text(
                      p.downloadFile ?? '—',
                      style: mono,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
                ],
              ),
            );
          },
        ),
      ),
    );
  }
}

class _OptionsRow extends StatelessWidget {
  const _OptionsRow({required this.model});

  final FlasherModel model;

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Expanded(
          child: SwitchListTile(
            title: const Text('전체 포맷'),
            subtitle: const Text('끄면 덮어쓰기만 합니다'),
            value: model.fullFormat,
            onChanged: model.busy ? null : (v) => model.formatFully = v,
          ),
        ),
        Expanded(
          child: SwitchListTile(
            title: const Text('완료 후 재부팅'),
            value: model.rebootWhenDone,
            onChanged: model.busy ? null : (v) => model.rebootAfter = v,
          ),
        ),
      ],
    );
  }
}

class _ActionBar extends StatelessWidget {
  const _ActionBar({required this.model, required this.onFlash});

  final FlasherModel model;
  final VoidCallback onFlash;

  @override
  Widget build(BuildContext context) {
    if (model.phase == Phase.flashing) {
      return FilledButton.icon(
        style: FilledButton.styleFrom(
          minimumSize: const Size.fromHeight(48),
          backgroundColor: Theme.of(context).colorScheme.error,
        ),
        onPressed: model.cancel,
        icon: const Icon(Icons.stop),
        label: const Text('중단'),
      );
    }
    return FilledButton.icon(
      style: FilledButton.styleFrom(minimumSize: const Size.fromHeight(48)),
      onPressed: model.canFlash ? onFlash : null,
      icon: const Icon(Icons.bolt),
      label: const Text('플래싱 시작'),
    );
  }
}

class _ProgressSection extends StatelessWidget {
  const _ProgressSection({required this.model});

  final FlasherModel model;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final theme = Theme.of(context).textTheme;

    if (model.phase == Phase.failed) {
      return Card(
        color: scheme.errorContainer,
        child: ListTile(
          leading: Icon(Icons.error_outline, color: scheme.onErrorContainer),
          title: Text('실패', style: TextStyle(color: scheme.onErrorContainer)),
          subtitle: Text(
            model.errorMessage ?? '알 수 없는 오류',
            style: TextStyle(color: scheme.onErrorContainer),
          ),
        ),
      );
    }

    if (model.phase == Phase.succeeded) {
      return Card(
        color: scheme.primaryContainer,
        child: ListTile(
          leading: Icon(Icons.check_circle, color: scheme.onPrimaryContainer),
          title: Text(
            '완료',
            style: TextStyle(color: scheme.onPrimaryContainer),
          ),
          subtitle: Text(
            model.rebootWhenDone ? '보드를 재부팅했습니다.' : '기록이 끝났습니다.',
            style: TextStyle(color: scheme.onPrimaryContainer),
          ),
        ),
      );
    }

    final label = model.currentPartition == null
        ? '준비 중…'
        : '${model.currentIndex}/${model.partitionCount}  ${model.currentPartition}';

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text(label, style: theme.bodyMedium),
            Text(
              '${(model.overallProgress * 100).toStringAsFixed(0)}%',
              style: theme.bodyMedium,
            ),
          ],
        ),
        const SizedBox(height: 6),
        LinearProgressIndicator(value: model.overallProgress, minHeight: 8),
        if (model.currentPartition != null) ...[
          const SizedBox(height: 4),
          LinearProgressIndicator(
            value: model.partitionProgress,
            minHeight: 3,
            backgroundColor: Colors.transparent,
          ),
        ],
      ],
    );
  }
}

class _LogPane extends StatefulWidget {
  const _LogPane({required this.lines});

  final List<String> lines;

  @override
  State<_LogPane> createState() => _LogPaneState();
}

class _LogPaneState extends State<_LogPane> {
  final _scroll = ScrollController();

  @override
  void didUpdateWidget(_LogPane old) {
    super.didUpdateWidget(old);
    // Follow the tail as the flash proceeds.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scroll.hasClients) {
        _scroll.jumpTo(_scroll.position.maxScrollExtent);
      }
    });
  }

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      decoration: BoxDecoration(
        color: scheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(8),
      ),
      padding: const EdgeInsets.all(12),
      child: Scrollbar(
        controller: _scroll,
        child: ListView.builder(
          controller: _scroll,
          itemCount: widget.lines.length,
          itemBuilder: (context, i) => Text(
            widget.lines[i],
            style: const TextStyle(fontFamily: 'Menlo', fontSize: 11),
          ),
        ),
      ),
    );
  }
}
