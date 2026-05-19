import 'package:flutter/material.dart';

class MonoText extends StatelessWidget {
  final String text;
  final double? fontSize;
  final Color? color;
  final FontWeight? fontWeight;
  final TextAlign? textAlign;
  final int? maxLines;
  final TextOverflow? overflow;

  const MonoText(
    this.text, {
    super.key,
    this.fontSize,
    this.color,
    this.fontWeight,
    this.textAlign,
    this.maxLines,
    this.overflow,
  });

  @override
  Widget build(BuildContext context) {
    return Text(
      text,
      textAlign: textAlign,
      maxLines: maxLines,
      overflow: overflow,
      style: TextStyle(
        fontFamily: 'JetBrainsMono',
        fontSize: fontSize ?? 13,
        color: color ?? Theme.of(context).colorScheme.onSurface,
        fontWeight: fontWeight ?? FontWeight.normal,
      ),
    );
  }
}
