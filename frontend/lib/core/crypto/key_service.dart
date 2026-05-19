import 'dart:convert';
import 'dart:typed_data';

import 'package:bip39/bip39.dart' as bip39;
import 'package:crypto/crypto.dart';

import '../storage/secure_storage.dart';

class KeyService {
  static String generateMnemonic() => bip39.generateMnemonic(strength: 256);

  static bool validateMnemonic(String mnemonic) => bip39.validateMnemonic(mnemonic);

  static Uint8List mnemonicToSeed(String mnemonic) {
    final seedHex = bip39.mnemonicToSeedHex(mnemonic);
    final bytes = <int>[];
    for (var i = 0; i < seedHex.length; i += 2) {
      bytes.add(int.parse(seedHex.substring(i, i + 2), radix: 16));
    }
    return Uint8List.fromList(bytes);
  }

  static Uint8List deriveIdentityKey(Uint8List seed) {
    final hmac = Hmac(sha512, utf8.encode('torex-identity-v1'));
    return Uint8List.fromList(hmac.convert(seed).bytes);
  }

  static Uint8List deriveViewKey(Uint8List seed) {
    final hmac = Hmac(sha512, utf8.encode('torex-view-v1'));
    return Uint8List.fromList(hmac.convert(seed).bytes.sublist(0, 32));
  }

  static Uint8List deriveSpendKey(Uint8List seed) {
    final hmac = Hmac(sha512, utf8.encode('torex-spend-v1'));
    return Uint8List.fromList(hmac.convert(seed).bytes.sublist(0, 32));
  }

  static String pubkeyHex(Uint8List identityKey) {
    final digest = sha256.convert(identityKey.sublist(0, 32));
    return digest.bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
  }

  static Future<void> saveMnemonic(String mnemonic) async {
    await SecureStorage.saveMnemonic(mnemonic);
  }

  static Future<String?> loadMnemonic() async {
    return SecureStorage.getMnemonic();
  }
}
