import 'package:flutter/material.dart';

import 'aw_tool.dart';
import 'flasher_model.dart';
import 'l10n/app_localizations.dart';
import 'theme.dart';
import 'widgets.dart';

/// Card fusing writes to a disk on this computer, so a wrong pick destroys
/// something the user cares about. The whole flow therefore lives in one dialog
/// that nothing but its own buttons can dismiss.
Future<void> showSdCardDialog(BuildContext context, FlasherModel model) {
  model.resetSdCard();
  model.loadSdDisks();
  return showDialog<void>(
    context: context,
    barrierDismissible: false,
    builder: (_) => SdCardDialog(model: model),
  );
}

enum _Step { select, confirm }

class SdCardDialog extends StatefulWidget {
  const SdCardDialog({super.key, required this.model});

  final FlasherModel model;

  @override
  State<SdCardDialog> createState() => _SdCardDialogState();
}

class _SdCardDialogState extends State<SdCardDialog> {
  _Step _step = _Step.select;
  DiskInfo? _selected;
  bool _acknowledged = false;
  final _typed = TextEditingController();

  FlasherModel get model => widget.model;

  @override
  void dispose() {
    _typed.dispose();
    super.dispose();
  }

  /// A disk that is not reported as removable is allowed only once the user
  /// has said, in so many words, that it is their card.
  bool get _canContinue {
    final d = _selected;
    if (d == null || d.isSystemDisk) return false;
    return d.removable || _acknowledged;
  }

  bool get _typedMatches =>
      _selected != null &&
      _typed.text.trim().toLowerCase() == _selected!.shortId.toLowerCase();

  void _select(DiskInfo d) {
    setState(() {
      _selected = d;
      _acknowledged = false;
    });
  }

  Future<void> _confirmStop() async {
    final l10n = AppLocalizations.of(context);
    final ok = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (context) => AlertDialog(
        icon: const IconBadge(
          icon: Icons.warning_amber_rounded,
          color: AppColors.danger,
          active: true,
        ),
        title: Text(l10n.sdCardConfirmStopTitle),
        content: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 380),
          child: Text(l10n.sdCardConfirmStopWarning),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text(l10n.cancel),
          ),
          FilledButton(
            style: FilledButton.styleFrom(
              backgroundColor: AppColors.danger,
              minimumSize: const Size(120, 44),
            ),
            onPressed: () => Navigator.pop(context, true),
            child: Text(l10n.flashStop),
          ),
        ],
      ),
    );
    if (ok == true) model.cancelSdCardFlash();
  }

  @override
  Widget build(BuildContext context) {
    return PopScope(
      canPop: false,
      child: ListenableBuilder(
        listenable: model,
        builder: (context, _) => switch (model.sdCardPhase) {
          SdCardPhase.writing => _writing(context),
          SdCardPhase.succeeded => _result(context, ok: true),
          SdCardPhase.failed => _result(context, ok: false),
          _ => _step == _Step.select ? _pickStep(context) : _confirm(context),
        },
      ),
    );
  }

  // --- Pick a disk ---------------------------------------------------------

  Widget _pickStep(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    final loading = model.sdCardPhase == SdCardPhase.enumerating;
    final selected = _selected;

    return AlertDialog(
      scrollable: true,
      icon: const IconBadge(
        icon: Icons.sd_card,
        color: AppColors.warning,
        active: true,
      ),
      title: Text(l10n.sdCardDialogTitle),
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 460, minWidth: 380),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(l10n.sdCardSelectHint, style: theme.textTheme.bodyMedium),
            const SizedBox(height: 12),
            // Not a ListView: an AlertDialog measures its content's intrinsic
            // size, which a shrink-wrapped viewport refuses to provide.
            ConstrainedBox(
              constraints: const BoxConstraints(maxHeight: 220),
              child: loading
                  ? const Padding(
                      padding: EdgeInsets.all(24),
                      child: Center(child: CircularProgressIndicator()),
                    )
                  : model.sdCardError != null
                      ? Text(
                          model.sdCardError!,
                          style: theme.textTheme.bodyMedium?.copyWith(
                            color: AppColors.danger,
                          ),
                        )
                      : model.sdDisks.isEmpty
                          ? Text(l10n.sdCardNoDisks)
                          : SingleChildScrollView(
                              child: Column(
                                children: [
                                  for (final d in model.sdDisks)
                                    _DiskTile(
                                      disk: d,
                                      selected: selected?.id == d.id,
                                      onTap: d.isSystemDisk ? null : () => _select(d),
                                    ),
                                ],
                              ),
                            ),
            ),
            if (selected != null && !selected.removable && !selected.isSystemDisk) ...[
              const SizedBox(height: 8),
              CheckboxListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                controlAffinity: ListTileControlAffinity.leading,
                value: _acknowledged,
                onChanged: (v) => setState(() => _acknowledged = v ?? false),
                title: Text(
                  l10n.sdCardNotRemovableAck,
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: AppColors.danger,
                  ),
                ),
              ),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: loading ? null : model.loadSdDisks,
          child: Text(l10n.sdCardRefresh),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(l10n.cancel),
        ),
        FilledButton(
          style: FilledButton.styleFrom(minimumSize: const Size(100, 44)),
          onPressed: _canContinue && !loading
              ? () => setState(() {
                    _typed.clear();
                    _step = _Step.confirm;
                  })
              : null,
          child: Text(l10n.sdCardNext),
        ),
      ],
    );
  }

  // --- Retype the disk name ------------------------------------------------

  Widget _confirm(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    final d = _selected!;

    return AlertDialog(
      scrollable: true,
      icon: const IconBadge(
        icon: Icons.warning_amber_rounded,
        color: AppColors.danger,
        active: true,
      ),
      title: Text(l10n.sdCardConfirmTitle),
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 420),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              l10n.sdCardConfirmBody(d.displayName, d.sizeGB.toStringAsFixed(1)),
              style: theme.textTheme.bodyLarge,
            ),
            const SizedBox(height: 14),
            Text(l10n.sdCardTypeToVerify(d.shortId), style: theme.textTheme.bodyMedium),
            const SizedBox(height: 8),
            TextField(
              controller: _typed,
              autofocus: true,
              onChanged: (_) => setState(() {}),
              decoration: InputDecoration(
                isDense: true,
                border: const OutlineInputBorder(),
                hintText: d.shortId,
              ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => setState(() => _step = _Step.select),
          child: Text(l10n.sdCardBack),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(l10n.cancel),
        ),
        FilledButton(
          style: FilledButton.styleFrom(
            backgroundColor: AppColors.danger,
            minimumSize: const Size(120, 44),
          ),
          onPressed: _typedMatches
              ? () => model.startSdCardFlash(d, force: !d.removable)
              : null,
          child: Text(l10n.sdCardWrite),
        ),
      ],
    );
  }

  // --- Writing -------------------------------------------------------------

  Widget _writing(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    final region = model.sdCurrentRegion;
    final pct = (model.sdCardProgress * 100).toStringAsFixed(0);

    return AlertDialog(
      scrollable: true,
      icon: const IconBadge(
        icon: Icons.sd_card,
        color: AppColors.accent,
        active: true,
      ),
      title: Text(l10n.sdCardWriting),
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 420, minWidth: 380),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            LinearProgressIndicator(
              value: model.sdCardProgress == 0 ? null : model.sdCardProgress,
              minHeight: 8,
              borderRadius: BorderRadius.circular(4),
              color: AppColors.accent,
            ),
            const SizedBox(height: 10),
            Text(
              region == null
                  ? l10n.progressPreparing
                  : '${l10n.sdCardWritingRegion(region)}  ·  $pct%',
              style: theme.textTheme.bodyMedium,
            ),
            const SizedBox(height: 8),
            Text(l10n.sdCardKeepConnected, style: theme.textTheme.labelSmall),
          ],
        ),
      ),
      actions: [
        TextButton(
          style: TextButton.styleFrom(foregroundColor: AppColors.danger),
          onPressed: _confirmStop,
          child: Text(l10n.flashStop),
        ),
      ],
    );
  }

  // --- Result --------------------------------------------------------------

  Widget _result(BuildContext context, {required bool ok}) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);

    return AlertDialog(
      scrollable: true,
      icon: IconBadge(
        icon: ok ? Icons.check_circle : Icons.error_outline,
        color: ok ? AppColors.success : AppColors.danger,
        active: true,
      ),
      title: Text(ok ? l10n.sdCardSucceededTitle : l10n.sdCardFailedTitle),
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 440),
        child: ok
            ? Text(l10n.sdCardSucceededBody, style: theme.textTheme.bodyMedium)
            : Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  SelectableText(
                    model.sdCardError ?? '',
                    style: theme.textTheme.bodyMedium,
                  ),
                  if (model.sdCardErrorKind == null) ...[
                    const SizedBox(height: 10),
                    Text(l10n.sdCardAdminHint, style: theme.textTheme.labelSmall),
                  ],
                ],
              ),
      ),
      actions: [
        FilledButton(
          style: FilledButton.styleFrom(minimumSize: const Size(100, 44)),
          onPressed: () {
            model.resetSdCard();
            Navigator.pop(context);
          },
          child: Text(l10n.sdCardClose),
        ),
      ],
    );
  }
}

class _DiskTile extends StatelessWidget {
  const _DiskTile({
    required this.disk,
    required this.selected,
    required this.onTap,
  });

  final DiskInfo disk;
  final bool selected;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    final disabled = onTap == null;

    final (badge, color) = disk.isSystemDisk
        ? (l10n.sdCardBadgeSystem, AppColors.danger)
        : disk.removable
            ? (l10n.sdCardBadgeRemovable, AppColors.success)
            : (l10n.sdCardBadgeNotRemovable, AppColors.danger);

    return Opacity(
      opacity: disabled ? 0.5 : 1,
      child: ListTile(
        dense: true,
        enabled: !disabled,
        selected: selected,
        selectedTileColor: AppColors.accent.withValues(alpha: 0.12),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
        leading: Icon(
          selected ? Icons.radio_button_checked : Icons.radio_button_unchecked,
        ),
        title: Text(disk.displayName, overflow: TextOverflow.ellipsis),
        subtitle: Text(
          '${disk.shortId}  ·  ${disk.sizeGB.toStringAsFixed(1)} GB',
          style: theme.textTheme.labelSmall,
        ),
        trailing: Container(
          padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
          decoration: BoxDecoration(
            color: color.withValues(alpha: 0.16),
            borderRadius: BorderRadius.circular(6),
          ),
          child: Text(
            badge,
            style: theme.textTheme.labelSmall?.copyWith(color: color),
          ),
        ),
        onTap: onTap,
      ),
    );
  }
}
