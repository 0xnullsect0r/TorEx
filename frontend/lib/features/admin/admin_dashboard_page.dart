import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:intl/intl.dart';

import '../../core/network/api_client.dart';
import '../../core/storage/secure_storage.dart';
import '../../shared/theme/app_theme.dart';
import '../../shared/widgets/mono_text.dart';

final _client = ApiClient();
final _fmt = NumberFormat('#,##0.00');
final _fmt2 = NumberFormat('#,##0.00');

class AdminDashboardPage extends ConsumerStatefulWidget {
  const AdminDashboardPage({super.key});

  @override
  ConsumerState<AdminDashboardPage> createState() => _AdminDashboardState();
}

class _AdminDashboardState extends ConsumerState<AdminDashboardPage> {
  Map<String, dynamic> _stats = {};
  List<dynamic> _feeTiers = [];
  List<dynamic> _pendingWithdrawals = [];
  bool _loading = true;

  @override
  void initState() {
    super.initState();
    _loadAll();
  }

  Future<void> _loadAll() async {
    setState(() => _loading = true);
    try {
      final token = await SecureStorage.getSessionToken();
      final headers = token != null ? {'X-Admin-Session': token} : <String, dynamic>{};
      final results = await Future.wait([
        _client.get<Map<String, dynamic>>('/admin/api/stats', headers: headers),
        _client.get<Map<String, dynamic>>('/admin/api/fee-tiers', headers: headers),
        _client.get<Map<String, dynamic>>('/admin/api/withdrawals', headers: headers),
      ]);
      setState(() {
        _stats = results[0].data ?? {};
        _feeTiers = (results[1].data?['tiers'] as List?) ?? [];
        _pendingWithdrawals = (results[2].data?['withdrawals'] as List?) ?? [];
      });
    } catch (_) {
    } finally {
      if (mounted) {
        setState(() => _loading = false);
      }
    }
  }

  Future<void> _logout() async {
    await SecureStorage.clearSession();
    if (mounted) {
      context.go('/admin');
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Admin Dashboard'),
        actions: [
          IconButton(icon: const Icon(Icons.refresh), onPressed: _loadAll),
          IconButton(icon: const Icon(Icons.logout), onPressed: _logout),
        ],
      ),
      body: _loading
          ? const Center(child: CircularProgressIndicator())
          : SingleChildScrollView(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  _StatsGrid(stats: _stats),
                  const SizedBox(height: 24),
                  _FeeTiersTable(tiers: _feeTiers),
                  const SizedBox(height: 24),
                  _PendingWithdrawals(
                    withdrawals: _pendingWithdrawals,
                    onRefresh: _loadAll,
                  ),
                ],
              ),
            ),
    );
  }
}

class _StatsGrid extends StatelessWidget {
  final Map<String, dynamic> stats;

  const _StatsGrid({required this.stats});

  @override
  Widget build(BuildContext context) {
    final items = [
      ('Users', stats['total_users']?.toString() ?? '0'),
      ('Active Orders', stats['active_orders']?.toString() ?? '0'),
      ('24h Trades', stats['trades_24h']?.toString() ?? '0'),
      ('24h Volume', '\$${_fmt.format(stats['volume_24h'] ?? 0)}'),
      ('Pending Withdrawals', stats['pending_withdrawals']?.toString() ?? '0'),
      ('24h Deposits', stats['deposits_24h']?.toString() ?? '0'),
    ];
    return GridView.count(
      crossAxisCount: 3,
      shrinkWrap: true,
      physics: const NeverScrollableScrollPhysics(),
      crossAxisSpacing: 8,
      mainAxisSpacing: 8,
      childAspectRatio: 2.5,
      children: items
          .map(
            (item) => Card(
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    Text(
                      item.$1,
                      style: const TextStyle(
                        color: AppColors.textSecondary,
                        fontSize: 11,
                      ),
                    ),
                    const SizedBox(height: 4),
                    MonoText(item.$2, fontSize: 18, fontWeight: FontWeight.bold),
                  ],
                ),
              ),
            ),
          )
          .toList(),
    );
  }
}

class _FeeTiersTable extends StatelessWidget {
  final List<dynamic> tiers;

  const _FeeTiersTable({required this.tiers});

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('Fee Tiers', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        Card(
          child: Table(
            columnWidths: const {
              0: FlexColumnWidth(2),
              1: FlexColumnWidth(2),
              2: FlexColumnWidth(1),
              3: FlexColumnWidth(1),
            },
            children: [
              TableRow(
                decoration: const BoxDecoration(
                  border: Border(bottom: BorderSide(color: AppColors.border)),
                ),
                children: ['Min Volume', 'Max Volume', 'Maker', 'Taker']
                    .map(
                      (h) => Padding(
                        padding: const EdgeInsets.all(10),
                        child: Text(
                          h,
                          style: const TextStyle(
                            color: AppColors.textSecondary,
                            fontSize: 12,
                            fontWeight: FontWeight.w600,
                          ),
                        ),
                      ),
                    )
                    .toList(),
              ),
              ...tiers.map(
                (t) => TableRow(
                  children: [
                    Padding(
                      padding: const EdgeInsets.all(10),
                      child: MonoText('\$${_fmt2.format(t['min_volume'] ?? 0)}', fontSize: 12),
                    ),
                    Padding(
                      padding: const EdgeInsets.all(10),
                      child: MonoText(
                        t['max_volume'] != null ? '\$${_fmt2.format(t['max_volume'])}' : '∞',
                        fontSize: 12,
                      ),
                    ),
                    Padding(
                      padding: const EdgeInsets.all(10),
                      child: MonoText(
                        t['maker_pct']?.toString() ?? '-',
                        fontSize: 12,
                        color: AppColors.buyGreen,
                      ),
                    ),
                    Padding(
                      padding: const EdgeInsets.all(10),
                      child: MonoText(
                        t['taker_pct']?.toString() ?? '-',
                        fontSize: 12,
                        color: AppColors.sellRed,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _PendingWithdrawals extends StatelessWidget {
  final List<dynamic> withdrawals;
  final VoidCallback onRefresh;

  const _PendingWithdrawals({required this.withdrawals, required this.onRefresh});

  Future<void> _approve(String withdrawalId) async {
    final token = await SecureStorage.getSessionToken();
    final headers = token != null ? {'X-Admin-Session': token} : <String, dynamic>{};
    await ApiClient().post('/admin/api/withdrawals/$withdrawalId/approve', headers: headers);
    onRefresh();
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('Pending Withdrawals', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        if (withdrawals.isEmpty)
          const Card(
            child: Padding(
              padding: EdgeInsets.all(16),
              child: Text(
                'No pending withdrawals',
                style: TextStyle(color: AppColors.textSecondary),
              ),
            ),
          )
        else
          ...withdrawals.map(
            (w) {
              final dest = w['dest_address']?.toString() ?? '';
              final shortDest = dest.length > 12 ? '${dest.substring(0, 12)}...' : dest;
              return Card(
                child: ListTile(
                  title: MonoText('${w['amount']} USDT → $shortDest', fontSize: 13),
                  subtitle: Text(
                    '${w['chain']?.toString().toUpperCase()} | ${w['status']}',
                    style: const TextStyle(color: AppColors.textSecondary, fontSize: 11),
                  ),
                  trailing: ElevatedButton(
                    onPressed: () => _approve(w['withdrawal_id']?.toString() ?? ''),
                    style: ElevatedButton.styleFrom(
                      backgroundColor: AppColors.buyGreen,
                      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
                    ),
                    child: const Text('Approve', style: TextStyle(fontSize: 12)),
                  ),
                ),
              );
            },
          ),
      ],
    );
  }
}
