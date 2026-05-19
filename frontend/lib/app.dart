import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import 'core/storage/secure_storage.dart';
import 'features/admin/admin_dashboard_page.dart';
import 'features/admin/admin_login_page.dart';
import 'features/onboarding/onboarding_page.dart';
import 'features/settings/settings_page.dart';
import 'features/trading/trading_page.dart';
import 'features/wallet/wallet_page.dart';
import 'shared/theme/app_theme.dart';

final _router = GoRouter(
  initialLocation: '/',
  routes: [
    GoRoute(path: '/', builder: (ctx, state) => const AuthGate()),
    GoRoute(path: '/onboarding', builder: (ctx, state) => const OnboardingPage()),
    GoRoute(path: '/trading', builder: (ctx, state) => const TradingPage()),
    GoRoute(path: '/wallet', builder: (ctx, state) => const WalletPage()),
    GoRoute(path: '/settings', builder: (ctx, state) => const SettingsPage()),
    GoRoute(path: '/admin', builder: (ctx, state) => const AdminLoginPage()),
    GoRoute(path: '/admin/dashboard', builder: (ctx, state) => const AdminDashboardPage()),
  ],
);

final themeProvider = StateProvider<ThemeMode>((ref) => ThemeMode.dark);

class TorExApp extends ConsumerWidget {
  const TorExApp({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final themeMode = ref.watch(themeProvider);
    return MaterialApp.router(
      title: 'TorEx',
      theme: AppTheme.lightTheme,
      darkTheme: AppTheme.darkTheme,
      themeMode: themeMode,
      routerConfig: _router,
      debugShowCheckedModeBanner: false,
    );
  }
}

class AuthGate extends ConsumerWidget {
  const AuthGate({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return FutureBuilder<String?>(
      future: SecureStorage.getMnemonic(),
      builder: (context, snapshot) {
        if (snapshot.connectionState != ConnectionState.done) {
          return const Scaffold(body: Center(child: CircularProgressIndicator()));
        }
        final destination = snapshot.data != null ? '/trading' : '/onboarding';
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (context.mounted) {
            context.go(destination);
          }
        });
        return const Scaffold(body: Center(child: CircularProgressIndicator()));
      },
    );
  }
}
