import 'package:dio/dio.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/network/api_client.dart';
import '../../core/storage/secure_storage.dart';
import '../../shared/theme/app_theme.dart';

final _client = ApiClient();

class AdminLoginPage extends ConsumerStatefulWidget {
  const AdminLoginPage({super.key});

  @override
  ConsumerState<AdminLoginPage> createState() => _AdminLoginPageState();
}

class _AdminLoginPageState extends ConsumerState<AdminLoginPage> {
  final _usernameCtrl = TextEditingController(text: 'admin');
  final _passwordCtrl = TextEditingController();
  final _totpCtrl = TextEditingController();
  bool _loading = false;
  String? _error;
  bool _showTotp = false;
  String? _tempToken;

  @override
  void dispose() {
    _usernameCtrl.dispose();
    _passwordCtrl.dispose();
    _totpCtrl.dispose();
    super.dispose();
  }

  Future<void> _login() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final resp = await _client.post<Map<String, dynamic>>(
        '/admin/login',
        data: {
          'username': _usernameCtrl.text,
          'password': _passwordCtrl.text,
          if (_showTotp) 'totp_code': _totpCtrl.text,
          if (_showTotp && _tempToken != null) 'temp_token': _tempToken,
        },
      );
      final data = resp.data ?? {};
      if (data['totp_required'] == true) {
        setState(() {
          _showTotp = true;
          _tempToken = data['temp_token'] as String?;
        });
        return;
      }
      final token = data['session_token'] as String?;
      if (token != null) {
        await SecureStorage.saveSessionToken(token);
        if (mounted) {
          context.go('/admin/dashboard');
        }
      }
    } on DioException catch (e) {
      setState(() => _error = e.response?.data?.toString() ?? 'Login failed');
    } finally {
      if (mounted) {
        setState(() => _loading = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Center(
        child: SizedBox(
          width: 360,
          child: Card(
            child: Padding(
              padding: const EdgeInsets.all(32),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('TorEx Admin', style: Theme.of(context).textTheme.headlineSmall),
                  const SizedBox(height: 4),
                  const Text(
                    'Administrator access only',
                    style: TextStyle(color: AppColors.textSecondary, fontSize: 12),
                  ),
                  const SizedBox(height: 24),
                  if (!_showTotp) ...[
                    TextField(
                      controller: _usernameCtrl,
                      decoration: const InputDecoration(
                        labelText: 'Username',
                        isDense: true,
                        contentPadding: EdgeInsets.all(10),
                      ),
                    ),
                    const SizedBox(height: 12),
                    TextField(
                      controller: _passwordCtrl,
                      obscureText: true,
                      decoration: const InputDecoration(
                        labelText: 'Password',
                        isDense: true,
                        contentPadding: EdgeInsets.all(10),
                      ),
                      onSubmitted: (_) => _login(),
                    ),
                  ] else ...[
                    const Text(
                      'Enter TOTP code',
                      style: TextStyle(color: AppColors.textSecondary),
                    ),
                    const SizedBox(height: 12),
                    TextField(
                      controller: _totpCtrl,
                      keyboardType: TextInputType.number,
                      maxLength: 6,
                      decoration: const InputDecoration(
                        labelText: '6-digit code',
                        isDense: true,
                        contentPadding: EdgeInsets.all(10),
                        counterText: '',
                      ),
                      onSubmitted: (_) => _login(),
                    ),
                  ],
                  if (_error != null) ...[
                    const SizedBox(height: 8),
                    Text(
                      _error!,
                      style: const TextStyle(color: AppColors.sellRed, fontSize: 12),
                    ),
                  ],
                  const SizedBox(height: 20),
                  SizedBox(
                    width: double.infinity,
                    child: ElevatedButton(
                      onPressed: _loading ? null : _login,
                      child: _loading
                          ? const SizedBox(
                              width: 16,
                              height: 16,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : Text(_showTotp ? 'Verify' : 'Login'),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
