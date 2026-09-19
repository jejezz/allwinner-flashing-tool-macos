import 'package:flutter/material.dart';

import 'l10n/app_localizations.dart';
import 'theme.dart';
import 'widgets.dart';

/// Shown by the wrench icon in the header, Windows only. The WinUSB binding
/// step is easy to miss — the board looks perfectly normal in Device
/// Manager while libusb still can't see it at all — so the fix lives here,
/// next to where the "device not found" error actually happens, rather than
/// only in the README.
Future<void> showTroubleshootSheet(BuildContext context) {
  return showDialog<void>(
    context: context,
    builder: (context) => const _TroubleshootDialog(),
  );
}

class _TroubleshootDialog extends StatelessWidget {
  const _TroubleshootDialog();

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);
    final steps = [
      l10n.troubleshootStep1,
      l10n.troubleshootStep2,
      l10n.troubleshootStep3,
      l10n.troubleshootStep4,
      l10n.troubleshootStep5,
    ];

    return AlertDialog(
      icon: const IconBadge(
        icon: Icons.troubleshoot,
        color: AppColors.warning,
        active: true,
      ),
      title: Text(l10n.troubleshootTitle),
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 440),
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(l10n.troubleshootIntro, style: theme.textTheme.bodyMedium),
              const SizedBox(height: 12),
              // The exact CLI error, verbatim — this is the signature to
              // match against, not prose, so it stays in one language.
              Container(
                width: double.infinity,
                padding:
                    const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                decoration: BoxDecoration(
                  color: insetFill(context),
                  borderRadius: BorderRadius.circular(10),
                ),
                child: const Text(
                  'Error: Allwinner USB FEL device (1f3a:efe8) not found',
                  style: TextStyle(fontFamily: kMonoFamily, fontSize: 11.5),
                ),
              ),
              const SizedBox(height: 16),
              for (var i = 0; i < steps.length; i++)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 5),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Container(
                        width: 20,
                        height: 20,
                        alignment: Alignment.center,
                        decoration: BoxDecoration(
                          shape: BoxShape.circle,
                          color: AppColors.primary.withValues(alpha: 0.16),
                        ),
                        child: Text(
                          '${i + 1}',
                          style: const TextStyle(
                            fontSize: 11,
                            fontWeight: FontWeight.w700,
                            color: AppColors.primary,
                          ),
                        ),
                      ),
                      const SizedBox(width: 10),
                      Expanded(
                        child: Text(steps[i],
                            style: theme.textTheme.bodyMedium),
                      ),
                    ],
                  ),
                ),
              const SizedBox(height: 10),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Icon(Icons.info_outline,
                      size: 15, color: AppColors.idle),
                  const SizedBox(width: 7),
                  Expanded(
                    child: Text(
                      l10n.troubleshootNote,
                      style: theme.textTheme.labelSmall,
                    ),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
      actions: [
        FilledButton(
          style: FilledButton.styleFrom(
            backgroundColor: AppColors.accent,
            minimumSize: const Size(110, 44),
          ),
          onPressed: () => Navigator.pop(context),
          child: const Text('OK'),
        ),
      ],
    );
  }
}
