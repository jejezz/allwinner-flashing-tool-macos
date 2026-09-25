import 'package:flutter/widgets.dart';

import '../l10n/app_localizations.dart';
import 'about_dialog.dart';

/// Allwinner Flasher's wording for the common About dialog. Opened by the ⓘ
/// button in the header and by the macOS app menu's "About" item.
Future<void> showFlasherAbout(BuildContext context) {
  final l10n = AppLocalizations.of(context);
  return showAppAboutDialog(
    context,
    tagline: l10n.aboutTagline,
    description: l10n.aboutDescription,
  );
}
