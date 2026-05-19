pub mod error {
    use std::fmt::Display;

    use axum::{
        http::StatusCode,
        response::{IntoResponse, Response},
        Json,
    };
    use serde::Serialize;

    #[derive(Debug, thiserror::Error)]
    pub enum AppError {
        #[error("resource not found")]
        NotFound,
        #[error("unauthorized")]
        Unauthorized,
        #[error("bad request: {0}")]
        BadRequest(String),
        #[error("conflict")]
        Conflict,
        #[error(transparent)]
        Internal(#[from] anyhow::Error),
    }

    #[derive(Serialize)]
    struct ErrorBody {
        error: String,
    }

    impl IntoResponse for AppError {
        fn into_response(self) -> Response {
            let status = match &self {
                AppError::NotFound => StatusCode::NOT_FOUND,
                AppError::Unauthorized => StatusCode::UNAUTHORIZED,
                AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
                AppError::Conflict => StatusCode::CONFLICT,
                AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let body = Json(ErrorBody {
                error: display_string(&self),
            });
            (status, body).into_response()
        }
    }

    fn display_string(value: &impl Display) -> String {
        value.to_string()
    }
}

pub mod db {
    use anyhow::Context;
    use sqlx::{postgres::PgPoolOptions, PgPool};

    pub async fn create_pool(url: &str) -> PgPool {
        PgPoolOptions::new()
            .max_connections(10)
            .connect(url)
            .await
            .context("failed to connect to postgres")
            .expect("postgres connection")
    }
}

pub mod cache {
    use anyhow::Context;
    use redis::{aio::ConnectionManager, Client};

    pub async fn create_redis(url: &str) -> ConnectionManager {
        let client = Client::open(url).context("failed to open redis client").expect("redis client");
        client
            .get_connection_manager()
            .await
            .context("failed to connect to redis")
            .expect("redis connection")
    }
}

pub mod kafka {
    use anyhow::Context;
    use rdkafka::{
        consumer::{Consumer, StreamConsumer},
        producer::{FutureProducer, FutureRecord},
        util::Timeout,
        ClientConfig,
    };

    #[derive(Clone)]
    pub struct KafkaProducer {
        inner: FutureProducer,
    }

    impl KafkaProducer {
        pub fn new(brokers: &str) -> anyhow::Result<Self> {
            let inner = ClientConfig::new()
                .set("bootstrap.servers", brokers)
                .create()
                .context("failed to create kafka producer")?;
            Ok(Self { inner })
        }

        pub async fn send(&self, topic: &str, key: &str, payload: &str) -> anyhow::Result<()> {
            self.inner
                .send(
                    FutureRecord::to(topic).key(key).payload(payload),
                    Timeout::Never,
                )
                .await
                .map_err(|(err, _)| anyhow::anyhow!(err))
                .context("failed to send kafka message")?;
            Ok(())
        }

        pub fn inner(&self) -> &FutureProducer {
            &self.inner
        }
    }

    pub struct KafkaConsumer {
        inner: StreamConsumer,
    }

    impl KafkaConsumer {
        pub fn new(brokers: &str, group_id: &str, topics: &[&str]) -> anyhow::Result<Self> {
            let inner: StreamConsumer = ClientConfig::new()
                .set("bootstrap.servers", brokers)
                .set("group.id", group_id)
                .set("enable.partition.eof", "false")
                .set("auto.offset.reset", "earliest")
                .create()
                .context("failed to create kafka consumer")?;
            inner.subscribe(topics).context("failed to subscribe kafka consumer")?;
            Ok(Self { inner })
        }

        pub fn inner(&self) -> &StreamConsumer {
            &self.inner
        }
    }
}

pub mod types {
    use std::{fmt, str::FromStr};

    use anyhow::{anyhow, Context};
    use bigdecimal::BigDecimal;
    use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize, Serializer};
    use uuid::Uuid;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct UserId(pub [u8; 32]);

    impl UserId {
        pub fn to_hex(self) -> String {
            hex::encode(self.0)
        }

        pub fn from_hex(value: &str) -> anyhow::Result<Self> {
            let bytes = hex::decode(value).context("invalid user id hex")?;
            let arr: [u8; 32] = bytes.try_into().map_err(|_| anyhow!("invalid user id length"))?;
            Ok(Self(arr))
        }
    }

    impl Serialize for UserId {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_str(&hex::encode(self.0))
        }
    }

    impl<'de> Deserialize<'de> for UserId {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let value = String::deserialize(deserializer)?;
            let bytes = hex::decode(&value).map_err(D::Error::custom)?;
            let arr: [u8; 32] = bytes.try_into().map_err(|_| D::Error::custom("invalid user id length"))?;
            Ok(Self(arr))
        }
    }

    impl fmt::Display for UserId {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", hex::encode(self.0))
        }
    }

    #[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
    pub struct OrderId(pub Uuid);

    #[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
    pub struct TradeId(pub Uuid);

    #[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Amount(pub i64);

    impl Amount {
        pub fn from_decimal(decimal: &BigDecimal) -> anyhow::Result<Self> {
            let scaled = decimal * BigDecimal::from(1_000_000_i64);
            let normalized = scaled.with_scale(0);
            let value = i64::from_str(&normalized.to_string()).context("amount out of range")?;
            Ok(Self(value))
        }

        pub fn to_decimal(self) -> BigDecimal {
            BigDecimal::from(self.0) / BigDecimal::from(1_000_000_i64)
        }
    }
}

pub mod config {
    use anyhow::Context;

    #[derive(Clone, Debug)]
    pub struct AppConfig {
        pub postgres_url: String,
        pub redis_url: String,
        pub kafka_broker: String,
        pub vault_addr: String,
        pub vault_token: String,
        pub eth_node_url: String,
        pub tron_node_url: String,
        pub tron_pro_api_key: String,
        pub hcaptcha_secret: String,
    }

    impl AppConfig {
        pub fn from_env() -> anyhow::Result<Self> {
            dotenvy::dotenv().ok();
            Ok(Self {
                postgres_url: std::env::var("POSTGRES_URL").context("POSTGRES_URL missing")?,
                redis_url: std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string()),
                kafka_broker: std::env::var("KAFKA_BROKER").unwrap_or_else(|_| "127.0.0.1:9092".to_string()),
                vault_addr: std::env::var("VAULT_ADDR").unwrap_or_else(|_| "http://127.0.0.1:8200".to_string()),
                vault_token: std::env::var("VAULT_TOKEN").unwrap_or_default(),
                eth_node_url: std::env::var("ETH_NODE_URL").unwrap_or_default(),
                tron_node_url: std::env::var("TRON_NODE_URL").unwrap_or_default(),
                tron_pro_api_key: std::env::var("TRON_PRO_API_KEY").unwrap_or_default(),
                hcaptcha_secret: std::env::var("HCAPTCHA_SECRET").unwrap_or_default(),
            })
        }
    }
}
