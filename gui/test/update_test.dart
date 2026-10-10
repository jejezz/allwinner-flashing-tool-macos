// From jejezz/application-release-templates common/ @ conventions-v1.
// conventions/updating.md — 업데이트 흐름의 화면: 알림 · 건너뛰기 · 내려받기 · 오류 · OS 별 설치 안내.
// aw_flasher를 앱의 pubspec name으로 바꾼다. 실제 네트워크·파일·프로세스는 쓰지 않는다 (가짜 AppUpdater).

import 'dart:async';
import 'dart:io';

import 'package:app_updater/app_updater.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:aw_flasher/about/about_dialog.dart';
import 'package:aw_flasher/app_identity.dart';
import 'package:aw_flasher/l10n/app_localizations.dart';
import 'package:aw_flasher/update/update_dialogs.dart';
import 'package:aw_flasher/update/update_scope.dart';
import 'package:aw_flasher/update/update_service.dart';

UpdateInfo _info({String notes = 'Fixed things.'}) => UpdateInfo(
      appId: 'sample-flutter',
      currentVersion: '1.0.0',
      latestVersion: '1.1.0',
      tag: 'v1.1.0',
      notes: notes,
      releaseUrl: Uri.parse('https://github.com/jejezz/sample-flutter/releases/tag/v1.1.0'),
      asset: UpdateAsset(
        name: 'Sample-1.1.0-macos-universal.dmg',
        url: Uri.parse('https://github.com/jejezz/sample-flutter/releases/download/v1.1.0/x.dmg'),
        size: 2 * 1024 * 1024,
        sha256: 'a' * 64,
      ),
    );

class _FakeUpdater implements AppUpdater {
  UpdateCheckResult checkResult = const UpToDate();
  UpdateCheckResult? autoResult;

  /// download() waits for this before finishing, so a test can look at the progress screen.
  Completer<void>? gate;
  UpdateException? downloadError;
  UpdateException? installError;
  InstallOutcome outcome = InstallOutcome.openedForUser;

  int downloads = 0;
  int installs = 0;
  int discards = 0;

  @override
  String get appId => 'sample-flutter';
  @override
  UpdateInstaller? get installer => null;
  @override
  List<String> get downloadUrlPrefixes => const [];
  @override
  Duration get requestTimeout => const Duration(seconds: 1);

  @override
  Future<UpdateCheckResult> check() async => checkResult;

  @override
  Future<UpdateCheckResult?> checkAutomatically(UpdatePolicy policy) async => autoResult;

  @override
  Future<DownloadedUpdate> download(UpdateInfo info, {ProgressCallback? onProgress, CancelToken? cancel}) async {
    downloads++;
    onProgress?.call(info.asset.size ~/ 2, info.asset.size);
    if (gate != null) await gate!.future;
    if (cancel?.isCancelled ?? false) throw UpdateException(UpdateErrorKind.cancelled, 'cancelled');
    if (downloadError != null) throw downloadError!;
    onProgress?.call(info.asset.size, info.asset.size);
    return DownloadedUpdate(info: info, file: File('/tmp/app_updater_fake/x.dmg'));
  }

  @override
  Future<InstallOutcome> install(DownloadedUpdate update) async {
    installs++;
    if (installError != null) throw installError!;
    return outcome;
  }

  @override
  Future<void> discard(DownloadedUpdate update) async => discards++;

  @override
  Future<void> cleanUpOldDownloads({Duration olderThan = const Duration(days: 1)}) async {}

  @override
  void close() {}
}

class _Harness {
  _Harness({String os = 'macos', bool Function()? isBusy}) {
    service = UpdateService(
      updater: updater,
      policy: policy,
      os: os,
      isBusy: isBusy,
      idlePollInterval: const Duration(milliseconds: 20),
      startupDelay: Duration.zero,
      quitApp: () async => quits++,
      openUrl: (url) async => opened.add(url),
    );
  }

  final updater = _FakeUpdater();
  final policy = UpdatePolicy(MemoryUpdateStateStore());
  late final UpdateService service;
  final navigatorKey = GlobalKey<NavigatorState>();
  final opened = <Uri>[];
  int quits = 0;

  Widget app({Locale locale = const Locale('ko')}) => MaterialApp(
        navigatorKey: navigatorKey,
        locale: locale,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        supportedLocales: AppLocalizations.supportedLocales,
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () => service.checkManually(context),
              child: const Text('check'),
            ),
          ),
        ),
      );
}

Future<void> _check(WidgetTester tester, _Harness h, {Locale locale = const Locale('ko')}) async {
  await tester.pumpWidget(h.app(locale: locale));
  await tester.tap(find.text('check'));
  await tester.pump(); // 확인 중 창
  await tester.pump(); // 결과
  await tester.pump(const Duration(milliseconds: 300));
}

void main() {
  setUp(() => PackageInfo.setMockInitialValues(
        appName: 'Sample', packageName: 'art.zoomon.sample', version: '1.0.0', buildNumber: '3', buildSignature: ''));

  group('manual check', () {
    testWidgets('up to date says so, with the running version', (tester) async {
      final h = _Harness();
      await _check(tester, h);
      expect(find.text('최신 버전입니다'), findsOneWidget);
      expect(find.text('1.0.0 버전을 사용 중입니다.'), findsOneWidget);
      expect(find.text('업데이트를 확인하는 중…'), findsNothing); // 확인 중 창은 닫혔다
    });

    testWidgets('a failed check explains in plain words, never the raw error', (tester) async {
      final h = _Harness();
      h.updater.checkResult = UpdateCheckFailed(UpdateException(UpdateErrorKind.network, 'SocketException 10.0.0.1:443'));
      await _check(tester, h);
      expect(find.text('업데이트를 확인하지 못했습니다'), findsOneWidget);
      expect(find.textContaining('인터넷 연결'), findsOneWidget);
      expect(find.textContaining('SocketException'), findsNothing);
    });

    testWidgets('English', (tester) async {
      final h = _Harness();
      await _check(tester, h, locale: const Locale('en'));
      expect(find.text("You're up to date"), findsOneWidget);
    });
  });

  group('new version', () {
    testWidgets('shows versions and notes; Later changes nothing', (tester) async {
      final h = _Harness()..updater.checkResult = UpdateAvailable(_info());
      await _check(tester, h);
      expect(find.text('새 버전이 있습니다'), findsOneWidget);
      expect(find.textContaining('1.1.0 버전이 나왔습니다. (현재 1.0.0)'), findsOneWidget);
      expect(find.text('Fixed things.'), findsOneWidget);
      await tester.tap(find.text('나중에'));
      await tester.pumpAndSettle();
      expect(h.updater.downloads, 0);
      expect(await h.policy.isSkipped('1.1.0'), isFalse);
    });

    testWidgets('Skip remembers this version', (tester) async {
      final h = _Harness()..updater.checkResult = UpdateAvailable(_info());
      await _check(tester, h);
      await tester.tap(find.text('이 버전 건너뛰기'));
      await tester.pumpAndSettle();
      expect(await h.policy.isSkipped('1.1.0'), isTrue);
      expect(h.updater.downloads, 0);
    });

    testWidgets('nothing is downloaded before the person agrees', (tester) async {
      final h = _Harness()..updater.checkResult = UpdateAvailable(_info());
      await _check(tester, h);
      expect(h.updater.downloads, 0);
      expect(h.updater.installs, 0);
    });
  });

  group('update now', () {
    Future<void> agree(WidgetTester tester) async {
      await tester.tap(find.text('지금 업데이트'));
      await tester.pump();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
    }

    testWidgets('shows progress while downloading, then macOS opens the DMG', (tester) async {
      final h = _Harness()
        ..updater.checkResult = UpdateAvailable(_info())
        ..updater.gate = Completer<void>();
      await _check(tester, h);
      await agree(tester);
      expect(find.text('내려받는 중…'), findsOneWidget);
      expect(find.text('1.0 MB / 2.0 MB'), findsOneWidget);
      expect(find.text('취소'), findsOneWidget);

      h.updater.gate!.complete();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      expect(h.updater.installs, 1);
      expect(find.text('설치 창이 열렸습니다'), findsOneWidget);
      expect(h.quits, 0); // macOS 는 앱을 종료하지 않는다
    });

    testWidgets('Windows: the app quits only when the person says so', (tester) async {
      final h = _Harness(os: 'windows')
        ..updater.checkResult = UpdateAvailable(_info())
        ..updater.outcome = InstallOutcome.installerStarted;
      await _check(tester, h);
      await agree(tester);
      expect(find.text('설치 프로그램을 열었습니다'), findsOneWidget);
      await tester.tap(find.text('닫기'));
      await tester.pumpAndSettle();
      expect(h.quits, 0);

      // 다시 한 번 — 이번에는 종료한다.
      await tester.tap(find.text('check'));
      await tester.pump();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      await agree(tester);
      await tester.tap(find.text('${AppIdentity.displayName} 종료'));
      await tester.pumpAndSettle();
      expect(h.quits, 1);
    });

    testWidgets('Linux: one button, quits to install', (tester) async {
      final h = _Harness(os: 'linux')
        ..updater.checkResult = UpdateAvailable(_info())
        ..updater.outcome = InstallOutcome.installerStarted;
      await _check(tester, h);
      await agree(tester);
      expect(find.text('설치를 준비했습니다'), findsOneWidget);
      expect(find.text('닫기'), findsNothing);
      await tester.tap(find.text('종료하고 설치'));
      await tester.pumpAndSettle();
      expect(h.quits, 1);
    });

    testWidgets('a file that fails verification is reported and never installed', (tester) async {
      final h = _Harness()
        ..updater.checkResult = UpdateAvailable(_info())
        ..updater.downloadError = UpdateException(UpdateErrorKind.checksumMismatch, 'sha256 differs');
      await _check(tester, h);
      await agree(tester);
      expect(find.text('업데이트하지 못했습니다'), findsOneWidget);
      expect(find.textContaining('설치하지 않았습니다'), findsOneWidget);
      expect(find.textContaining('sha256'), findsNothing);
      expect(h.updater.installs, 0);
      await tester.tap(find.text('닫기'));
      await tester.pumpAndSettle();
      expect(find.byType(AlertDialog), findsNothing);
    });

    testWidgets('Cancel stops quietly', (tester) async {
      final h = _Harness()
        ..updater.checkResult = UpdateAvailable(_info())
        ..updater.gate = Completer<void>();
      await _check(tester, h);
      await agree(tester);
      await tester.tap(find.text('취소'));
      h.updater.gate!.complete();
      await tester.pumpAndSettle();
      expect(find.byType(AlertDialog), findsNothing);
      expect(h.updater.installs, 0);
    });

    testWidgets('an installer that cannot start cleans up and says so', (tester) async {
      final h = _Harness()
        ..updater.checkResult = UpdateAvailable(_info())
        ..updater.installError = UpdateException(UpdateErrorKind.installFailed, 'open exited with 1');
      await _check(tester, h);
      await agree(tester);
      expect(find.text('설치를 시작하지 못했습니다.'), findsOneWidget);
      expect(h.updater.discards, 1);
    });
  });

  group('busy (the app is doing something that must not be interrupted)', () {
    testWidgets('the automatic prompt waits until the app is idle', (tester) async {
      var busy = true;
      final h = _Harness(isBusy: () => busy)..updater.autoResult = UpdateAvailable(_info());
      await tester.pumpWidget(h.app());
      h.service.startAutomaticCheck(h.navigatorKey);
      await tester.pump(const Duration(milliseconds: 200));
      expect(find.text('새 버전이 있습니다'), findsNothing);

      busy = false;
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.text('새 버전이 있습니다'), findsOneWidget);
    });

    testWidgets('"Update now" is refused while busy and nothing is downloaded', (tester) async {
      var busy = false;
      final h = _Harness(isBusy: () => busy)..updater.checkResult = UpdateAvailable(_info());
      await _check(tester, h);
      busy = true; // 알림이 떠 있는 사이에 일이 시작됐다
      await tester.tap(find.text('지금 업데이트'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.text('지금은 업데이트할 수 없습니다'), findsOneWidget);
      expect(h.updater.downloads, 0);
      expect(h.updater.installs, 0);
    });

    testWidgets('work that starts during the download cancels the install and removes the file', (tester) async {
      var busy = false;
      final h = _Harness(isBusy: () => busy)
        ..updater.checkResult = UpdateAvailable(_info())
        ..updater.gate = Completer<void>();
      await _check(tester, h);
      await tester.tap(find.text('지금 업데이트'));
      await tester.pump();
      await tester.pump();
      busy = true;
      h.updater.gate!.complete();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.text('지금은 업데이트할 수 없습니다'), findsOneWidget);
      expect(h.updater.installs, 0);
      expect(h.updater.discards, 1);
      expect(h.quits, 0);
    });

    testWidgets('a manual check still reports while busy (only installing is blocked)', (tester) async {
      final h = _Harness(isBusy: () => true);
      await _check(tester, h);
      expect(find.text('최신 버전입니다'), findsOneWidget);
    });
  });

  group('cannot install automatically', () {
    testWidgets('opens the release page', (tester) async {
      final h = _Harness();
      h.updater.checkResult = UpdateUnavailable(
        reason: UnavailableReason.noChecksum,
        latestVersion: '1.1.0',
        releaseUrl: Uri.parse('https://github.com/jejezz/sample-flutter/releases/tag/v1.1.0'),
      );
      await _check(tester, h);
      expect(find.text('직접 설치해 주세요'), findsOneWidget);
      await tester.tap(find.text('릴리스 페이지 열기'));
      await tester.pumpAndSettle();
      expect(h.opened.single.toString(), endsWith('/v1.1.0'));
      expect(h.updater.downloads, 0);
    });

    testWidgets('can be skipped too', (tester) async {
      final h = _Harness();
      h.updater.checkResult = const UpdateUnavailable(reason: UnavailableReason.noAssetForPlatform, latestVersion: '1.1.0');
      await _check(tester, h);
      expect(find.text('릴리스 페이지 열기'), findsNothing); // 주소가 없으면 단추도 없다
      await tester.tap(find.text('이 버전 건너뛰기'));
      await tester.pumpAndSettle();
      expect(await h.policy.isSkipped('1.1.0'), isTrue);
    });
  });

  group('automatic check', () {
    testWidgets('shows the alert when there is something new', (tester) async {
      final h = _Harness()..updater.autoResult = UpdateAvailable(_info());
      await tester.pumpWidget(h.app());
      h.service.startAutomaticCheck(h.navigatorKey);
      await tester.pump(const Duration(milliseconds: 10));
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.text('새 버전이 있습니다'), findsOneWidget);
    });

    testWidgets('stays silent when nothing is due, up to date, or failed', (tester) async {
      final h = _Harness(); // autoResult == null
      await tester.pumpWidget(h.app());
      h.service.startAutomaticCheck(h.navigatorKey);
      await tester.pump(const Duration(milliseconds: 10));
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.byType(AlertDialog), findsNothing);

      h.updater.autoResult = const UpToDate();
      h.service.startAutomaticCheck(h.navigatorKey);
      await tester.pump(const Duration(milliseconds: 10));
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.byType(AlertDialog), findsNothing);
    });
  });

  group('about dialog', () {
    Widget about({VoidCallback? onCheck}) => MaterialApp(
          locale: const Locale('ko'),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          supportedLocales: AppLocalizations.supportedLocales,
          home: Scaffold(
            body: AppAboutDialog(version: '1.0.0', buildNumber: '3', tagline: 't', description: 'd', onCheckForUpdates: onCheck),
          ),
        );

    testWidgets('has the button only when updates are wired', (tester) async {
      await tester.pumpWidget(about());
      expect(find.text('업데이트 확인'), findsNothing);

      var pressed = 0;
      await tester.pumpWidget(about(onCheck: () => pressed++));
      await tester.tap(find.text('업데이트 확인'));
      expect(pressed, 1);
    });
  });

  group('UpdateScope', () {
    Widget host(UpdateService? service) => UpdateScope(
          service: service,
          child: MaterialApp(
            locale: const Locale('ko'),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            supportedLocales: AppLocalizations.supportedLocales,
            home: Scaffold(
              body: Builder(
                builder: (context) => TextButton(
                  onPressed: () => showAppAboutDialog(context, tagline: 't', description: 'd'),
                  child: const Text('open about'),
                ),
              ),
            ),
          ),
        );

    testWidgets('the About dialog gets its own "check for updates" button, wherever it is opened', (tester) async {
      final h = _Harness();
      await tester.pumpWidget(host(h.service));
      await tester.tap(find.text('open about'));
      await tester.pumpAndSettle();
      expect(find.text('업데이트 확인'), findsOneWidget);

      await tester.tap(find.text('업데이트 확인'));
      await tester.pump();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      expect(find.text('최신 버전입니다'), findsOneWidget); // 가짜 업데이터의 기본 결과
    });

    testWidgets('no scope (or no service) → no button', (tester) async {
      await tester.pumpWidget(host(null));
      await tester.tap(find.text('open about'));
      await tester.pumpAndSettle();
      expect(find.text('업데이트 확인'), findsNothing);
    });
  });

  test('formatBytes', () {
    expect(formatBytes(0), '0 B');
    expect(formatBytes(1536), '1.5 KB');
    expect(formatBytes(27019979), '26 MB');
    expect(formatBytes(2 * 1024 * 1024), '2.0 MB');
  });
}
