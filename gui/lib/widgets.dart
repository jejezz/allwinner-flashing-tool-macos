import 'package:flutter/material.dart';

import 'theme.dart';

/// The single card surface used across the app: a translucent panel with a
/// hairline border and an optional accent wash. Ported from the Saturn client.
class GlassCard extends StatelessWidget {
  const GlassCard({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(18),
    this.accent,
    this.active = false,
    this.onTap,
    this.radius = AppRadius.card,
  });

  final Widget child;
  final EdgeInsetsGeometry padding;
  final Color? accent;

  /// Lifts the card with the accent wash and a matching glow.
  final bool active;
  final VoidCallback? onTap;
  final double radius;

  @override
  Widget build(BuildContext context) {
    final isDark = Theme.of(context).brightness == Brightness.dark;
    final tint = accent ?? AppColors.primary;
    final base = isDark ? AppColors.surface : AppColors.surfaceLight;

    return Material(
      color: Colors.transparent,
      child: InkWell(
        onTap: onTap,
        borderRadius: BorderRadius.circular(radius),
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 260),
          curve: Curves.easeOut,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(radius),
            gradient: LinearGradient(
              begin: Alignment.topLeft,
              end: Alignment.bottomRight,
              colors: active
                  ? [
                      Color.alphaBlend(tint.withValues(alpha: 0.26), base),
                      Color.alphaBlend(tint.withValues(alpha: 0.08), base),
                    ]
                  : [
                      base,
                      isDark
                          ? Color.alphaBlend(
                              Colors.white.withValues(alpha: 0.02), base)
                          : base,
                    ],
            ),
            border: Border.all(
              color: active
                  ? tint.withValues(alpha: 0.55)
                  : (isDark ? AppColors.stroke : AppColors.strokeLight),
            ),
            boxShadow: [
              if (active)
                BoxShadow(
                  color: tint.withValues(alpha: isDark ? 0.24 : 0.18),
                  blurRadius: 26,
                  spreadRadius: -6,
                  offset: const Offset(0, 10),
                )
              else if (!isDark)
                BoxShadow(
                  color: Colors.black.withValues(alpha: 0.05),
                  blurRadius: 18,
                  offset: const Offset(0, 8),
                ),
            ],
          ),
          child: Padding(padding: padding, child: child),
        ),
      ),
    );
  }
}

/// The product mark: the phoenix on the same dark plate as the `.app` icon, so
/// the window and the Dock read as one thing. The plate is drawn rather than
/// baked into the asset, which keeps its corners crisp at any size and lets it
/// share the palette with the rest of the UI.
class AppMark extends StatelessWidget {
  const AppMark({super.key, this.size = 44});

  final double size;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(
        // Apple's icon-grid corner radius, as a fraction of the tile.
        borderRadius: BorderRadius.circular(size * 0.2237),
        gradient: const LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [Color(0xFF1F2A38), AppColors.bg],
        ),
        border: Border.all(color: AppColors.stroke),
      ),
      child: Padding(
        // Matches the margin the glyph has inside the .app icon.
        padding: EdgeInsets.all(size * 0.14),
        child: Image.asset(
          'assets/phoenix.png',
          filterQuality: FilterQuality.medium,
        ),
      ),
    );
  }
}

/// Rounded square icon badge.
class IconBadge extends StatelessWidget {
  const IconBadge({
    super.key,
    required this.icon,
    required this.color,
    this.active = false,
    this.size = 46,
  });

  final IconData icon;
  final Color color;
  final bool active;
  final double size;

  @override
  Widget build(BuildContext context) {
    return AnimatedContainer(
      duration: const Duration(milliseconds: 220),
      width: size,
      height: size,
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(size * 0.32),
        color: active
            ? color.withValues(alpha: 0.9)
            : color.withValues(alpha: 0.14),
      ),
      child: Icon(
        icon,
        size: size * 0.52,
        color: active ? Colors.black.withValues(alpha: 0.85) : color,
      ),
    );
  }
}

/// Small status pill.
class StatusPill extends StatelessWidget {
  const StatusPill({
    super.key,
    required this.label,
    required this.color,
    this.icon,
    this.pulsing = false,
  });

  final String label;
  final Color color;
  final IconData? icon;

  /// Breathes the leading dot — used while waiting on the board.
  final bool pulsing;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.14),
        borderRadius: BorderRadius.circular(AppRadius.chip),
        border: Border.all(color: color.withValues(alpha: 0.35)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (icon != null)
            Icon(icon, size: 14, color: color)
          else
            _Dot(color: color, pulsing: pulsing),
          const SizedBox(width: 7),
          Text(
            label,
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w700,
              color: color,
            ),
          ),
        ],
      ),
    );
  }
}

class _Dot extends StatefulWidget {
  const _Dot({required this.color, required this.pulsing});

  final Color color;
  final bool pulsing;

  @override
  State<_Dot> createState() => _DotState();
}

class _DotState extends State<_Dot> with SingleTickerProviderStateMixin {
  late final AnimationController _controller = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1400),
  );

  @override
  void initState() {
    super.initState();
    if (widget.pulsing) _controller.repeat(reverse: true);
  }

  @override
  void didUpdateWidget(covariant _Dot oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.pulsing && !_controller.isAnimating) {
      _controller.repeat(reverse: true);
    } else if (!widget.pulsing && _controller.isAnimating) {
      _controller.stop();
      _controller.value = 1;
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return FadeTransition(
      opacity: widget.pulsing
          ? Tween<double>(begin: 0.35, end: 1).animate(_controller)
          : const AlwaysStoppedAnimation(1),
      child: Container(
        width: 8,
        height: 8,
        decoration: BoxDecoration(color: widget.color, shape: BoxShape.circle),
      ),
    );
  }
}

/// Section title with an optional trailing action.
class SectionHeader extends StatelessWidget {
  const SectionHeader({
    super.key,
    required this.title,
    this.subtitle,
    this.trailing,
  });

  final String title;
  final String? subtitle;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(4, 0, 4, 12),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title, style: theme.textTheme.titleMedium),
                if (subtitle != null) ...[
                  const SizedBox(height: 2),
                  Text(subtitle!, style: theme.textTheme.labelSmall),
                ],
              ],
            ),
          ),
          ?trailing,
        ],
      ),
    );
  }
}

/// A progress bar with rounded ends, matching the card radii.
class GlassProgressBar extends StatelessWidget {
  const GlassProgressBar({
    super.key,
    required this.value,
    this.color,
    this.height = 8,
  });

  /// Null renders the indeterminate sweep, for work with no byte count yet.
  final double? value;
  final Color? color;
  final double height;

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(height),
      child: LinearProgressIndicator(
        value: value,
        minHeight: height,
        color: color,
      ),
    );
  }
}
