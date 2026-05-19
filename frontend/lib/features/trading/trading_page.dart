import 'dart:async';

import 'package:dio/dio.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/network/api_client.dart';
import '../../core/network/ws_client.dart';
import '../../shared/theme/app_theme.dart';
import '../../shared/widgets/mono_text.dart';
import 'fee_display_widget.dart';

final _apiClient = ApiClient();
final _wsClient = WsClient();

final selectedPairProvider = StateProvider<String>((ref) => 'BTC_USDT');
final tradingWsProvider = Provider<void>((ref) {
  final pair = ref.watch(selectedPairProvider);
  _wsClient.disconnect();
  unawaited(_wsClient.connect(pair));
  ref.onDispose(_wsClient.disconnect);
});

class TradingPage extends ConsumerStatefulWidget {
  const TradingPage({super.key});

  @override
  ConsumerState<TradingPage> createState() => _TradingPageState();
}

class _TradingPageState extends ConsumerState<TradingPage> {
  int _tabIndex = 1;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('TorEx'),
        actions: [
          TextButton(
            onPressed: () => context.push('/wallet'),
            child: const Text('Wallet', style: TextStyle(color: AppColors.textSecondary)),
          ),
          IconButton(
            icon: const Icon(Icons.settings_outlined),
            onPressed: () => context.push('/settings'),
          ),
        ],
      ),
      body: const _TradingBody(),
      bottomNavigationBar: BottomNavigationBar(
        currentIndex: _tabIndex,
        onTap: (i) {
          if (i == 0) context.go('/wallet');
          if (i == 2) context.go('/settings');
          setState(() => _tabIndex = i);
        },
        items: const [
          BottomNavigationBarItem(
            icon: Icon(Icons.account_balance_wallet_outlined),
            label: 'Wallet',
          ),
          BottomNavigationBarItem(
            icon: Icon(Icons.candlestick_chart_outlined),
            label: 'Trade',
          ),
          BottomNavigationBarItem(
            icon: Icon(Icons.settings_outlined),
            label: 'Settings',
          ),
        ],
      ),
    );
  }
}

class _TradingBody extends ConsumerWidget {
  const _TradingBody();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    ref.watch(tradingWsProvider);
    final isWide = MediaQuery.of(context).size.width > 900;

    if (isWide) {
      return Row(
        children: [
          const SizedBox(width: 260, child: OrderBookWidget()),
          const VerticalDivider(width: 1),
          Expanded(
            child: Column(
              children: [
                const _PairSelector(),
                const FeeDisplayWidget(),
                const Expanded(child: _ChartPlaceholder()),
                const SizedBox(height: 240, child: RecentTradesWidget()),
              ],
            ),
          ),
          const VerticalDivider(width: 1),
          SizedBox(
            width: 320,
            child: Column(
              children: const [
                OrderEntryWidget(),
                Expanded(child: MyOrdersWidget()),
              ],
            ),
          ),
        ],
      );
    }

    return SingleChildScrollView(
      child: Column(
        children: const [
          _PairSelector(),
          FeeDisplayWidget(),
          _ChartPlaceholder(),
          OrderEntryWidget(),
          SizedBox(height: 280, child: OrderBookWidget()),
          SizedBox(height: 240, child: RecentTradesWidget()),
          SizedBox(height: 280, child: MyOrdersWidget()),
        ],
      ),
    );
  }
}

class _PairSelector extends ConsumerWidget {
  const _PairSelector();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final pair = ref.watch(selectedPairProvider);
    final pairs = ['BTC_USDT', 'ETH_USDT', 'SOL_USDT', 'BNB_USDT', 'XRP_USDT', 'ADA_USDT'];
    return SizedBox(
      height: 40,
      child: ListView.builder(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 8),
        itemCount: pairs.length,
        itemBuilder: (context, i) => GestureDetector(
          onTap: () => ref.read(selectedPairProvider.notifier).state = pairs[i],
          child: Container(
            margin: const EdgeInsets.symmetric(horizontal: 4, vertical: 4),
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
            decoration: BoxDecoration(
              color: pair == pairs[i] ? AppColors.primary.withOpacity(0.2) : Colors.transparent,
              borderRadius: BorderRadius.circular(4),
              border: Border.all(
                color: pair == pairs[i] ? AppColors.primary : AppColors.border,
              ),
            ),
            child: MonoText(
              pairs[i],
              fontSize: 12,
              color: pair == pairs[i] ? AppColors.primary : AppColors.textSecondary,
            ),
          ),
        ),
      ),
    );
  }
}

class _ChartPlaceholder extends StatelessWidget {
  const _ChartPlaceholder();

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: const EdgeInsets.all(8),
      child: SizedBox(
        height: 240,
        child: Center(
          child: Text(
            'Chart (connect data source)',
            style: Theme.of(context)
                .textTheme
                .bodyMedium
                ?.copyWith(color: AppColors.textSecondary),
          ),
        ),
      ),
    );
  }
}

class OrderBookWidget extends ConsumerStatefulWidget {
  const OrderBookWidget({super.key});

  @override
  ConsumerState<OrderBookWidget> createState() => _OrderBookState();
}

class _OrderBookState extends ConsumerState<OrderBookWidget> {
  List<List<double>> _asks = [];
  List<List<double>> _bids = [];
  StreamSubscription<Map<String, dynamic>>? _subscription;

  @override
  void initState() {
    super.initState();
    _subscription = _wsClient.stream.listen((msg) {
      if (msg['type'] == 'orderbook' && mounted) {
        setState(() {
          _asks = _parseDepth(msg['asks']);
          _bids = _parseDepth(msg['bids']);
        });
      }
    });
  }

  @override
  void dispose() {
    _subscription?.cancel();
    super.dispose();
  }

  List<List<double>> _parseDepth(dynamic raw) {
    if (raw == null) return [];
    return (raw as List)
        .map((item) {
          final row = item as List;
          return [
            double.tryParse(row[0].toString()) ?? 0,
            double.tryParse(row[1].toString()) ?? 0,
          ];
        })
        .toList();
  }

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: const EdgeInsets.all(8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.all(8),
            child: Text(
              'Order Book',
              style: Theme.of(context).textTheme.titleMedium?.copyWith(fontSize: 13),
            ),
          ),
          _buildHeader(),
          const Divider(height: 1),
          ...(_asks.take(10).toList().reversed.map((row) => _buildRow(row[0], row[1], isBid: false))),
          const Divider(height: 1),
          ...(_bids.take(10).map((row) => _buildRow(row[0], row[1], isBid: true))),
        ],
      ),
    );
  }

  Widget _buildHeader() => const Padding(
        padding: EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        child: Row(
          children: [
            Expanded(child: MonoText('Price', fontSize: 11, color: AppColors.textSecondary)),
            Expanded(
              child: MonoText(
                'Size',
                fontSize: 11,
                color: AppColors.textSecondary,
                textAlign: TextAlign.right,
              ),
            ),
            Expanded(
              child: MonoText(
                'Total',
                fontSize: 11,
                color: AppColors.textSecondary,
                textAlign: TextAlign.right,
              ),
            ),
          ],
        ),
      );

  Widget _buildRow(double price, double qty, {required bool isBid}) => Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
        child: Row(
          children: [
            Expanded(
              child: MonoText(
                price.toStringAsFixed(2),
                fontSize: 12,
                color: isBid ? AppColors.buyGreen : AppColors.sellRed,
              ),
            ),
            Expanded(
              child: MonoText(
                qty.toStringAsFixed(6),
                fontSize: 12,
                color: AppColors.textPrimary,
                textAlign: TextAlign.right,
              ),
            ),
            Expanded(
              child: MonoText(
                (price * qty).toStringAsFixed(2),
                fontSize: 12,
                color: AppColors.textSecondary,
                textAlign: TextAlign.right,
              ),
            ),
          ],
        ),
      );
}

class OrderEntryWidget extends ConsumerStatefulWidget {
  const OrderEntryWidget({super.key});

  @override
  ConsumerState<OrderEntryWidget> createState() => _OrderEntryState();
}

class _OrderEntryState extends ConsumerState<OrderEntryWidget> {
  final _priceCtrl = TextEditingController();
  final _qtyCtrl = TextEditingController();
  bool _isLimit = true;
  bool _isLoading = false;
  String? _error;

  @override
  void dispose() {
    _priceCtrl.dispose();
    _qtyCtrl.dispose();
    super.dispose();
  }

  Future<void> _submitOrder(String side) async {
    setState(() {
      _isLoading = true;
      _error = null;
    });
    try {
      final pair = ref.read(selectedPairProvider);
      final body = {
        'pair': pair,
        'side': side,
        'order_type': _isLimit ? 'limit' : 'market',
        'quantity': double.tryParse(_qtyCtrl.text) ?? 0,
        if (_isLimit) 'price': double.tryParse(_priceCtrl.text) ?? 0,
        'zk_proof': List.filled(32, 0),
      };
      await _apiClient.post('/orders', data: body);
      if (mounted) {
        _qtyCtrl.clear();
        _priceCtrl.clear();
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('${side.toUpperCase()} order placed'),
            backgroundColor: side == 'buy' ? AppColors.buyGreen : AppColors.sellRed,
          ),
        );
      }
    } on DioException catch (e) {
      setState(() => _error = e.response?.data?.toString() ?? e.message);
    } finally {
      if (mounted) {
        setState(() => _isLoading = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: const EdgeInsets.all(8),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Text(
                  'Order',
                  style: Theme.of(context).textTheme.titleMedium?.copyWith(fontSize: 14),
                ),
                const Spacer(),
                ToggleButtons(
                  isSelected: [_isLimit, !_isLimit],
                  onPressed: (i) => setState(() => _isLimit = i == 0),
                  borderRadius: BorderRadius.circular(4),
                  selectedColor: AppColors.primary,
                  fillColor: AppColors.primary.withOpacity(0.15),
                  constraints: const BoxConstraints(minHeight: 28, minWidth: 60),
                  children: const [
                    Text('Limit', style: TextStyle(fontSize: 12)),
                    Text('Market', style: TextStyle(fontSize: 12)),
                  ],
                ),
              ],
            ),
            const SizedBox(height: 12),
            if (_isLimit) ...[
              TextField(
                controller: _priceCtrl,
                keyboardType: const TextInputType.numberWithOptions(decimal: true),
                decoration: const InputDecoration(
                  labelText: 'Price (USDT)',
                  isDense: true,
                  contentPadding: EdgeInsets.all(10),
                ),
              ),
              const SizedBox(height: 8),
            ],
            TextField(
              controller: _qtyCtrl,
              keyboardType: const TextInputType.numberWithOptions(decimal: true),
              decoration: const InputDecoration(
                labelText: 'Amount',
                isDense: true,
                contentPadding: EdgeInsets.all(10),
              ),
            ),
            if (_error != null) ...[
              const SizedBox(height: 8),
              Text(_error!, style: const TextStyle(color: AppColors.sellRed, fontSize: 12)),
            ],
            const SizedBox(height: 12),
            Row(
              children: [
                Expanded(
                  child: ElevatedButton(
                    onPressed: _isLoading ? null : () => _submitOrder('buy'),
                    style: ElevatedButton.styleFrom(backgroundColor: AppColors.buyGreen),
                    child: _isLoading
                        ? const SizedBox(
                            width: 16,
                            height: 16,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Text('Buy'),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: ElevatedButton(
                    onPressed: _isLoading ? null : () => _submitOrder('sell'),
                    style: ElevatedButton.styleFrom(backgroundColor: AppColors.sellRed),
                    child: const Text('Sell'),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class RecentTradesWidget extends ConsumerStatefulWidget {
  const RecentTradesWidget({super.key});

  @override
  ConsumerState<RecentTradesWidget> createState() => _RecentTradesState();
}

class _RecentTradesState extends ConsumerState<RecentTradesWidget> {
  final List<Map<String, dynamic>> _trades = [];
  StreamSubscription<Map<String, dynamic>>? _subscription;

  @override
  void initState() {
    super.initState();
    _subscription = _wsClient.stream.listen((msg) {
      if (msg['type'] == 'trade' && mounted) {
        setState(() {
          _trades.insert(0, msg);
          if (_trades.length > 50) {
            _trades.removeLast();
          }
        });
      }
    });
  }

  @override
  void dispose() {
    _subscription?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: const EdgeInsets.all(8),
      child: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(8),
            child: Row(
              children: [
                Text(
                  'Trades',
                  style: Theme.of(context).textTheme.titleMedium?.copyWith(fontSize: 13),
                ),
              ],
            ),
          ),
          const Divider(height: 1),
          Expanded(
            child: ListView.builder(
              itemCount: _trades.length,
              itemBuilder: (ctx, i) {
                final t = _trades[i];
                final isBuy = t['side'] == 'buy';
                return Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                  child: Row(
                    children: [
                      Expanded(
                        child: MonoText(
                          t['price']?.toString() ?? '-',
                          fontSize: 12,
                          color: isBuy ? AppColors.buyGreen : AppColors.sellRed,
                        ),
                      ),
                      Expanded(
                        child: MonoText(
                          t['quantity']?.toString() ?? '-',
                          fontSize: 12,
                          color: AppColors.textPrimary,
                          textAlign: TextAlign.right,
                        ),
                      ),
                    ],
                  ),
                );
              },
            ),
          ),
        ],
      ),
    );
  }
}

class MyOrdersWidget extends ConsumerStatefulWidget {
  const MyOrdersWidget({super.key});

  @override
  ConsumerState<MyOrdersWidget> createState() => _MyOrdersState();
}

class _MyOrdersState extends ConsumerState<MyOrdersWidget> {
  List<Map<String, dynamic>> _orders = [];
  bool _loading = false;

  @override
  void initState() {
    super.initState();
    _loadOrders();
  }

  Future<void> _loadOrders() async {
    setState(() => _loading = true);
    try {
      final resp = await _apiClient.get<Map<String, dynamic>>('/orders');
      final data = resp.data ?? {};
      setState(() => _orders = List<Map<String, dynamic>>.from(data['orders'] ?? []));
    } catch (_) {
    } finally {
      if (mounted) {
        setState(() => _loading = false);
      }
    }
  }

  Future<void> _cancel(String orderId) async {
    try {
      await _apiClient.delete('/orders/$orderId');
      await _loadOrders();
    } catch (_) {}
  }

  @override
  Widget build(BuildContext context) {
    return Card(
      margin: const EdgeInsets.all(8),
      child: Column(
        children: [
          Padding(
            padding: const EdgeInsets.all(8),
            child: Row(
              children: [
                Text(
                  'Open Orders',
                  style: Theme.of(context).textTheme.titleMedium?.copyWith(fontSize: 13),
                ),
                const Spacer(),
                IconButton(icon: const Icon(Icons.refresh, size: 16), onPressed: _loadOrders),
              ],
            ),
          ),
          const Divider(height: 1),
          Expanded(
            child: _loading
                ? const Center(child: CircularProgressIndicator())
                : _orders.isEmpty
                    ? const Center(
                        child: Text(
                          'No open orders',
                          style: TextStyle(color: AppColors.textSecondary),
                        ),
                      )
                    : ListView.builder(
                        itemCount: _orders.length,
                        itemBuilder: (context, index) {
                          final o = _orders[index];
                          final side = o['side']?.toString();
                          final isBuy = side == 'buy';
                          return ListTile(
                            dense: true,
                            title: Row(
                              children: [
                                MonoText(o['pair']?.toString() ?? '', fontSize: 12),
                                const SizedBox(width: 8),
                                Container(
                                  padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                                  decoration: BoxDecoration(
                                    color: isBuy
                                        ? AppColors.buyGreen.withOpacity(0.2)
                                        : AppColors.sellRed.withOpacity(0.2),
                                    borderRadius: BorderRadius.circular(4),
                                  ),
                                  child: MonoText(
                                    side?.toUpperCase() ?? '',
                                    fontSize: 10,
                                    color: isBuy ? AppColors.buyGreen : AppColors.sellRed,
                                  ),
                                ),
                              ],
                            ),
                            subtitle: MonoText(
                              '${o['price']} @ ${o['quantity']}',
                              fontSize: 11,
                              color: AppColors.textSecondary,
                            ),
                            trailing: IconButton(
                              icon: const Icon(
                                Icons.close,
                                size: 14,
                                color: AppColors.textSecondary,
                              ),
                              onPressed: () => _cancel(o['order_id']?.toString() ?? ''),
                            ),
                          );
                        },
                      ),
          ),
        ],
      ),
    );
  }
}
