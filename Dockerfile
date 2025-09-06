# Multi-stage build для оптимизации размера образа
FROM rust:1.89-slim as builder

# Устанавливаем необходимые системные зависимости для сборки
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Создаем рабочую директорию
WORKDIR /app

# Копируем весь проект (упрощенный подход)
COPY . .

# Собираем сервер
RUN cargo build --release --bin clipboard-server

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
