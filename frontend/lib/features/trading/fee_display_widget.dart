import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/network/api_client.dart';
import '../../shared/theme/app_theme.dart';
import '../../shared/widgets/mono_text.dart';

final _client = ApiClient();

final myFeeProvider = FutureProvider<Map<String, dynamic>>((ref) async {
  final resp = await _client.get<Map<String, dynamic>>('/fees/my-tier');
  return resp.data ?? {};
});

class FeeDisplayWidget extends ConsumerWidget {
  const FeeDisplayWidget({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final fees = ref.watch(myFeeProvider);
    return fees.when(
      loading: () => const SizedBox.shrink(),
      error: (e, _) => const SizedBox.shrink(),
      data: (data) => Padding(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        child: Row(
          children: [
            MonoText(
              'Maker: ${data['maker_pct'] ?? '-'}',
              fontSize: 11,
              color: AppColors.textSecondary,
            ),
            const SizedBox(width: 12),
            MonoText(
              'Taker: ${data['taker_pct'] ?? '-'}',
              fontSize: 11,
              color: AppColors.textSecondary,
            ),
          ],
        ),
      ),
    );
  }
}
