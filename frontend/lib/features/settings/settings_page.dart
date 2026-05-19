import 'package:flutter/material.dart';
        import 'package:flutter_riverpod/flutter_riverpod.dart';
        import 'package:go_router/go_router.dart';

        import '../../app.dart';
        import '../../core/storage/secure_storage.dart';
        import '../../shared/theme/app_theme.dart';

        class SettingsPage extends ConsumerWidget {
          const SettingsPage({super.key});

          @override
          Widget build(BuildContext context, WidgetRef ref) {
            final themeMode = ref.watch(themeProvider);

            return Scaffold(
              appBar: AppBar(
                title: const Text('Settings'),
                leading: IconButton(
                  icon: const Icon(Icons.arrow_back),
                  onPressed: () => context.go('/trading'),
                ),
              ),
              body: ListView(
                children: [
                  SwitchListTile(
                    title: const Text('Dark Mode'),
                    subtitle: const Text(
                      'Toggle light/dark theme',
                      style: TextStyle(color: AppColors.textSecondary, fontSize: 12),
                    ),
                    value: themeMode == ThemeMode.dark,
                    onChanged: (v) => ref.read(themeProvider.notifier).state =
                        v ? ThemeMode.dark : ThemeMode.light,
                    activeColor: AppColors.primary,
                  ),
                  const Divider(),
                  ListTile(
                    title: const Text('Backup Recovery Phrase'),
                    subtitle: const Text(
                      'View your 24-word seed phrase',
                      style: TextStyle(color: AppColors.textSecondary, fontSize: 12),
                    ),
                    trailing: const Icon(Icons.chevron_right, color: AppColors.textSecondary),
                    onTap: () => _showMnemonic(context),
                  ),
                  const Divider(),
                  ListTile(
                    title: const Text('Export View Key'),
                    subtitle: const Text(
                      'Share read-only access to your account',
                      style: TextStyle(color: AppColors.textSecondary, fontSize: 12),
                    ),
                    trailing: const Icon(Icons.chevron_right, color: AppColors.textSecondary),
                    onTap: () => _showViewKey(context),
                  ),
                  const Divider(),
                  ListTile(
                    title: const Text('Clear All Data', style: TextStyle(color: AppColors.sellRed)),
                    subtitle: const Text(
                      'Remove mnemonic and sessions from this device',
                      style: TextStyle(color: AppColors.textSecondary, fontSize: 12),
                    ),
                    onTap: () => _confirmClear(context),
                  ),
                  const Divider(),
                  const Padding(
                    padding: EdgeInsets.all(16),
                    child: Text(
                      'TorEx v1.0.0
Privacy-first exchange. No PII. No email.',
                      style: TextStyle(color: AppColors.textSecondary, fontSize: 12),
                    ),
                  ),
                ],
              ),
            );
          }

          Future<void> _showMnemonic(BuildContext context) async {
            final mnemonic = await SecureStorage.getMnemonic();
            if (!context.mounted) return;
            showDialog(
              context: context,
              builder: (ctx) => AlertDialog(
                backgroundColor: AppColors.surface,
                title: const Text('Recovery Phrase'),
                content: SelectableText(
                  mnemonic ?? 'Not found',
                  style: const TextStyle(fontFamily: 'JetBrainsMono', fontSize: 13),
                ),
                actions: [
                  TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('Close')),
                ],
              ),
            );
          }

          Future<void> _showViewKey(BuildContext context) async {
            final key = await SecureStorage.getViewKey();
            if (!context.mounted) return;
            showDialog(
              context: context,
              builder: (ctx) => AlertDialog(
                backgroundColor: AppColors.surface,
                title: const Text('View Key'),
                content: SelectableText(
                  key ?? 'Not generated',
                  style: const TextStyle(fontFamily: 'JetBrainsMono', fontSize: 11),
                ),
                actions: [
                  TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('Close')),
                ],
              ),
            );
          }

          Future<void> _confirmClear(BuildContext context) async {
            final confirmed = await showDialog<bool>(
              context: context,
              builder: (ctx) => AlertDialog(
                backgroundColor: AppColors.surface,
                title: const Text('Clear All Data?'),
                content: const Text(
                  'This will remove your mnemonic from this device. Make sure you have it backed up.',
                ),
                actions: [
                  TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('Cancel')),
                  TextButton(
                    onPressed: () => Navigator.pop(ctx, true),
                    child: const Text('Clear', style: TextStyle(color: AppColors.sellRed)),
                  ),
                ],
              ),
            );
            if (confirmed == true) {
              await SecureStorage.clearAll();
              if (context.mounted) {
                context.go('/onboarding');
              }
            }
          }
        }
