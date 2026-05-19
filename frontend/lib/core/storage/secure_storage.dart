import 'package:flutter_secure_storage/flutter_secure_storage.dart';

class SecureStorage {
  static const _storage = FlutterSecureStorage();

  static const _mnemonicKey = 'torex_mnemonic';
  static const _sessionKey = 'torex_session';
  static const _viewKeyKey = 'torex_view_key';

  static Future<void> saveMnemonic(String mnemonic) => _storage.write(key: _mnemonicKey, value: mnemonic);

  static Future<String?> getMnemonic() => _storage.read(key: _mnemonicKey);

  static Future<void> deleteMnemonic() => _storage.delete(key: _mnemonicKey);

  static Future<void> saveSessionToken(String token) => _storage.write(key: _sessionKey, value: token);

  static Future<String?> getSessionToken() => _storage.read(key: _sessionKey);

  static Future<void> clearSession() => _storage.delete(key: _sessionKey);

  static Future<void> saveViewKey(String viewKeyHex) => _storage.write(key: _viewKeyKey, value: viewKeyHex);

  static Future<String?> getViewKey() => _storage.read(key: _viewKeyKey);

  static Future<void> clearAll() => _storage.deleteAll();
}
