# Multi-stage build для оптимизации размера образа
FROM rust:1.89-slim as builder

# Устанавливаем необходимые системные зависимости для сборки
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Создаем рабочую директорию
WORKDIR /app

# Копируем только конфигурацию сервера
COPY server/Cargo.toml ./

# Создаем пустой файл для предварительной сборки зависимостей
RUN mkdir -p src
RUN echo "fn main() {}" > src/main.rs

# Собираем только зависимости (кэшируется Docker)
RUN cargo build --release

# Удаляем временные файлы
RUN rm -rf src

# Копируем реальный исходный код сервера
COPY server/src ./src

# Пересобираем только наш код
RUN touch src/main.rs && cargo build --release

# Финальный образ
FROM debian:bookworm-slim

# Устанавливаем только runtime зависимости
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && update-ca-certificates

# Создаем пользователя без прав root
RUN useradd -r -s /bin/false clipboard-server

# Создаем рабочую директорию
WORKDIR /app

# Копируем скомпилированный бинарный файл
COPY --from=builder /app/target/release/clipboard-server /app/clipboard-server

# Устанавливаем права доступа
RUN chown clipboard-server:clipboard-server /app/clipboard-server

# Переключаемся на непривилегированного пользователя
USER clipboard-server

# Открываем порт
EXPOSE 8080

# Устанавливаем переменные окружения
ENV RUST_LOG=info
ENV RUST_BACKTRACE=1
ENV DOCKER_ENV=true

# Команда запуска
CMD ["./clipboard-server"]
