import 'package:dio/dio.dart';

import '../storage/secure_storage.dart';

class ApiClient {
  static const _baseUrl = '/api';

  late final Dio _dio;

  ApiClient() {
    _dio = Dio(
      BaseOptions(
        baseUrl: _baseUrl,
        connectTimeout: const Duration(seconds: 10),
        receiveTimeout: const Duration(seconds: 30),
        headers: {'Content-Type': 'application/json'},
      ),
    );
    _dio.interceptors.add(_SessionInterceptor());
  }

  Dio get dio => _dio;

  Future<Response<T>> get<T>(
    String path, {
    Map<String, dynamic>? queryParameters,
    Map<String, dynamic>? headers,
  }) {
    return _dio.get<T>(
      path,
      queryParameters: queryParameters,
      options: Options(headers: headers),
    );
  }

  Future<Response<T>> post<T>(String path, {dynamic data, Map<String, dynamic>? headers}) {
    return _dio.post<T>(path, data: data, options: Options(headers: headers));
  }

  Future<Response<T>> put<T>(String path, {dynamic data, Map<String, dynamic>? headers}) {
    return _dio.put<T>(path, data: data, options: Options(headers: headers));
  }

  Future<Response<T>> delete<T>(String path, {Map<String, dynamic>? headers}) {
    return _dio.delete<T>(path, options: Options(headers: headers));
  }
}

class _SessionInterceptor extends Interceptor {
  @override
  Future<void> onRequest(RequestOptions options, RequestInterceptorHandler handler) async {
    final hasAdminSession = options.headers.containsKey('X-Admin-Session');
    if (!hasAdminSession) {
      final session = await SecureStorage.getSessionToken();
      if (session != null) {
        options.headers['X-Session-Id'] = session;
      }
    }
    handler.next(options);
  }
}
