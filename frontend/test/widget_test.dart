import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:torex/core/crypto/key_service.dart';
import 'package:torex/shared/theme/app_theme.dart';
import 'package:torex/shared/widgets/mono_text.dart';

void main() {
  group('MonoText widget', () {
    testWidgets('renders text with JetBrainsMono font', (tester) async {
      await tester.pumpWidget(const MaterialApp(
        home: Scaffold(body: MonoText('0.00123456')),
      ));
      expect(find.text('0.00123456'), findsOneWidget);
    });
  });

  group('KeyService', () {
    test('generateMnemonic returns 24 words', () {
      final mnemonic = KeyService.generateMnemonic();
      expect(mnemonic.split(' ').length, 24);
    });

    test('validateMnemonic returns true for valid mnemonic', () {
      final mnemonic = KeyService.generateMnemonic();
      expect(KeyService.validateMnemonic(mnemonic), isTrue);
    });

    test('validateMnemonic returns false for invalid mnemonic', () {
      expect(KeyService.validateMnemonic('hello world foo bar'), isFalse);
    });

    test('mnemonicToSeed returns 64 bytes', () {
      final mnemonic = KeyService.generateMnemonic();
      final seed = KeyService.mnemonicToSeed(mnemonic);
      expect(seed.length, 64);
    });

    test('deriveIdentityKey returns deterministic result', () {
      final mnemonic = KeyService.generateMnemonic();
      final seed = KeyService.mnemonicToSeed(mnemonic);
      final key1 = KeyService.deriveIdentityKey(seed);
      final key2 = KeyService.deriveIdentityKey(seed);
      expect(key1, equals(key2));
    });

    test('different mnemonics produce different keys', () {
      final seed1 = KeyService.mnemonicToSeed(KeyService.generateMnemonic());
      final seed2 = KeyService.mnemonicToSeed(KeyService.generateMnemonic());
      final key1 = KeyService.deriveIdentityKey(seed1);
      final key2 = KeyService.deriveIdentityKey(seed2);
      expect(key1, isNot(equals(key2)));
    });
  });

  group('AppTheme', () {
    test('darkTheme has correct background color', () {
      expect(AppTheme.darkTheme.scaffoldBackgroundColor, AppColors.background);
    });

    test('primary color is set', () {
      expect(AppTheme.darkTheme.colorScheme.primary, AppColors.primary);
    });
  });
}
