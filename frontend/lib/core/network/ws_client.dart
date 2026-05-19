import 'dart:async';
import 'dart:convert';

import 'package:web_socket_channel/web_socket_channel.dart';

import '../storage/secure_storage.dart';

class WsClient {
  WebSocketChannel? _channel;
  StreamSubscription? _subscription;
  final StreamController<Map<String, dynamic>> _controller = StreamController.broadcast();

  Stream<Map<String, dynamic>> get stream => _controller.stream;

  Future<void> connect(String pair) async {
    disconnect();
    final session = await SecureStorage.getSessionToken();
    final uri = _buildUri();
    _channel = WebSocketChannel.connect(uri);
    if (session != null) {
      _channel!.sink.add(jsonEncode({'action': 'auth', 'session': session}));
    }
    _channel!.sink.add(jsonEncode({'action': 'subscribe', 'channel': 'orderbook', 'pair': pair}));
    _channel!.sink.add(jsonEncode({'action': 'subscribe', 'channel': 'trades', 'pair': pair}));
    _subscription = _channel!.stream.listen(
      (data) {
        try {
          final msg = jsonDecode(data as String) as Map<String, dynamic>;
          _controller.add(msg);
        } catch (_) {}
      },
      onDone: () => _controller.add({'type': 'disconnected'}),
      onError: (e) => _controller.add({'type': 'error', 'message': e.toString()}),
    );
  }

  Uri _buildUri() {
    final base = Uri.base;
    final scheme = base.scheme == 'https' ? 'wss' : 'ws';
    final host = base.host.isEmpty ? 'localhost' : base.host;
    return Uri(
      scheme: scheme,
      host: host,
      port: base.hasPort ? base.port : null,
      path: '/ws',
    );
  }

  void disconnect() {
    _subscription?.cancel();
    _subscription = null;
    _channel?.sink.close();
    _channel = null;
  }
}
