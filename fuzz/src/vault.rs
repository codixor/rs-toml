//! Mirrors of the `vault` service's serde configuration types, so the fuzz
//! targets can deserialize them with both rs-toml and the `toml` crate.
//!
//! Copied from `vault/crates/vault-server/src/config.rs` and
//! `vault/crates/vault-import/src/config.rs` with every serde attribute and
//! `deserialize_with` helper kept. Only the serde surface is mirrored:
//! validation (`Config::validate`, `check`) runs after deserialization in
//! vault and is not part of what is compared here.
//!
//! Substitutions (all marked `SUBST` below):
//! - `vault_core::logical::parse_duration` is copied verbatim into
//!   [`parse_duration`], with vault-core's `Error` replaced by `String`.
//! - `PartialEq` is derived in addition to `Debug` (and `Clone` where vault
//!   has it) so values can be compared. No mirrored type has an `f64`
//!   field, so no NaN-aware comparison is needed.
//!
//! Every other field type (`PathBuf`, `SocketAddr`, `Duration`, `String`,
//! integers) is already a std type in vault and is kept as is.

pub mod server {
    //! `vault-server`'s `Config` and every type it reaches.

    use serde::{Deserialize, Deserializer};
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use std::time::Duration;

    #[derive(Debug, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Config {
        pub listener: ListenerConfig,
        pub storage: StorageConfig,
        #[serde(default)]
        pub anchor: Option<AnchorConfig>,
        #[serde(default)]
        pub audit: Vec<AuditConfig>,
        #[serde(default)]
        pub allow_unaudited: bool,
        pub operators: Vec<String>,
        #[serde(default = "yes")]
        pub mlock: bool,
        #[serde(default)]
        pub require_encrypted_volume: bool,
        #[serde(default)]
        pub encrypted_volume_uuid: Option<String>,
        #[serde(default = "default_log_level")]
        pub log_level: String,
        #[serde(default)]
        pub outbound: Vec<OutboundConfig>,
        #[serde(default)]
        pub public: Option<PublicConfig>,
        #[serde(default)]
        pub metrics: Option<MetricsConfig>,
        #[serde(default)]
        pub ui: Option<UiConfig>,
        #[serde(default)]
        pub unix: Option<UnixConfig>,
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct UnixConfig {
        pub path: PathBuf,
        #[serde(default = "default_unix_mode", deserialize_with = "unix_mode")]
        pub mode: u32,
    }

    fn default_unix_mode() -> u32 {
        0o660
    }

    fn unix_mode<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
        match String::deserialize(d)?.as_str() {
            "0660" => Ok(0o660),
            "0600" => Ok(0o600),
            m => Err(serde::de::Error::custom(format!(
                "unix.mode {m:?}: use \"0660\" or \"0600\""
            ))),
        }
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct UiConfig {
        pub dir: PathBuf,
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct MetricsConfig {
        pub scrapers: Vec<String>,
        #[serde(default = "default_metrics_refresh", deserialize_with = "duration")]
        pub refresh: Duration,
    }

    fn default_metrics_refresh() -> Duration {
        Duration::from_secs(600)
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct PublicConfig {
        pub address: SocketAddr,
        pub mounts: Vec<String>,
        #[serde(default = "default_public_max_connections")]
        pub max_connections: usize,
        #[serde(default = "default_public_max_connections_per_ip")]
        pub max_connections_per_ip: usize,
        #[serde(default = "default_public_max_in_flight")]
        pub max_in_flight: usize,
        #[serde(default = "default_public_max_request_size")]
        pub max_request_size: usize,
        #[serde(
            default = "default_public_header_read_timeout",
            deserialize_with = "duration"
        )]
        pub header_read_timeout: Duration,
        #[serde(
            default = "default_public_body_read_timeout",
            deserialize_with = "duration"
        )]
        pub body_read_timeout: Duration,
        #[serde(default = "default_public_idle_timeout", deserialize_with = "duration")]
        pub idle_timeout: Duration,
        #[serde(
            default = "default_public_max_connection_age",
            deserialize_with = "duration"
        )]
        pub max_connection_age: Duration,
        #[serde(default)]
        pub rate_limit: RateLimitConfig,
        #[serde(default)]
        pub acme: Option<AcmeConfig>,
        #[serde(default)]
        pub device_enrollment: bool,
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct AcmeConfig {
        pub address: SocketAddr,
        pub base_url: String,
        pub tls_cert_file: PathBuf,
        pub tls_key_file: PathBuf,
        #[serde(default = "default_acme_max_request_size")]
        pub max_request_size: usize,
        #[serde(default)]
        pub validation: Option<AcmeValidationConfig>,
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct AcmeValidationConfig {
        pub hosts: Vec<String>,
        pub networks: Vec<String>,
        #[serde(default = "default_http01_port")]
        pub http_port: u16,
        #[serde(default = "default_tls_alpn01_port")]
        pub tls_port: u16,
        #[serde(default)]
        pub dns_resolvers: Vec<SocketAddr>,
    }

    fn default_acme_max_request_size() -> usize {
        16 * 1024
    }
    fn default_http01_port() -> u16 {
        80
    }
    fn default_tls_alpn01_port() -> u16 {
        443
    }
    fn default_public_max_connections() -> usize {
        256
    }
    fn default_public_max_connections_per_ip() -> usize {
        16
    }
    fn default_public_max_in_flight() -> usize {
        32
    }
    fn default_public_max_request_size() -> usize {
        4096
    }
    fn default_public_header_read_timeout() -> Duration {
        Duration::from_secs(5)
    }
    fn default_public_body_read_timeout() -> Duration {
        Duration::from_secs(5)
    }
    fn default_public_idle_timeout() -> Duration {
        Duration::from_secs(15)
    }
    fn default_public_max_connection_age() -> Duration {
        Duration::from_secs(300)
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct OutboundConfig {
        pub url: String,
        pub ca_file: PathBuf,
        #[serde(rename = "use", default, deserialize_with = "some_uses")]
        pub uses: Vec<OutboundUse>,
        #[serde(default)]
        pub client_cert_file: Option<PathBuf>,
        #[serde(default)]
        pub client_key_file: Option<PathBuf>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum OutboundUse {
        Jwt,
        Webhooks,
        Connections,
        Audit,
    }

    fn some_uses<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<OutboundUse>, D::Error> {
        let uses = Vec::<OutboundUse>::deserialize(d)?;
        if uses.is_empty() {
            return Err(serde::de::Error::custom(
                "use must name at least one use (or be left out for the default)",
            ));
        }
        Ok(uses)
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ListenerConfig {
        pub address: SocketAddr,
        pub tls_cert_file: PathBuf,
        pub tls_key_file: PathBuf,
        pub client_ca_file: PathBuf,
        #[serde(default)]
        pub client_crl_files: Vec<PathBuf>,
        #[serde(default)]
        pub operator_ca_file: Option<PathBuf>,
        #[serde(default)]
        pub device_mounts: Vec<String>,
        #[serde(default = "default_max_request_size")]
        pub max_request_size: usize,
        #[serde(default = "default_max_buffered_bytes")]
        pub max_buffered_bytes: usize,
        #[serde(default = "default_max_connections")]
        pub max_connections: usize,
        #[serde(default = "default_max_connections_per_client")]
        pub max_connections_per_client: usize,
        #[serde(default = "default_max_handshakes")]
        pub max_handshakes: usize,
        #[serde(default = "default_max_handshakes_per_ip")]
        pub max_handshakes_per_ip: usize,
        #[serde(default = "default_max_in_flight")]
        pub max_in_flight: usize,
        #[serde(default = "default_handshake_timeout", deserialize_with = "duration")]
        pub handshake_timeout: Duration,
        #[serde(default = "default_header_read_timeout", deserialize_with = "duration")]
        pub header_read_timeout: Duration,
        #[serde(default = "default_body_read_timeout", deserialize_with = "duration")]
        pub body_read_timeout: Duration,
        #[serde(default = "default_idle_timeout", deserialize_with = "duration")]
        pub idle_timeout: Duration,
        #[serde(default = "default_max_connection_age", deserialize_with = "duration")]
        pub max_connection_age: Duration,
        #[serde(default)]
        pub rate_limit: RateLimitConfig,
        #[serde(default = "default_tls_reload_interval", deserialize_with = "duration")]
        pub tls_reload_interval: Duration,
        #[serde(default)]
        pub tls_server_names: Vec<String>,
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct RateLimitConfig {
        pub requests_per_second: u32,
        pub burst: u32,
    }

    impl Default for RateLimitConfig {
        fn default() -> Self {
            Self {
                requests_per_second: 50,
                burst: 100,
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
    pub enum StorageConfig {
        File { path: PathBuf },
        Postgres(PostgresStorage),
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct PostgresStorage {
        pub connection: String,
        #[serde(default)]
        pub password_file: Option<PathBuf>,
        #[serde(default)]
        pub ca_file: Option<PathBuf>,
        #[serde(default = "default_pg_max_connections")]
        pub max_connections: usize,
        #[serde(default = "default_pg_timeout", deserialize_with = "duration")]
        pub timeout: Duration,
        #[serde(default = "yes")]
        pub create_table: bool,
        #[serde(default = "default_pg_table")]
        pub table: String,
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
    pub enum AnchorConfig {
        File { path: PathBuf },
        Postgres(PostgresAnchorSettings),
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct PostgresAnchorSettings {
        pub connection: String,
        #[serde(default)]
        pub password_file: Option<PathBuf>,
        #[serde(default)]
        pub ca_file: Option<PathBuf>,
        #[serde(default = "default_pg_timeout", deserialize_with = "duration")]
        pub timeout: Duration,
        #[serde(default = "default_anchor_table")]
        pub table: String,
        #[serde(default = "default_anchor_name")]
        pub name: String,
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
    pub enum AuditConfig {
        File { path: PathBuf },
    }

    fn yes() -> bool {
        true
    }
    fn default_log_level() -> String {
        "info".into()
    }
    const DEFAULT_MAX_REQUEST_SIZE: usize = 1024 * 1024;
    fn default_max_request_size() -> usize {
        DEFAULT_MAX_REQUEST_SIZE
    }
    fn default_max_buffered_bytes() -> usize {
        64 * 1024 * 1024
    }
    fn default_max_connections() -> usize {
        1024
    }
    fn default_max_connections_per_client() -> usize {
        64
    }
    fn default_max_handshakes() -> usize {
        256
    }
    fn default_max_handshakes_per_ip() -> usize {
        16
    }
    fn default_max_in_flight() -> usize {
        256
    }
    fn default_handshake_timeout() -> Duration {
        Duration::from_secs(5)
    }
    fn default_header_read_timeout() -> Duration {
        Duration::from_secs(10)
    }
    fn default_body_read_timeout() -> Duration {
        Duration::from_secs(30)
    }
    fn default_idle_timeout() -> Duration {
        Duration::from_secs(300)
    }
    fn default_tls_reload_interval() -> Duration {
        Duration::from_secs(30)
    }
    fn default_max_connection_age() -> Duration {
        Duration::from_secs(3600)
    }
    fn default_pg_max_connections() -> usize {
        8
    }
    fn default_pg_timeout() -> Duration {
        Duration::from_secs(10)
    }
    fn default_pg_table() -> String {
        "vault_kv".into()
    }
    fn default_anchor_table() -> String {
        "public.vault_anchor".into()
    }
    fn default_anchor_name() -> String {
        "vault".into()
    }

    /// Durations are written like the API's: "90", "30s", "15m", "1h".
    fn duration<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let s = String::deserialize(d)?;
        // SUBST: `vault_core::logical::parse_duration`, copied below.
        let secs = super::parse_duration(&s).map_err(serde::de::Error::custom)?;
        #[allow(clippy::cast_sign_loss)] // as in vault: `secs` is never negative
        Ok(Duration::from_secs(secs as u64))
    }
}

pub mod import {
    //! `vault-import`'s `Config` and every type it reaches.

    use serde::Deserialize;
    use std::path::PathBuf;

    pub const DEFAULT_MAX_ENTRIES: usize = 100_000;

    #[derive(Debug, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Config {
        #[serde(default = "yes")]
        pub mlock: bool,
        pub source: Source,
        pub target: Target,
        #[serde(default)]
        pub kv: Vec<KvTable>,
        #[serde(default)]
        pub secrets: Vec<SecretsTable>,
    }

    #[derive(Debug, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Source {
        pub address: String,
        pub ca_file: PathBuf,
        pub token_file: PathBuf,
        pub namespace: Option<String>,
        pub cert_file: Option<PathBuf>,
        pub key_file: Option<PathBuf>,
    }

    #[derive(Debug, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Target {
        pub address: String,
        pub ca_file: PathBuf,
        pub cert_file: PathBuf,
        pub key_file: PathBuf,
        pub token_file: PathBuf,
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct KvTable {
        pub source: String,
        pub target: String,
        #[serde(default)]
        pub source_prefix: String,
        #[serde(default)]
        pub target_prefix: String,
        #[serde(default)]
        pub on_conflict: OnConflict,
        #[serde(default = "default_max_entries")]
        pub max_entries: usize,
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct SecretsTable {
        pub source: String,
        #[serde(default)]
        pub source_prefix: String,
        pub target: String,
        pub environment: String,
        #[serde(default)]
        pub folder: String,
        #[serde(default)]
        pub on_conflict: OnConflict,
        #[serde(default)]
        pub non_string: NonString,
        #[serde(default)]
        pub custom_metadata: CustomMetadata,
        #[serde(default = "default_max_entries")]
        pub max_entries: usize,
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum CustomMetadata {
        #[default]
        Skip,
        Copy,
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum NonString {
        #[default]
        Fail,
        Json,
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum OnConflict {
        #[default]
        Fail,
        NewVersion,
    }

    fn yes() -> bool {
        true
    }

    fn default_max_entries() -> usize {
        DEFAULT_MAX_ENTRIES
    }
}

/// Upper bound for any duration input (vault-core `MAX_DURATION_SECS`).
const MAX_DURATION_SECS: i64 = 100 * 365 * 86400;

/// Parse "90", "30s", "15m", "24h", "7d" into seconds.
///
/// SUBST: copied from `vault_core::logical::parse_duration`; vault-core's
/// `Error::invalid(..)` is replaced by a `String` with the same message.
fn parse_duration(s: &str) -> Result<i64, String> {
    let s = s.trim();
    let err = || format!("invalid duration {s:?}");
    let (num, mult) = match s.chars().last().ok_or_else(err)? {
        's' => (&s[..s.len() - 1], 1),
        'm' => (&s[..s.len() - 1], 60),
        'h' => (&s[..s.len() - 1], 3600),
        'd' => (&s[..s.len() - 1], 86400),
        c if c.is_ascii_digit() => (s, 1),
        _ => return Err(err()),
    };
    let n: i64 = num.parse().map_err(|_| err())?;
    if n < 0 {
        return Err(err());
    }
    let secs = n.checked_mul(mult).ok_or_else(err)?;
    if secs > MAX_DURATION_SECS {
        return Err(format!("duration {s:?} exceeds 100 years"));
    }
    Ok(secs)
}
