// From jejezz/application-release-templates common/ @ conventions-v1.
//
// conventions/about-dialog.md 형식의 정보 창. dove-zip(← daylight-commander)
// 구현을 일반화했다. 앱마다 다른 문구(소개, 설명, 기능 목록)만 인자로 받고,
// 이름·저작권·라이선스·아이콘은 AppIdentity에서 읽는다.
//
// 필요한 것:
//   - pubspec: package_info_plus, url_launcher, flutter_localizations
//   - lib/l10n/app_*.arb에 common/l10n/common_*.arb의 키
//   - assets/icon/app_icon.png (pubspec의 flutter.assets에 등록)

import 'package:flutter/material.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:url_launcher/url_launcher.dart';

import '../app_identity.dart';
import '../l10n/app_localizations.dart';

/// 정보 창을 연다. 앱 바의 정보 버튼, macOS 앱 메뉴의 "About", 트레이
/// 메뉴가 모두 이 함수를 부른다 (about-dialog.md §1).
Future<void> showAppAboutDialog(
  BuildContext context, {
  required String tagline,
  required String description,
  List<String> features = const [],
  List<String> assetCredits = const [],
}) async {
  // 버전은 실제 빌드에서 읽는다 — 코드에 적어 두면 pubspec과 어긋난다.
  final info = await PackageInfo.fromPlatform();
  if (!context.mounted) return;

  await showDialog<void>(
    context: context,
    builder: (context) => AppAboutDialog(
      version: info.version,
      buildNumber: info.buildNumber,
      tagline: tagline,
      description: description,
      features: features,
      assetCredits: assetCredits,
    ),
  );
}

class AppAboutDialog extends StatelessWidget {
  const AppAboutDialog({
    super.key,
    required this.version,
    required this.buildNumber,
    required this.tagline,
    required this.description,
    this.features = const [],
    this.assetCredits = const [],
  });

  final String version;
  final String buildNumber;
  final String tagline;
  final String description;
  final List<String> features;
  final List<String> assetCredits;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final text = theme.textTheme;
    final muted = text.bodySmall?.copyWith(color: theme.colorScheme.onSurfaceVariant);

    return AlertDialog(
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 440),
        child: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(
                children: [
                  ClipRRect(
                    borderRadius: BorderRadius.circular(10),
                    child: Image.asset(AppIdentity.iconAsset, width: 48, height: 48),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(AppIdentity.displayName, style: text.titleLarge),
                        // 문의할 때 그대로 붙여넣을 수 있게 선택 가능하게 둔다.
                        SelectableText(l10n.aboutVersion(version, buildNumber), style: muted),
                      ],
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 16),
              Text(tagline, style: text.titleSmall),
              const SizedBox(height: 4),
              Text(description, style: text.bodyMedium),
              if (features.isNotEmpty) ...[
                const SizedBox(height: 12),
                for (final feature in features)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 4),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        const Text('•  '),
                        Expanded(child: Text(feature, style: text.bodyMedium)),
                      ],
                    ),
                  ),
              ],
              const SizedBox(height: 12),
              const Divider(),
              const SizedBox(height: 8),
              Text(AppIdentity.copyright, style: muted),
              Text(AppIdentity.licenseName, style: muted),
              for (final credit in assetCredits) Text(credit, style: muted),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => showLicensePage(
            context: context,
            applicationName: AppIdentity.displayName,
            applicationVersion: l10n.aboutVersion(version, buildNumber),
            applicationIcon: Padding(
              padding: const EdgeInsets.all(8),
              child: Image.asset(AppIdentity.iconAsset, width: 48, height: 48),
            ),
            applicationLegalese: AppIdentity.copyright,
          ),
          child: Text(l10n.aboutOpenSourceLicenses),
        ),
        TextButton(
          onPressed: () => launchUrl(Uri.parse(AppIdentity.repositoryUrl)),
          child: Text(l10n.aboutRepository),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(l10n.commonClose),
        ),
      ],
    );
  }
}
