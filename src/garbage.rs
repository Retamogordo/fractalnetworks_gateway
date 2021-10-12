use anyhow::Result;
use log::*;
use sqlx::{query, SqlitePool};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

pub const GARBAGE_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
pub const GARBAGE_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Garbage collector. This runs in a configurable interval (by default, once
/// per hour) and runs garbage_collect().
pub async fn garbage(pool: &SqlitePool, duration: Duration) -> Result<()> {
    info!("Launching garbage collector every {}s", duration.as_secs());
    let mut interval = tokio::time::interval(duration);
    loop {
        interval.tick().await;
        garbage_collect(&pool).await?;
    }
}

/// Deletes all traffic items in the database that are older than
/// TRAFFIC_RETENTION, and finally performs a VACUUM on the database to ensure
/// it is as compact as possible. Without this, the database file would keep
/// growing in size.
pub async fn garbage_collect(pool: &SqlitePool) -> Result<()> {
    info!("Running garbage collection");
    let time = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let cutoff = time - GARBAGE_RETENTION;
    let result = query("DELETE FROM gateway_traffic WHERE time < ?")
        .bind(cutoff.as_secs() as i64)
        .execute(pool)
        .await?;
    info!("Removed {} traffic data lines", result.rows_affected());

    let result = query(
        "DELETE FROM gateway_network
            WHERE NOT EXISTS (
                SELECT 1 FROM gateway_traffic
                WHERE gateway_traffic.network_id = gateway_network.network_id)",
    )
    .bind(cutoff.as_secs() as i64)
    .execute(pool)
    .await?;
    info!("Removed {} network pubkeys", result.rows_affected());

    let result = query(
        "DELETE FROM gateway_device
            WHERE NOT EXISTS (
                SELECT 1 FROM gateway_traffic
                WHERE gateway_traffic.device_id = gateway_device.device_id)",
    )
    .bind(cutoff.as_secs() as i64)
    .execute(pool)
    .await?;
    info!("Removed {} device pubkeys", result.rows_affected());

    query("VACUUM").execute(pool).await?;
    info!("Completed database vacuum");
    Ok(())
}
