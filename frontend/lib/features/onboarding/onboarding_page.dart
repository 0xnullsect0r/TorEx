import 'dart:async';

import 'package:convert/convert.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/crypto/key_service.dart';
import '../../core/crypto/wasm_bridge.dart';
import '../../core/storage/secure_storage.dart';
import '../../shared/theme/app_theme.dart';

class OnboardingPage extends ConsumerStatefulWidget {
  const OnboardingPage({super.key});

  @override
  ConsumerState<OnboardingPage> createState() => _OnboardingPageState();
}

class _OnboardingPageState extends ConsumerState<OnboardingPage> {
  bool _showImport = false;
  String? _generatedMnemonic;
  bool _confirmed = false;
  final _importController = TextEditingController();
  String? _error;

  @override
  void initState() {
    super.initState();
    unawaited(WasmCrypto.init());
    _generatedMnemonic = KeyService.generateMnemonic();
  }

  @override
  void dispose() {
    _importController.dispose();
    super.dispose();
  }

  Future<void> _confirmAndSave(String mnemonic) async {
    if (!KeyService.validateMnemonic(mnemonic)) {
      setState(() => _error = 'Invalid mnemonic phrase');
      return;
    }
    final seed = KeyService.mnemonicToSeed(mnemonic);
    final viewKey = KeyService.deriveViewKey(seed);
    await KeyService.saveMnemonic(mnemonic);
    await SecureStorage.saveViewKey(hex.encode(viewKey));
    if (mounted) {
      context.go('/trading');
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const SizedBox(height: 40),
              Text(
                'TorEx',
                style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                      color: AppColors.primary,
                      fontSize: 32,
                      fontWeight: FontWeight.bold,
                    ),
              ),
              const SizedBox(height: 8),
              Text(
                'Privacy-first cryptocurrency exchange',
                style: Theme.of(context)
                    .textTheme
                    .bodyMedium
                    ?.copyWith(color: AppColors.textSecondary),
              ),
              const SizedBox(height: 48),
              if (!_showImport) ...[
                Text('Your recovery phrase', style: Theme.of(context).textTheme.titleMedium),
                const SizedBox(height: 8),
                Text(
                  'Write these 24 words down. They are your identity — no account, no email, no password.',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
                const SizedBox(height: 16),
                if (_generatedMnemonic != null) _MnemonicGrid(mnemonic: _generatedMnemonic!),
                const SizedBox(height: 24),
                Row(
                  children: [
                    Checkbox(
                      value: _confirmed,
                      onChanged: (v) => setState(() => _confirmed = v ?? false),
                      activeColor: AppColors.primary,
                    ),
                    Expanded(
                      child: Text(
                        'I have written down my recovery phrase',
                        style: Theme.of(context).textTheme.bodyMedium,
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 16),
                SizedBox(
                  width: double.infinity,
                  child: ElevatedButton(
                    onPressed: _confirmed && _generatedMnemonic != null
                        ? () => _confirmAndSave(_generatedMnemonic!)
                        : null,
                    child: const Text('Create Wallet'),
                  ),
                ),
                const SizedBox(height: 12),
                TextButton(
                  onPressed: () => setState(() => _showImport = true),
                  child: const Text(
                    'Import existing wallet',
                    style: TextStyle(color: AppColors.textSecondary),
                  ),
                ),
              ] else ...[
                Text('Import wallet', style: Theme.of(context).textTheme.titleMedium),
                const SizedBox(height: 16),
                TextField(
                  controller: _importController,
                  maxLines: 4,
                  decoration: const InputDecoration(
                    hintText: 'Enter your 24-word recovery phrase...',
                    hintStyle: TextStyle(color: AppColors.textSecondary),
                  ),
                ),
                if (_error != null) ...[
                  const SizedBox(height: 8),
                  Text(_error!, style: const TextStyle(color: AppColors.sellRed)),
                ],
                const SizedBox(height: 16),
                SizedBox(
                  width: double.infinity,
                  child: ElevatedButton(
                    onPressed: () => _confirmAndSave(_importController.text.trim()),
                    child: const Text('Import Wallet'),
                  ),
                ),
                const SizedBox(height: 12),
                TextButton(
                  onPressed: () => setState(() {
                    _showImport = false;
                    _error = null;
                  }),
                  child: const Text('Back', style: TextStyle(color: AppColors.textSecondary)),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class _MnemonicGrid extends StatelessWidget {
  final String mnemonic;

  const _MnemonicGrid({required this.mnemonic});

  @override
  Widget build(BuildContext context) {
    final words = mnemonic.split(' ');
    return Container(
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: AppColors.cardBg,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: AppColors.border),
      ),
      child: GridView.builder(
        shrinkWrap: true,
        physics: const NeverScrollableScrollPhysics(),
        gridDelegate: const SliverGridDelegateWithFixedCrossAxisCount(
          crossAxisCount: 4,
          childAspectRatio: 2.5,
          crossAxisSpacing: 8,
          mainAxisSpacing: 8,
        ),
        itemCount: words.length,
        itemBuilder: (context, i) => Row(
          children: [
            Text(
              '${i + 1}. ',
              style: const TextStyle(color: AppColors.textSecondary, fontSize: 11),
            ),
            Expanded(
              child: Text(
                words[i],
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: const TextStyle(
                  color: AppColors.textPrimary,
                  fontSize: 12,
                  fontFamily: 'JetBrainsMono',
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
