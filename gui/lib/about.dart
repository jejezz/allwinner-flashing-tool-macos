import 'package:flutter/material.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:url_launcher/url_launcher.dart';

import 'l10n/app_localizations.dart';
import 'theme.dart';
import 'widgets.dart';

const kRepositoryUrl =
    'https://github.com/jejezz/allwinner-flashing-tool-macos';

/// Shown by the ⓘ button in the header. The macOS menu bar's own "About" item
/// still opens the standard AppKit panel, which reads the bundle's name,
/// version and copyright — this one adds what the tool is actually for.
///
/// The version comes from the built bundle (Info.plist, the Windows version
/// resource, Linux's version.json), all of which Flutter fills in from
/// `version:` in pubspec.yaml — so bumping pubspec is the only step needed.
Future<void> showAboutSheet(BuildContext context) async {
  final info = await PackageInfo.fromPlatform();
  if (!context.mounted) return;
  final version = info.buildNumber.isEmpty
      ? info.version
      : '${info.version}+${info.buildNumber}';
  return showDialog<void>(
    context: context,
    builder: (context) => _AboutDialog(version: version),
  );
}

class _AboutDialog extends StatelessWidget {
  const _AboutDialog({required this.version});

  final String version;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final l10n = AppLocalizations.of(context);

    return AlertDialog(
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 400),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                const AppMark(size: 44),
                const SizedBox(width: 14),
                // Flexible, not fixed: the subtitle line is longer in some
                // languages, and the dialog is narrower than the window.
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        'Allwinner Flasher',
                        style: theme.textTheme.titleLarge,
                      ),
                      const SizedBox(height: 2),
                      Text(
                        '$version · ${l10n.appSubtitle}',
                        style: theme.textTheme.labelSmall,
                      ),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: 18),
            Text(l10n.aboutDescription, style: theme.textTheme.bodyMedium),
            const SizedBox(height: 18),
            Divider(color: insetFill(context)),
            const SizedBox(height: 10),
            Text(l10n.aboutBuiltWith, style: theme.textTheme.bodySmall),
            const SizedBox(height: 3),
            Text(l10n.aboutLicense, style: theme.textTheme.bodySmall),
            const SizedBox(height: 10),
            Text(
              '© 2026 jyahn',
              style: theme.textTheme.labelSmall,
            ),
            const SizedBox(height: 3),
            // Icons8's free tier is attribution-required, so the credit ships
            // with the app rather than living only in the README.
            Text(
              'App icon by Icons8 (icons8.com)',
              style: theme.textTheme.labelSmall,
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => launchUrl(
            Uri.parse(kRepositoryUrl),
            mode: LaunchMode.externalApplication,
          ),
          child: Text(l10n.aboutOpenRepo),
        ),
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
