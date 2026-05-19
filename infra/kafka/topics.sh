#!/usr/bin/env bash
set -e
KAFKA_BIN=/opt/kafka/bin

$KAFKA_BIN/kafka-topics.sh --bootstrap-server localhost:9092 \
  --create --if-not-exists --topic trades \
  --partitions 6 --replication-factor 1

$KAFKA_BIN/kafka-topics.sh --bootstrap-server localhost:9092 \
  --create --if-not-exists --topic withdrawals \
  --partitions 3 --replication-factor 1

$KAFKA_BIN/kafka-topics.sh --bootstrap-server localhost:9092 \
  --create --if-not-exists --topic settlement_complete \
  --partitions 6 --replication-factor 1

echo "Kafka topics created."
