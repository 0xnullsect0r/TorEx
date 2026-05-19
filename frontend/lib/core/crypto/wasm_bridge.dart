import 'dart:typed_data';

/// Stub WASM bridge for crypto_primitives Rust crate.
/// In production, replace with wasm-bindgen compiled output loaded via js_interop.
class WasmCrypto {
  static bool _initialized = false;

  static Future<void> init() async {
    // TODO: load wasm-pack output from assets/wasm/crypto_primitives.js
    _initialized = true;
  }

  /// Generate a ZK balance proof (stub returns all zeros).
  static Uint8List generateBalanceProof(Uint8List seed, int amount) {
    if (!_initialized) {
      throw StateError('WasmCrypto not initialized');
    }
    return Uint8List(32);
  }

  /// Generate a ZK withdrawal proof (stub).
  static Uint8List generateWithdrawalProof(Uint8List seed, int amount, String destAddress) {
    if (!_initialized) {
      throw StateError('WasmCrypto not initialized');
    }
    return Uint8List(32);
  }

  /// Perform Noise_XX handshake (stub returns negotiated key).
  static Uint8List noiseHandshake(Uint8List staticKey, Uint8List serverPubkey) {
    if (!_initialized) {
      throw StateError('WasmCrypto not initialized');
    }
    return Uint8List(32);
  }
}
