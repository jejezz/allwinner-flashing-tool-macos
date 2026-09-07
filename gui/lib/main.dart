import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';

import 'about.dart';
import 'aw_tool.dart';
import 'flasher_model.dart';
import 'l10n/app_localizations.dart';
import 'theme.dart';
import 'widgets.dart';

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
      // The product name is the same in both languages, so it is not in the
      // ARB files.
      title: 'Allwinner Flasher',
      debugShowCheckedModeBanner: false,
      localizationsDelegates: AppLocalizations.localizationsDelegates,
      supportedLocales: AppLocalizations.supportedLocales,
      theme: AppTheme.light(),
      darkTheme: AppTheme.dark(),
      themeMode: ThemeMode.system,
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

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // The model produces a few user-facing strings of its own; the locale
    // lives here, so hand them over whenever it resolves or changes.
    model.l10n = AppLocalizations.of(context);
  }

  void _onChange() => setState(() {});

  /// Accent for the whole window, so its state reads from across a bench.
  Color get _tint => switch (model.phase) {
        Phase.flashing => AppColors.accent,
        Phase.succeeded => AppColors.success,
        Phase.failed => AppColors.danger,
        _ => AppColors.primary,
      };

  Future<void> _pickImage() async {
    final picked = await FilePicker.pickFile(
      dialogTitle: AppLocalizations.of(context).pickerTitle,
      type: FileType.custom,
      allowedExtensions: ['img'],
    );
    final path = picked?.path;
    if (path != null) await model.loadImage(path);
  }

  Future<void> _confirmAndFlash() async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => _ConfirmDialog(model: model),
    );
    if (ok == true) await model.flash();
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: AuroraBackground(
        tint: _tint,
        child: SafeArea(
          child: model.toolError != null
              ? const _ToolMissing()
              : Padding(
                  padding: const EdgeInsets.fromLTRB(22, 18, 22, 22),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      _Header(model: model),
                      const SizedBox(height: 18),
                      Expanded(
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Expanded(
                              flex: 5,
                              child: _ControlColumn(
                                model: model,
                                onPick: _pickImage,
                                onFlash: _confirmAndFlash,
                              ),
                            ),
                            const SizedBox(width: 16),
                            Expanded(
                              flex: 4,
                              child: _DetailColumn(model: model),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
        ),
      ),
    );
  }
}

/// Wording for a device state. Kept here rather than on [DeviceStatus] so the
/// model layer stays free of localisations.
String deviceLabel(AppLocalizations l10n, DeviceStatus d) => switch (d.state) {
      DeviceState.fel =>
        d.socHex == null ? l10n.deviceFel : l10n.deviceFelWithSoc(d.socHex!),
      DeviceState.efex => l10n.deviceEfex,
      DeviceState.none => l10n.deviceWaiting,
    };

/// Short form for the header pill, which sits next to the card showing
/// [deviceLabel] — repeating the same sentence twice reads as a bug. The mode
/// names are protocol terms and are not translated.
String deviceShortLabel(AppLocalizations l10n, DeviceStatus d) =>
    switch (d.state) {
      DeviceState.fel => 'FEL',
      DeviceState.efex => 'EFEX',
      DeviceState.none => l10n.deviceShortNone,
    };

class _Header extends StatelessWidget {
  const _Header({required this.model});

  final FlasherModel model;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    final device = model.device;
    final (pillColor, pillIcon) = switch (device.state) {
      DeviceState.fel => (AppColors.primary, Icons.usb),
      DeviceState.efex => (AppColors.success, Icons.bolt),
      DeviceState.none => (AppColors.idle, null),
    };

    return Row(
      children: [
        const IconBadge(
          icon: Icons.memory,
          color: AppColors.accent,
          active: true,
          size: 44,
        ),
        const SizedBox(width: 14),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text('Allwinner Flasher', style: theme.textTheme.titleLarge),
              const SizedBox(height: 2),
              Text(l10n.appSubtitle, style: theme.textTheme.labelSmall),
            ],
          ),
        ),
        StatusPill(
          label: deviceShortLabel(l10n, device),
          color: pillColor,
          icon: pillIcon,
          pulsing: !device.connected,
        ),
        const SizedBox(width: 10),
        IconButton(
          tooltip: l10n.aboutTooltip,
          icon: const Icon(Icons.info_outline, size: 20),
          onPressed: () => showAboutSheet(context),
        ),
      ],
    );
  }
}

class _ControlColumn extends StatelessWidget {
  const _ControlColumn({
    required this.model,
    required this.onPick,
    required this.onFlash,
  });

  final FlasherModel model;
  final VoidCallback onPick;
  final VoidCallback onFlash;

  @override
  Widget build(BuildContext context) {
    return SingleChildScrollView(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _DeviceCard(model: model),
          const SizedBox(height: 14),
          _ImageCard(model: model, onPick: onPick),
          const SizedBox(height: 14),
          _OptionsCard(model: model),
          const SizedBox(height: 18),
          _ActionButton(model: model, onFlash: onFlash),
          if (model.phase != Phase.idle &&
              model.phase != Phase.loadingImage) ...[
            const SizedBox(height: 14),
            _ProgressCard(model: model),
          ],
          // Only while nothing has run yet: once there is progress to read,
          // the steps are just noise.
          if (model.phase == Phase.idle || model.phase == Phase.loadingImage)
            ...[
            const SizedBox(height: 14),
            _StepsCard(model: model),
          ],
        ],
      ),
    );
  }
}

/// The three things that have to be true before flashing, each ticking off as
/// it is satisfied — the window doubles as the instructions.
class _StepsCard extends StatelessWidget {
  const _StepsCard({required this.model});

  final FlasherModel model;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return GlassCard(
      padding: const EdgeInsets.fromLTRB(18, 16, 18, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SectionHeader(title: l10n.stepsTitle),
          _Step(
            index: 1,
            label: l10n.step1,
            hint: l10n.step1Hint,
            done: model.device.connected,
          ),
          _Step(
            index: 2,
            label: l10n.step2,
            hint: l10n.step2Hint,
            done: model.partitions.isNotEmpty,
          ),
          _Step(
            index: 3,
            label: l10n.step3,
            hint: l10n.step3Hint,
            done: false,
          ),
        ],
      ),
    );
  }
}

class _Step extends StatelessWidget {
  const _Step({
    required this.index,
    required this.label,
    required this.hint,
    required this.done,
  });

  final int index;
  final String label;
  final String hint;
  final bool done;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final color = done ? AppColors.success : AppColors.idle;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 5),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 22,
            height: 22,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: color.withValues(alpha: done ? 0.9 : 0.14),
            ),
            child: done
                ? Icon(Icons.check,
                    size: 14, color: Colors.black.withValues(alpha: 0.85))
                : Text(
                    '$index',
                    style: TextStyle(
                      fontSize: 11,
                      fontWeight: FontWeight.w700,
                      color: color,
                    ),
                  ),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  label,
                  style: theme.textTheme.labelLarge?.copyWith(
                    color: done ? AppColors.success : null,
                  ),
                ),
                const SizedBox(height: 1),
                Text(hint, style: theme.textTheme.labelSmall),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _DeviceCard extends StatelessWidget {
  const _DeviceCard({required this.model});

  final FlasherModel model;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    final device = model.device;
    final connected = device.connected;
    final accent =
        device.state == DeviceState.efex ? AppColors.success : AppColors.primary;

    return GlassCard(
      accent: accent,
      active: connected,
      child: Row(
        children: [
          IconBadge(
            icon: connected ? Icons.usb : Icons.usb_off,
            color: connected ? accent : AppColors.idle,
            active: connected,
          ),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  deviceLabel(l10n, device),
                  style: theme.textTheme.titleMedium,
                ),
                const SizedBox(height: 3),
                Text(
                  connected ? l10n.deviceReady : l10n.deviceHint,
                  style: theme.textTheme.labelSmall,
                ),
              ],
            ),
          ),
        ],
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
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    final path = model.imagePath;
    final loading = model.phase == Phase.loadingImage;
    final loaded = model.partitions.isNotEmpty;

    return GlassCard(
      accent: AppColors.accent,
      active: loaded,
      child: Row(
        children: [
          IconBadge(
            icon: loaded ? Icons.inventory_2 : Icons.folder_open,
            color: loaded ? AppColors.accent : AppColors.idle,
            active: loaded,
          ),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  path == null ? l10n.imageTitle : path.split('/').last,
                  style: theme.textTheme.titleMedium,
                  overflow: TextOverflow.ellipsis,
                ),
                const SizedBox(height: 3),
                Text(
                  loading
                      ? l10n.imageLoading
                      : loaded
                          ? l10n.imageSummary(
                              model.selectedCount,
                              model.flashableCount,
                            )
                          : l10n.imageHint,
                  style: theme.textTheme.labelSmall,
                ),
              ],
            ),
          ),
          const SizedBox(width: 8),
          TextButton(
            onPressed: model.busy ? null : onPick,
            child: Text(path == null ? l10n.choose : l10n.change),
          ),
        ],
      ),
    );
  }
}

class _OptionsCard extends StatelessWidget {
  const _OptionsCard({required this.model});

  final FlasherModel model;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return GlassCard(
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
      child: Column(
        children: [
          _OptionRow(
            icon: Icons.cleaning_services,
            title: l10n.optionFullFormat,
            subtitle: model.fullFormat
                ? l10n.optionFullFormatOn
                : l10n.optionFullFormatOff,
            value: model.fullFormat,
            onChanged: model.busy ? null : (v) => model.formatFully = v,
          ),
          Divider(indent: 62, endIndent: 12, color: insetFill(context)),
          _OptionRow(
            icon: Icons.restart_alt,
            title: l10n.optionReboot,
            subtitle: l10n.optionRebootHint,
            value: model.rebootWhenDone,
            onChanged: model.busy ? null : (v) => model.rebootAfter = v,
          ),
        ],
      ),
    );
  }
}

class _OptionRow extends StatelessWidget {
  const _OptionRow({
    required this.icon,
    required this.title,
    required this.subtitle,
    required this.value,
    required this.onChanged,
  });

  final IconData icon;
  final String title;
  final String subtitle;
  final bool value;
  final ValueChanged<bool>? onChanged;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final enabled = onChanged != null;
    return Opacity(
      opacity: enabled ? 1 : 0.5,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
        child: Row(
          children: [
            IconBadge(icon: icon, color: AppColors.primary, size: 36),
            const SizedBox(width: 14),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(title, style: theme.textTheme.labelLarge),
                  const SizedBox(height: 2),
                  Text(subtitle, style: theme.textTheme.labelSmall),
                ],
              ),
            ),
            Switch(value: value, onChanged: onChanged),
          ],
        ),
      ),
    );
  }
}

class _ActionButton extends StatelessWidget {
  const _ActionButton({required this.model, required this.onFlash});

  final FlasherModel model;
  final VoidCallback onFlash;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    if (model.phase == Phase.flashing) {
      return FilledButton.icon(
        style: FilledButton.styleFrom(backgroundColor: AppColors.danger),
        onPressed: model.cancel,
        icon: const Icon(Icons.stop_circle_outlined),
        label: Text(l10n.flashStop),
      );
    }
    return FilledButton.icon(
      style: FilledButton.styleFrom(
        backgroundColor: AppColors.accent,
        disabledBackgroundColor: AppColors.idle.withValues(alpha: 0.22),
      ),
      onPressed: model.canFlash ? onFlash : null,
      icon: const Icon(Icons.bolt),
      label: Text(
        model.phase == Phase.succeeded ? l10n.flashAgain : l10n.flashStart,
      ),
    );
  }
}

class _ProgressCard extends StatelessWidget {
  const _ProgressCard({required this.model});

  final FlasherModel model;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);

    if (model.phase == Phase.failed) {
      return GlassCard(
        accent: AppColors.danger,
        active: true,
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const IconBadge(
              icon: Icons.error_outline,
              color: AppColors.danger,
              active: true,
            ),
            const SizedBox(width: 14),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(l10n.resultFailed, style: theme.textTheme.titleMedium),
                  const SizedBox(height: 4),
                  Text(
                    model.errorMessage ?? l10n.resultUnknownError,
                    style: theme.textTheme.labelSmall,
                  ),
                ],
              ),
            ),
          ],
        ),
      );
    }

    if (model.phase == Phase.succeeded) {
      return GlassCard(
        accent: AppColors.success,
        active: true,
        child: Row(
          children: [
            const IconBadge(
              icon: Icons.check_circle_outline,
              color: AppColors.success,
              active: true,
            ),
            const SizedBox(width: 14),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(l10n.resultDone, style: theme.textTheme.titleMedium),
                  const SizedBox(height: 3),
                  Text(
                    model.rebootWhenDone
                        ? l10n.resultDoneRebooted
                        : l10n.resultDoneNoReboot,
                    style: theme.textTheme.labelSmall,
                  ),
                ],
              ),
            ),
          ],
        ),
      );
    }

    final current = model.currentPartition;
    return GlassCard(
      accent: AppColors.accent,
      active: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Text(
                  current ?? l10n.progressPreparing,
                  style: theme.textTheme.titleMedium?.copyWith(
                    fontFamily: current == null ? null : kMonoFamily,
                    fontSize: current == null ? null : 15,
                  ),
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              if (current != null)
                Text(
                  '${model.currentIndex} / ${model.partitionCount}',
                  style: theme.textTheme.labelSmall,
                ),
            ],
          ),
          const SizedBox(height: 12),
          GlassProgressBar(
            value: model.overallProgress,
            color: AppColors.accent,
          ),
          const SizedBox(height: 8),
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(
                l10n.progressOverall(
                  (model.overallProgress * 100).toStringAsFixed(0),
                ),
                style: theme.textTheme.labelSmall,
              ),
              if (current != null)
                Text(
                  l10n.progressPartition(
                    (model.partitionProgress * 100).toStringAsFixed(0),
                  ),
                  style: theme.textTheme.labelSmall,
                ),
            ],
          ),
          if (current != null) ...[
            const SizedBox(height: 6),
            GlassProgressBar(
              value: model.partitionProgress,
              color: AppColors.primary,
              height: 4,
            ),
          ],
        ],
      ),
    );
  }
}

class _DetailColumn extends StatelessWidget {
  const _DetailColumn({required this.model});

  final FlasherModel model;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(flex: 4, child: _PartitionCard(model: model)),
        const SizedBox(height: 14),
        Expanded(flex: 5, child: _LogCard(lines: model.log)),
      ],
    );
  }
}

class _PartitionCard extends StatelessWidget {
  const _PartitionCard({required this.model});

  final FlasherModel model;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    final parts = model.partitions;

    return GlassCard(
      padding: const EdgeInsets.fromLTRB(18, 16, 10, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.only(right: 8),
            child: SectionHeader(
              title: l10n.partitionsTitle,
              subtitle: parts.isEmpty
                  ? null
                  : model.selectionEnabled
                      ? l10n.partitionsSelectable(model.selectedCount)
                      : l10n.partitionsFullFormat,
              trailing: model.hasDeselection
                  ? TextButton(
                      onPressed: model.busy ? null : model.selectAll,
                      child: Text(l10n.selectAll),
                    )
                  : null,
            ),
          ),
          Expanded(
            child: parts.isEmpty
                ? Center(
                    child: Text(
                      l10n.partitionsEmpty,
                      style: theme.textTheme.labelSmall,
                    ),
                  )
                : Scrollbar(
                    child: ListView.builder(
                      padding: const EdgeInsets.only(right: 10),
                      itemCount: parts.length,
                      itemBuilder: (context, i) =>
                          _PartitionRow(part: parts[i], model: model),
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}

class _PartitionRow extends StatelessWidget {
  const _PartitionRow({required this.part, required this.model});

  final PartitionInfo part;
  final FlasherModel model;

  @override
  Widget build(BuildContext context) {
    final isCurrent = model.currentPartition == part.name;
    // Nothing to write means nothing to choose: the row is informational.
    final hasPayload = part.downloadFile != null;
    final selected = hasPayload && model.isSelected(part.name);
    final canToggle = hasPayload && model.selectionEnabled && !model.busy;

    final mono = TextStyle(
      fontFamily: kMonoFamily,
      fontSize: 11,
      color: selected ? AppColors.hi(context) : AppColors.mid(context),
    );

    return Container(
      margin: const EdgeInsets.only(bottom: 3),
      decoration: BoxDecoration(
        color: isCurrent
            ? AppColors.accent.withValues(alpha: 0.18)
            : Colors.transparent,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(
          color: isCurrent
              ? AppColors.accent.withValues(alpha: 0.45)
              : Colors.transparent,
        ),
      ),
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          borderRadius: BorderRadius.circular(10),
          onTap: canToggle ? () => model.toggle(part.name) : null,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 5),
            child: Row(
              children: [
                _Mark(
                  hasPayload: hasPayload,
                  selected: selected,
                  selectable: canToggle,
                  isCurrent: isCurrent,
                ),
                const SizedBox(width: 10),
                Expanded(
                  flex: 3,
                  child: Text(
                    part.name,
                    style: mono,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                Expanded(
                  flex: 2,
                  child: Text(
                    hasPayload && !selected
                        // Start sector is in addrlo space, matching
                        // sys_partition.fex and the flashing commands — not
                        // the GPT LBA a U-Boot shell reports, which sits
                        // 0xa000 sectors higher.
                        ? AppLocalizations.of(context).partitionKeep
                        : '0x${part.startSector.toRadixString(16)}',
                    style: mono.copyWith(
                      color: hasPayload && !selected
                          ? AppColors.warning
                          : AppColors.mid(context),
                    ),
                    textAlign: TextAlign.right,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Leading indicator: a checkbox where there is a choice to make, a plain dot
/// where there is not (full-format mode, or a partition with no payload).
class _Mark extends StatelessWidget {
  const _Mark({
    required this.hasPayload,
    required this.selected,
    required this.selectable,
    required this.isCurrent,
  });

  final bool hasPayload;
  final bool selected;
  final bool selectable;
  final bool isCurrent;

  @override
  Widget build(BuildContext context) {
    if (!selectable) {
      return Container(
        width: 16,
        height: 16,
        alignment: Alignment.center,
        child: Container(
          width: 6,
          height: 6,
          decoration: BoxDecoration(
            shape: BoxShape.circle,
            color: !hasPayload
                ? AppColors.idle.withValues(alpha: 0.5)
                : (isCurrent ? AppColors.accent : AppColors.primary),
          ),
        ),
      );
    }

    return AnimatedContainer(
      duration: const Duration(milliseconds: 160),
      width: 16,
      height: 16,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(5),
        color: selected ? AppColors.primary : Colors.transparent,
        border: Border.all(
          color: selected
              ? AppColors.primary
              : AppColors.idle.withValues(alpha: 0.7),
          width: 1.5,
        ),
      ),
      child: selected
          ? const Icon(Icons.check, size: 11, color: Colors.white)
          : null,
    );
  }
}

class _LogCard extends StatefulWidget {
  const _LogCard({required this.lines});

  final List<String> lines;

  @override
  State<_LogCard> createState() => _LogCardState();
}

class _LogCardState extends State<_LogCard> {
  final _scroll = ScrollController();

  @override
  void didUpdateWidget(_LogCard old) {
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
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    return GlassCard(
      padding: const EdgeInsets.fromLTRB(18, 16, 10, 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.only(right: 8),
            child: SectionHeader(title: l10n.logTitle),
          ),
          Expanded(
            child: Container(
              decoration: BoxDecoration(
                color: insetFill(context),
                borderRadius: BorderRadius.circular(14),
              ),
              padding: const EdgeInsets.fromLTRB(12, 10, 4, 10),
              child: widget.lines.isEmpty
                  ? Center(
                      child: Text(
                        l10n.logEmpty,
                        style: theme.textTheme.labelSmall,
                      ),
                    )
                  : Scrollbar(
                      controller: _scroll,
                      child: ListView.builder(
                        controller: _scroll,
                        padding: const EdgeInsets.only(right: 10),
                        itemCount: widget.lines.length,
                        itemBuilder: (context, i) => Padding(
                          padding: const EdgeInsets.only(bottom: 2),
                          child: Text(
                            widget.lines[i],
                            style: TextStyle(
                              fontFamily: kMonoFamily,
                              fontSize: 11,
                              height: 1.35,
                              color: AppColors.mid(context),
                            ),
                          ),
                        ),
                      ),
                    ),
            ),
          ),
        ],
      ),
    );
  }
}

class _ConfirmDialog extends StatelessWidget {
  const _ConfirmDialog({required this.model});

  final FlasherModel model;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    return AlertDialog(
      icon: const IconBadge(
        icon: Icons.warning_amber_rounded,
        color: AppColors.warning,
        active: true,
      ),
      title: Text(l10n.confirmTitle),
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 380),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              model.fullFormat
                  ? l10n.confirmFullFormat
                  : l10n.confirmPartial(model.selectedCount),
              style: theme.textTheme.bodyLarge,
            ),
            if (model.keptNames.isNotEmpty) ...[
              const SizedBox(height: 12),
              _KeptList(names: model.keptNames),
            ],
            const SizedBox(height: 12),
            Text(
              model.fullFormat
                  ? l10n.confirmWarningFull
                  : l10n.confirmWarningPartial,
              style: theme.textTheme.bodyMedium,
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: Text(l10n.cancel),
        ),
        FilledButton(
          style: FilledButton.styleFrom(
            backgroundColor: AppColors.accent,
            minimumSize: const Size(120, 44),
          ),
          onPressed: () => Navigator.pop(context, true),
          child: Text(l10n.confirmFlash),
        ),
      ],
    );
  }
}

/// The partitions being left alone, spelled out in the confirmation — this is
/// the reason the user turned full format off, so it should be the thing they
/// can check before committing.
class _KeptList extends StatelessWidget {
  const _KeptList({required this.names});

  final List<String> names;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
      decoration: BoxDecoration(
        color: AppColors.success.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: AppColors.success.withValues(alpha: 0.35)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              const Icon(Icons.shield_outlined,
                  size: 15, color: AppColors.success),
              const SizedBox(width: 7),
              Text(
                l10n.confirmKeptTitle,
                style: theme.textTheme.labelLarge
                    ?.copyWith(color: AppColors.success),
              ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            names.join(', '),
            style: const TextStyle(fontFamily: kMonoFamily, fontSize: 11.5),
          ),
        ],
      ),
    );
  }
}

class _ToolMissing extends StatelessWidget {
  const _ToolMissing();

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 460),
        child: GlassCard(
          accent: AppColors.danger,
          active: true,
          padding: const EdgeInsets.all(26),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              const IconBadge(
                icon: Icons.link_off,
                color: AppColors.danger,
                active: true,
                size: 52,
              ),
              const SizedBox(height: 16),
              Text(l10n.toolMissingTitle, style: theme.textTheme.titleMedium),
              const SizedBox(height: 8),
              Text(
                l10n.toolMissingBody,
                textAlign: TextAlign.center,
                style: theme.textTheme.bodyMedium,
              ),
            ],
          ),
        ),
      ),
    );
  }
}
