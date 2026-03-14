use tracing::trace;

const SYSFS_BASE: &str = "/sys/class/net/draytek0/statistics";

#[derive(Debug, Clone, Default)]
pub struct NetStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

/// Read network statistics from sysfs. Returns None if the interface doesn't exist.
pub async fn read_stats() -> Option<NetStats> {
    let rx_bytes = read_stat("rx_bytes").await?;
    let tx_bytes = read_stat("tx_bytes").await?;
    let rx_packets = read_stat("rx_packets").await?;
    let tx_packets = read_stat("tx_packets").await?;

    let stats = NetStats {
        rx_bytes,
        tx_bytes,
        rx_packets,
        tx_packets,
    };
    trace!(?stats, "read network stats");
    Some(stats)
}

async fn read_stat(name: &str) -> Option<u64> {
    let path = format!("{}/{}", SYSFS_BASE, name);
    let content = tokio::fs::read_to_string(&path).await.ok()?;
    content.trim().parse().ok()
}
