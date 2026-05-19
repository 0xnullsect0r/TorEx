import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../../core/network/api_client.dart';
import '../../shared/theme/app_theme.dart';
import '../../shared/widgets/mono_text.dart';

final _client = ApiClient();

class WalletPage extends ConsumerStatefulWidget {
  const WalletPage({super.key});

  @override
  ConsumerState<WalletPage> createState() => _WalletPageState();
}

class _WalletPageState extends ConsumerState<WalletPage> {
  Map<String, dynamic> _balance = {};
  String? _depositAddress;
  bool _loadingDeposit = false;
  final _withdrawAmountCtrl = TextEditingController();
  final _withdrawAddrCtrl = TextEditingController();
  String _selectedChain = 'tron';
  bool _withdrawing = false;
  String? _withdrawError;

  @override
  void initState() {
    super.initState();
    _loadBalance();
  }

  @override
  void dispose() {
    _withdrawAmountCtrl.dispose();
    _withdrawAddrCtrl.dispose();
    super.dispose();
  }

  Future<void> _loadBalance() async {
    try {
      final resp = await _client.get<Map<String, dynamic>>('/wallet/balance');
      setState(() => _balance = resp.data ?? {});
    } catch (_) {}
  }

  Future<void> _getDepositAddress() async {
    setState(() => _loadingDeposit = true);
    try {
      final resp = await _client.post<Map<String, dynamic>>(
        '/wallet/deposit-address',
        data: {'chain': _selectedChain},
      );
      setState(() => _depositAddress = resp.data?['address'] as String?);
    } catch (_) {
    } finally {
      if (mounted) {
        setState(() => _loadingDeposit = false);
      }
    }
  }

  Future<void> _withdraw() async {
    setState(() {
      _withdrawing = true;
      _withdrawError = null;
    });
    try {
      await _client.post(
        '/wallet/withdraw',
        data: {
          'amount': double.tryParse(_withdrawAmountCtrl.text) ?? 0,
          'dest_address': _withdrawAddrCtrl.text.trim(),
          'chain': _selectedChain,
          'zk_proof': List.filled(32, 0),
        },
      );
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text('Withdrawal submitted'),
            backgroundColor: AppColors.buyGreen,
          ),
        );
        _withdrawAmountCtrl.clear();
        _withdrawAddrCtrl.clear();
      }
    } catch (e) {
      setState(() => _withdrawError = e.toString());
    } finally {
      if (mounted) {
        setState(() => _withdrawing = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Wallet'),
        leading: IconButton(
          icon: const Icon(Icons.arrow_back),
          onPressed: () => context.go('/trading'),
        ),
      ),
      body: SingleChildScrollView(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Card(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('Balance', style: Theme.of(context).textTheme.titleMedium),
                    const SizedBox(height: 12),
                    MonoText(
                      '${_balance['usdt'] ?? '0.00'} USDT',
                      fontSize: 28,
                      fontWeight: FontWeight.bold,
                    ),
                    const SizedBox(height: 8),
                    TextButton(onPressed: _loadBalance, child: const Text('Refresh')),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 16),
            Row(
              children: [
                Text('Chain:', style: Theme.of(context).textTheme.bodyMedium),
                const SizedBox(width: 12),
                DropdownButton<String>(
                  value: _selectedChain,
                  dropdownColor: AppColors.surface,
                  items: const [
                    DropdownMenuItem(value: 'tron', child: Text('TRC-20 (TRON)')),
                    DropdownMenuItem(value: 'eth', child: Text('ERC-20 (Ethereum)')),
                    DropdownMenuItem(value: 'bsc', child: Text('BEP-20 (BSC)')),
                  ],
                  onChanged: (v) => setState(() {
                    _selectedChain = v!;
                    _depositAddress = null;
                  }),
                ),
              ],
            ),
            const SizedBox(height: 16),
            Card(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('Deposit', style: Theme.of(context).textTheme.titleMedium),
                    const SizedBox(height: 12),
                    if (_depositAddress == null)
                      ElevatedButton(
                        onPressed: _loadingDeposit ? null : _getDepositAddress,
                        child: _loadingDeposit
                            ? const SizedBox(
                                width: 16,
                                height: 16,
                                child: CircularProgressIndicator(strokeWidth: 2),
                              )
                            : const Text('Generate Deposit Address'),
                      )
                    else ...[
                      QrImageView(data: _depositAddress!, size: 180, backgroundColor: Colors.white),
                      const SizedBox(height: 8),
                      GestureDetector(
                        onTap: () {
                          Clipboard.setData(ClipboardData(text: _depositAddress!));
                          ScaffoldMessenger.of(context).showSnackBar(
                            const SnackBar(content: Text('Address copied')),
                          );
                        },
                        child: MonoText(_depositAddress!, fontSize: 12, color: AppColors.primary),
                      ),
                      const SizedBox(height: 8),
                      const Text(
                        'This stealth address expires in 48 hours.',
                        style: TextStyle(color: AppColors.textSecondary, fontSize: 12),
                      ),
                    ],
                  ],
                ),
              ),
            ),
            const SizedBox(height: 16),
            Card(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text('Withdraw', style: Theme.of(context).textTheme.titleMedium),
                    const SizedBox(height: 12),
                    TextField(
                      controller: _withdrawAddrCtrl,
                      decoration: const InputDecoration(
                        labelText: 'Destination address',
                        isDense: true,
                        contentPadding: EdgeInsets.all(10),
                      ),
                    ),
                    const SizedBox(height: 8),
                    TextField(
                      controller: _withdrawAmountCtrl,
                      keyboardType: const TextInputType.numberWithOptions(decimal: true),
                      decoration: const InputDecoration(
                        labelText: 'Amount (USDT)',
                        isDense: true,
                        contentPadding: EdgeInsets.all(10),
                      ),
                    ),
                    if (_withdrawError != null) ...[
                      const SizedBox(height: 8),
                      Text(
                        _withdrawError!,
                        style: const TextStyle(color: AppColors.sellRed, fontSize: 12),
                      ),
                    ],
                    const SizedBox(height: 12),
                    SizedBox(
                      width: double.infinity,
                      child: ElevatedButton(
                        onPressed: _withdrawing ? null : _withdraw,
                        child: _withdrawing
                            ? const SizedBox(
                                width: 16,
                                height: 16,
                                child: CircularProgressIndicator(strokeWidth: 2),
                              )
                            : const Text('Withdraw USDT'),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
