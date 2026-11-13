//! Database migration entrypoint for container jobs.
//!
//! Helm hooks and self-hosted Compose one-shots share `g5_api::db`'s
//! cluster-role coordination lock. Calling raw `sqlx migrate run` bypasses
//! that guard.

use std::env;
use std::io;
use std::path::Path;
use std::time::Duration;

use g5_api::config::DatabaseConfig;
use g5_api::db;
use g5_contracts::tracing_compat::{
    default_ansi_enabled, legacy_compatible_env_filter, LegacyTargetFormatter,
};
use tracing_subscriber::fmt;

const DEFAULT_CONTAINER_MIGRATION_SOURCE: &str = "/app/migrations";
const WORKSPACE_MIGRATION_SOURCE: &str = "g5-api/src/db/migrations";
const PACKAGE_MIGRATION_SOURCE: &str = "src/db/migrations";

#[tokio::main]
async fn main() {
    init_logging();

    if let Err(error) = run().await {
        eprintln!("[g5-db-migrate] migration failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = env::var("DATABASE_URL")
        .or_else(|_| env::var("G5_DATABASE_URL"))
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "DATABASE_URL or G5_DATABASE_URL is required",
            )
        })?;

    let database = DatabaseConfig {
        url: database_url,
        max_connections: parse_u32_env("G5_MIGRATION_DATABASE_MAX_CONNECTIONS", 5),
        min_connections: parse_u32_env("G5_MIGRATION_DATABASE_MIN_CONNECTIONS", 0),
        connect_timeout: Duration::from_secs(parse_u64_env(
            "G5_MIGRATION_DATABASE_CONNECT_TIMEOUT_SECS",
            30,
        )),
        idle_timeout: Duration::from_secs(parse_u64_env(
            "G5_MIGRATION_DATABASE_IDLE_TIMEOUT_SECS",
            600,
        )),
    };

    if let Some(command) = env::args().nth(1) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown g5-db-migrate command {command:?}"),
        )
        .into());
    }

    let pool = db::init_pool(&database).await?;

    let migration_source = migration_source();
    let migration_source_path = Path::new(&migration_source);
    eprintln!(
        "[g5-db-migrate] running migrations from {}",
        migration_source_path.display()
    );

    db::run_migrations_from_path(&pool, migration_source_path).await?;
    pool.close().await;

    eprintln!("[g5-db-migrate] migrations complete");
    Ok(())
}

fn init_logging() {
    let env_filter = legacy_compatible_env_filter("info", g5_api::LEGACY_TRACING_TARGET_FAMILIES);
    let ansi = default_ansi_enabled();
    fmt()
        .with_ansi(ansi)
        .event_format(LegacyTargetFormatter::new(
            tracing_subscriber::fmt::format().with_ansi(ansi),
            g5_api::LEGACY_TRACING_TARGET_FAMILIES,
        ))
        .with_env_filter(env_filter)
        .init();
}

fn migration_source() -> String {
    env::var("G5_MIGRATION_SOURCE").unwrap_or_else(|_| {
        if Path::new(DEFAULT_CONTAINER_MIGRATION_SOURCE).is_dir() {
            DEFAULT_CONTAINER_MIGRATION_SOURCE.to_string()
        } else if Path::new(WORKSPACE_MIGRATION_SOURCE).is_dir() {
            WORKSPACE_MIGRATION_SOURCE.to_string()
        } else {
            PACKAGE_MIGRATION_SOURCE.to_string()
        }
    })
}

fn parse_u32_env(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn parse_u64_env(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}
