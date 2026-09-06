import 'package:flutter/material.dart';

/// Palette and theme, carried over from the Saturn client so the two tools
/// read as one family. Kept as a copy rather than a shared package: this repo
/// ships on its own, and the flashing tool needs a few semantics (a danger
/// accent for destructive work) that the client has no use for.
class AppColors {
  const AppColors._();

  // Dark surfaces
  static const bg = Color(0xFF0A0E14);
  static const bgAlt = Color(0xFF0E141C);
  static const surface = Color(0xFF151D27);
  static const surfaceHi = Color(0xFF1D2733);
  static const stroke = Color(0x1AFFFFFF);
  static const strokeStrong = Color(0x33FFFFFF);

  // Light surfaces
  static const bgLight = Color(0xFFF4F6FA);
  static const surfaceLight = Color(0xFFFFFFFF);
  static const strokeLight = Color(0x14000000);

  // Brand
  static const primary = Color(0xFF4C9DFF);
  static const primaryDeep = Color(0xFF2C6BE0);
  static const accent = Color(0xFF7C5CFF);

  // Semantics
  static const success = Color(0xFF34D399);
  static const warning = Color(0xFFFFB020);
  static const danger = Color(0xFFFF5A5F);
  static const idle = Color(0xFF64748B);

  static const textHi = Color(0xFFF1F5F9);
  static const textMid = Color(0xFFA9B4C4);
  static const textLow = Color(0xFF6B7787);

  /// Text colour that stays legible on either theme.
  static Color hi(BuildContext c) => Theme.of(c).colorScheme.onSurface;

  static Color mid(BuildContext c) => Theme.of(c).brightness == Brightness.dark
      ? textMid
      : const Color(0xFF5B6676);
}

class AppRadius {
  const AppRadius._();
  static const card = 24.0;
  static const tile = 20.0;
  static const chip = 999.0;
}

/// Monospace stack for anything the user compares character by character —
/// sector offsets, partition names, log lines. SeoulNamsan is proportional and
/// misaligns columns of hex.
const kMonoFamily = 'Menlo';

class AppTheme {
  const AppTheme._();

  static const _fontFamily = 'SeoulNamsan';

  static ThemeData dark() {
    const scheme = ColorScheme.dark(
      primary: AppColors.primary,
      onPrimary: Colors.white,
      secondary: AppColors.accent,
      onSecondary: Colors.white,
      surface: AppColors.surface,
      onSurface: AppColors.textHi,
      error: AppColors.danger,
      onError: Colors.white,
    );
    return _base(scheme, AppColors.bg, AppColors.textHi, AppColors.textMid);
  }

  static ThemeData light() {
    const scheme = ColorScheme.light(
      primary: AppColors.primaryDeep,
      onPrimary: Colors.white,
      secondary: AppColors.accent,
      onSecondary: Colors.white,
      surface: AppColors.surfaceLight,
      onSurface: Color(0xFF101828),
      error: AppColors.danger,
      onError: Colors.white,
    );
    return _base(scheme, AppColors.bgLight, const Color(0xFF101828),
        const Color(0xFF5B6676));
  }

  static ThemeData _base(
    ColorScheme scheme,
    Color background,
    Color textHi,
    Color textMid,
  ) {
    final isDark = scheme.brightness == Brightness.dark;
    return ThemeData(
      useMaterial3: true,
      colorScheme: scheme,
      fontFamily: _fontFamily,
      scaffoldBackgroundColor: background,
      canvasColor: background,
      splashFactory: InkSparkle.splashFactory,
      dividerTheme: DividerThemeData(
        color: isDark ? AppColors.stroke : AppColors.strokeLight,
        thickness: 1,
        space: 1,
      ),
      textTheme: TextTheme(
        displaySmall:
            TextStyle(fontSize: 34, fontWeight: FontWeight.w800, color: textHi),
        headlineMedium:
            TextStyle(fontSize: 26, fontWeight: FontWeight.w800, color: textHi),
        titleLarge:
            TextStyle(fontSize: 20, fontWeight: FontWeight.w700, color: textHi),
        titleMedium:
            TextStyle(fontSize: 16, fontWeight: FontWeight.w700, color: textHi),
        bodyLarge:
            TextStyle(fontSize: 15, fontWeight: FontWeight.w400, color: textHi),
        bodyMedium: TextStyle(
            fontSize: 13.5, fontWeight: FontWeight.w400, color: textMid),
        labelLarge:
            TextStyle(fontSize: 13, fontWeight: FontWeight.w700, color: textHi),
        labelSmall: TextStyle(
            fontSize: 11.5, fontWeight: FontWeight.w400, color: textMid),
      ),
      switchTheme: SwitchThemeData(
        thumbColor: WidgetStateProperty.resolveWith(
          (states) => states.contains(WidgetState.selected)
              ? Colors.white
              : const Color(0xFF94A3B8),
        ),
      ),
      progressIndicatorTheme: ProgressIndicatorThemeData(
        linearTrackColor:
            isDark ? Colors.black.withValues(alpha: 0.35) : const Color(0xFFE3E8EF),
        linearMinHeight: 8,
      ),
      dialogTheme: DialogThemeData(
        backgroundColor: isDark ? AppColors.surfaceHi : AppColors.surfaceLight,
        shape: const RoundedRectangleBorder(
          borderRadius: BorderRadius.all(Radius.circular(AppRadius.card)),
        ),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          minimumSize: const Size.fromHeight(54),
          shape: const RoundedRectangleBorder(
            borderRadius: BorderRadius.all(Radius.circular(18)),
          ),
          textStyle: const TextStyle(
            fontFamily: _fontFamily,
            fontSize: 16,
            fontWeight: FontWeight.w700,
          ),
        ),
      ),
      textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(
          textStyle: const TextStyle(
            fontFamily: _fontFamily,
            fontSize: 14,
            fontWeight: FontWeight.w700,
          ),
        ),
      ),
    );
  }
}

/// Subtle inset fill for rows sitting *inside* a card, where the divider
/// colour is far too light to read in dark mode.
Color insetFill(BuildContext context) =>
    Theme.of(context).brightness == Brightness.dark
        ? Colors.black.withValues(alpha: 0.28)
        : const Color(0xFF101828).withValues(alpha: 0.05);

/// Background wash behind the whole window.
class AuroraBackground extends StatelessWidget {
  const AuroraBackground({super.key, required this.child, this.tint});

  final Widget child;

  /// Shifts the wash toward the current state — blue while idle, purple while
  /// working, red on failure — so the window reads at a glance from across a
  /// bench.
  final Color? tint;

  @override
  Widget build(BuildContext context) {
    final isDark = Theme.of(context).brightness == Brightness.dark;
    final accent = tint ?? AppColors.primary;
    return AnimatedContainer(
      duration: const Duration(milliseconds: 600),
      curve: Curves.easeOut,
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: isDark
              ? [
                  AppColors.bg,
                  Color.alphaBlend(
                      accent.withValues(alpha: 0.10), AppColors.bgAlt),
                  AppColors.bg,
                ]
              : [
                  AppColors.bgLight,
                  Color.alphaBlend(accent.withValues(alpha: 0.08), Colors.white),
                  AppColors.bgLight,
                ],
          stops: const [0, 0.45, 1],
        ),
      ),
      child: child,
    );
  }
}
