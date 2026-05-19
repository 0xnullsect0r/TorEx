# Stage 1: Build Flutter web app
FROM ghcr.io/cirruslabs/flutter:stable AS builder
WORKDIR /app
COPY frontend/pubspec.yaml frontend/pubspec.lock* ./
RUN flutter pub get
COPY frontend/ .
RUN flutter build web --release --no-tree-shake-icons

# Stage 2: Serve with nginx
FROM nginx:1.27-alpine
COPY nginx/nginx.conf /etc/nginx/nginx.conf
COPY --from=builder /app/build/web /usr/share/nginx/html
