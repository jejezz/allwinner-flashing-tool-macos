// From jejezz/application-release-templates common/ @ conventions-v1.
//
// 업데이트 흐름을 앱에 연결한다 (conventions/updating.md):
//   시작 후 자동 확인(하루 1회) → 새 버전 알림 → 동의 → 내려받기 · 검증 → 설치.
// 앱은 이 클래스를 만들어 main 에서 `startAutomaticCheck` 를 부르고, 정보 창과
// macOS 앱 메뉴의 "업데이트 확인" 에 `checkManually` 를 연결하면 된다.
//
// 필요한 것:
//   - pubspec: app_updater, package_info_plus, shared_preferences, url_launcher
//   - lib/app_identity.dart 의 updateServerUrl · updateAppId
//   - lib/l10n/app_*.arb 에 update* 키

import 'dart:async';
import 'dart:io';
import 'dart:ui' show AppExitType;

import 'package:app_updater/app_updater.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart' show ServicesBinding;
import 'package:package_info_plus/package_info_plus.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:url_launcher/url_launcher.dart';

import '../app_identity.dart';
import '../l10n/app_localizations.dart';
import 'update_dialogs.dart';
import 'update_state_store.dart';

class UpdateService {
  UpdateService({
    required this.updater,
    required this.policy,
    String? os,
    Future<void> Function()? quitApp,
    Future<void> Function(Uri url)? openUrl,
    this.startupDelay = const Duration(seconds: 5),
  })  : os = os ?? _currentOs(),
        _quitApp = quitApp ?? _exitApp,
        _openUrl = openUrl ?? _launch;

  /// 데스크톱이 아니거나 서버 주소가 비어 있으면(`--dart-define=UPDATE_SERVER=`) null —
  /// 그 앱은 업데이트 확인을 하지 않는다 (스토어 배포 모바일 앱 등).
  static Future<UpdateService?> create() async {
    if (AppIdentity.updateServerUrl.isEmpty) return null;
    if (!(Platform.isMacOS || Platform.isWindows || Platform.isLinux)) return null;
    final info = await PackageInfo.fromPlatform();
    final prefs = await SharedPreferences.getInstance();
    return UpdateService(
      updater: AppUpdater(
        server: Uri.parse(AppIdentity.updateServerUrl),
        appId: AppIdentity.updateAppId,
        currentVersion: info.version,
        installer: platformInstaller(PlatformInfo.current()),
      ),
      policy: UpdatePolicy(PrefsUpdateStateStore(prefs)),
    );
  }

  final AppUpdater updater;
  final UpdatePolicy policy;

  /// `macos` · `windows` · `linux`. 설치 후 안내 문구를 고른다.
  final String os;

  /// 앱을 시작하고 첫 화면이 뜬 뒤 이만큼 기다렸다가 확인한다.
  final Duration startupDelay;

  final Future<void> Function() _quitApp;
  final Future<void> Function(Uri url) _openUrl;

  static String _currentOs() => Platform.isMacOS ? 'macos' : (Platform.isWindows ? 'windows' : 'linux');

  // 정식 종료: 앱의 WidgetsBindingObserver.didRequestAppExit 를 거친 뒤 끝난다. 종료 직전에 정리할 것(로그 flush,
  // 저장)이 있는 앱은 거기서 하면 된다. `exit(0)` 은 그것을 건너뛰므로 쓰지 않는다.
  static Future<void> _exitApp() async {
    await ServicesBinding.instance.exitApplication(AppExitType.cancelable);
  }

  static Future<void> _launch(Uri url) async {
    await launchUrl(url, mode: LaunchMode.externalApplication);
  }

  /// 앱 시작용. 기다린 뒤 하루 1회만 확인하고, 알릴 것이 없거나 실패하면 아무것도 띄우지 않는다.
  void startAutomaticCheck(GlobalKey<NavigatorState> navigatorKey) {
    unawaited(() async {
      try {
        await Future<void>.delayed(startupDelay);
        await updater.cleanUpOldDownloads();
        final result = await updater.checkAutomatically(policy);
        final context = navigatorKey.currentContext;
        if (result == null || context == null || !context.mounted) return;
        await _present(context, result, manual: false);
      } catch (e, stack) {
        // 업데이트 확인이 앱을 방해하면 안 된다.
        debugPrint('Automatic update check failed: $e\n$stack');
      }
    }());
  }

  /// "업데이트 확인" 단추. 하루 1회 제한과 건너뛴 버전을 무시하고, 결과를 항상 알려 준다.
  Future<void> checkManually(BuildContext context) async {
    unawaited(showDialog<void>(
      context: context,
      barrierDismissible: false,
      builder: (_) => const UpdateCheckingDialog(),
    ));
    final result = await updater.check();
    if (!context.mounted) return;
    Navigator.of(context, rootNavigator: true).pop(); // 확인 중 창
    if (result is! UpdateCheckFailed) await policy.markChecked();
    if (!context.mounted) return;
    await _present(context, result, manual: true);
  }

  Future<void> _present(BuildContext context, UpdateCheckResult result, {required bool manual}) async {
    final l10n = AppLocalizations.of(context);
    switch (result) {
      case UpToDate():
        if (!manual) return;
        final info = await PackageInfo.fromPlatform();
        if (!context.mounted) return;
        await showDialog<void>(
          context: context,
          builder: (_) => UpdateMessageDialog(
            title: l10n.updateUpToDateTitle,
            message: l10n.updateUpToDateBody(info.version),
          ),
        );
      case UpdateCheckFailed(:final error):
        if (!manual) return;
        await showDialog<void>(
          context: context,
          builder: (_) => UpdateMessageDialog(
            title: l10n.updateCheckFailedTitle,
            message: updateErrorMessage(l10n, error.kind),
          ),
        );
      case UpdateUnavailable(:final latestVersion, :final releaseUrl):
        await _showUnavailable(context, latestVersion, releaseUrl);
      case UpdateAvailable(:final info):
        final choice = await showDialog<UpdateChoice>(
          context: context,
          barrierDismissible: false,
          builder: (_) => UpdateAvailableDialog(info: info),
        );
        switch (choice) {
          case UpdateChoice.now:
            if (context.mounted) await _download(context, info);
          case UpdateChoice.skip:
            await policy.skip(info.latestVersion);
          case UpdateChoice.later || null:
            break;
        }
    }
  }

  /// 새 버전은 있지만 앱이 자동 설치할 수 없는 경우(체크섬 없음, 이 OS 용 파일 없음).
  Future<void> _showUnavailable(BuildContext context, String latest, Uri? releaseUrl) async {
    final l10n = AppLocalizations.of(context);
    final choice = await showDialog<UpdateChoice>(
      context: context,
      builder: (dialogContext) => UpdateMessageDialog(
        title: l10n.updateUnavailableTitle,
        message: l10n.updateUnavailableBody(latest),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(UpdateChoice.skip),
            child: Text(l10n.updateSkipVersion),
          ),
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(UpdateChoice.later),
            child: Text(l10n.commonClose),
          ),
          if (releaseUrl != null)
            FilledButton(
              onPressed: () => Navigator.of(dialogContext).pop(UpdateChoice.now),
              child: Text(l10n.updateOpenReleasePage),
            ),
        ],
      ),
    );
    switch (choice) {
      case UpdateChoice.now:
        if (releaseUrl != null) await _openUrl(releaseUrl);
      case UpdateChoice.skip:
        await policy.skip(latest);
      case UpdateChoice.later || null:
        break;
    }
  }

  Future<void> _download(BuildContext context, UpdateInfo info) async {
    final downloaded = await showDialog<DownloadedUpdate>(
      context: context,
      barrierDismissible: false,
      builder: (_) => UpdateDownloadDialog(updater: updater, info: info),
    );
    if (downloaded == null || !context.mounted) return;
    final l10n = AppLocalizations.of(context);

    final InstallOutcome outcome;
    try {
      outcome = await updater.install(downloaded);
    } on UpdateException catch (e) {
      await updater.discard(downloaded);
      if (!context.mounted) return;
      await showDialog<void>(
        context: context,
        builder: (_) => UpdateMessageDialog(
          title: l10n.updateFailedTitle,
          message: updateErrorMessage(l10n, e.kind),
        ),
      );
      return;
    }
    if (!context.mounted) return;

    switch (outcome) {
      case InstallOutcome.openedForUser:
        // macOS: DMG 창이 열렸다. 앱은 계속 실행된다.
        await showDialog<void>(
          context: context,
          builder: (_) => UpdateMessageDialog(
            title: l10n.updateMacosOpenedTitle,
            message: l10n.updateMacosOpenedBody(AppIdentity.displayName),
          ),
        );
      case InstallOutcome.installerStarted:
        // Windows · Linux: 설치하려면 이 앱이 종료돼야 한다. 사용자가 누를 때 종료한다.
        final linux = os == 'linux';
        final quit = await showDialog<bool>(
          context: context,
          barrierDismissible: false,
          builder: (dialogContext) => UpdateMessageDialog(
            title: linux ? l10n.updateLinuxInstallTitle : l10n.updateWindowsInstallTitle,
            message: linux
                ? l10n.updateLinuxInstallBody(AppIdentity.displayName)
                : l10n.updateWindowsInstallBody(AppIdentity.displayName),
            actions: [
              if (!linux)
                TextButton(
                  onPressed: () => Navigator.of(dialogContext).pop(false),
                  child: Text(l10n.commonClose),
                ),
              FilledButton(
                onPressed: () => Navigator.of(dialogContext).pop(true),
                child: Text(linux ? l10n.updateQuitAndInstall : l10n.updateQuitApp(AppIdentity.displayName)),
              ),
            ],
          ),
        );
        if (quit ?? false) await _quitApp();
    }
  }
}
