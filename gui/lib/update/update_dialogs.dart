// From jejezz/application-release-templates common/ @ conventions-v1.
//
// 업데이트 흐름의 화면들 (conventions/updating.md). 로직은 app_updater 패키지에,
// 문구는 앱의 ARB(update* 키)에 있고, 여기는 보여 주기만 한다.
//
// 필요한 것:
//   - pubspec: app_updater, flutter_localizations
//   - lib/l10n/app_*.arb 에 common/l10n/common_*.arb 의 update* 키

import 'package:app_updater/app_updater.dart';
import 'package:flutter/material.dart';

import '../app_identity.dart';
import '../l10n/app_localizations.dart';

enum UpdateChoice { now, later, skip }

/// 새 버전 알림: [나중에] [이 버전 건너뛰기] [지금 업데이트].
class UpdateAvailableDialog extends StatelessWidget {
  const UpdateAvailableDialog({super.key, required this.info});

  final UpdateInfo info;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final theme = Theme.of(context);
    final notes = info.notes.trim();
    return AlertDialog(
      title: Text(l10n.updateAvailableTitle),
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 440),
        child: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(l10n.updateAvailableBody(AppIdentity.displayName, info.currentVersion, info.latestVersion)),
              if (notes.isNotEmpty) ...[
                const SizedBox(height: 16),
                Text(l10n.updateReleaseNotes, style: theme.textTheme.titleSmall),
                const SizedBox(height: 4),
                ConstrainedBox(
                  constraints: const BoxConstraints(maxHeight: 160),
                  child: SingleChildScrollView(
                    child: SelectableText(notes, style: theme.textTheme.bodySmall),
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(UpdateChoice.skip),
          child: Text(l10n.updateSkipVersion),
        ),
        TextButton(
          onPressed: () => Navigator.of(context).pop(UpdateChoice.later),
          child: Text(l10n.updateLater),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop(UpdateChoice.now),
          child: Text(l10n.updateNow),
        ),
      ],
    );
  }
}

/// 내려받는 동안의 진행률. 끝나면 [DownloadedUpdate] 로, 취소하면 null 로 닫힌다.
/// 실패하면 이유를 보여 주고 닫기를 누르면 null 로 닫힌다. 동의를 받은 뒤에만 띄운다.
class UpdateDownloadDialog extends StatefulWidget {
  const UpdateDownloadDialog({super.key, required this.updater, required this.info});

  final AppUpdater updater;
  final UpdateInfo info;

  @override
  State<UpdateDownloadDialog> createState() => _UpdateDownloadDialogState();
}

class _UpdateDownloadDialogState extends State<UpdateDownloadDialog> {
  final _cancel = CancelToken();
  int _received = 0;
  UpdateException? _error;

  @override
  void initState() {
    super.initState();
    _run();
  }

  Future<void> _run() async {
    try {
      final done = await widget.updater.download(
        widget.info,
        cancel: _cancel,
        onProgress: (received, total) {
          if (mounted) setState(() => _received = received);
        },
      );
      if (!mounted) {
        await widget.updater.discard(done);
        return;
      }
      Navigator.of(context).pop(done);
    } on UpdateException catch (e) {
      if (!mounted) return;
      if (e.kind == UpdateErrorKind.cancelled) {
        Navigator.of(context).pop();
      } else {
        setState(() => _error = e);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    final error = _error;
    if (error != null) {
      return UpdateMessageDialog(
        title: l10n.updateFailedTitle,
        message: updateErrorMessage(l10n, error.kind),
      );
    }
    final total = widget.info.asset.size;
    final verifying = _received >= total;
    return PopScope(
      canPop: false,
      child: AlertDialog(
        title: Text(verifying ? l10n.updateVerifying : l10n.updateDownloading),
        content: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 440),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              LinearProgressIndicator(value: verifying ? null : (total > 0 ? _received / total : null)),
              const SizedBox(height: 8),
              Text(l10n.updateDownloadProgress(formatBytes(_received), formatBytes(total))),
            ],
          ),
        ),
        actions: [
          TextButton(onPressed: _cancel.cancel, child: Text(l10n.updateCancel)),
        ],
      ),
    );
  }
}

/// 제목 + 문구 + (선택) 눌렀을 때 닫히는 버튼들. 결과 안내용 공통 대화상자.
class UpdateMessageDialog extends StatelessWidget {
  const UpdateMessageDialog({
    super.key,
    required this.title,
    required this.message,
    this.actions,
  });

  final String title;
  final String message;

  /// 없으면 [닫기] 하나.
  final List<Widget>? actions;

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return AlertDialog(
      title: Text(title),
      content: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 440),
        child: SingleChildScrollView(child: Text(message)),
      ),
      actions: actions ??
          [
            FilledButton(
              onPressed: () => Navigator.of(context).pop(),
              child: Text(l10n.commonClose),
            ),
          ],
    );
  }
}

/// "확인하는 중…" — 수동 확인이 서버를 기다리는 동안.
class UpdateCheckingDialog extends StatelessWidget {
  const UpdateCheckingDialog({super.key});

  @override
  Widget build(BuildContext context) {
    final l10n = AppLocalizations.of(context);
    return PopScope(
      canPop: false,
      child: AlertDialog(
        content: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            const SizedBox.square(dimension: 20, child: CircularProgressIndicator(strokeWidth: 2.5)),
            const SizedBox(width: 16),
            Flexible(child: Text(l10n.updateChecking)),
          ],
        ),
      ),
    );
  }
}

/// 실패 종류별 안내 문구. 서버 주소나 내부 오류 문자열은 사용자에게 보이지 않는다.
String updateErrorMessage(AppLocalizations l10n, UpdateErrorKind kind) => switch (kind) {
      UpdateErrorKind.network => l10n.updateErrorNetwork,
      UpdateErrorKind.checksumMismatch ||
      UpdateErrorKind.sizeMismatch =>
        l10n.updateErrorChecksum,
      UpdateErrorKind.installFailed => l10n.updateErrorInstall,
      _ => l10n.updateErrorGeneric,
    };

/// 12.3 MB 처럼.
String formatBytes(int bytes) {
  const units = ['B', 'KB', 'MB', 'GB'];
  var value = bytes.toDouble();
  var i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i++;
  }
  return '${value >= 10 || i == 0 ? value.round() : value.toStringAsFixed(1)} ${units[i]}';
}
