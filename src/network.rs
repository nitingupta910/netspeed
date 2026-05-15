use anyhow::{Context, Result};
use std::fs;

#[derive(Debug, Clone)]
pub struct InterfaceStats {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// Find the interface that handles the default route via /proc/net/route.
pub fn get_default_interface() -> Result<String> {
    let content = fs::read_to_string("/proc/net/route")
        .context("Failed to read /proc/net/route")?;

    if let Some(iface) = default_iface_from_route(&content) {
        return Ok(iface);
    }

    // Fallback: first non-loopback interface found in /proc/net/dev
    list_interfaces()?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No usable network interface found"))
}

pub fn get_interface_stats(interface: &str) -> Result<InterfaceStats> {
    parse_dev()?.into_iter()
        .find(|s| s.name == interface)
        .ok_or_else(|| anyhow::anyhow!("Interface '{}' not found in /proc/net/dev", interface))
}

pub fn list_interfaces() -> Result<Vec<String>> {
    Ok(parse_dev()?.into_iter()
        .map(|s| s.name)
        .filter(|n| n != "lo")
        .collect())
}

fn parse_dev() -> Result<Vec<InterfaceStats>> {
    let content = fs::read_to_string("/proc/net/dev")
        .context("Failed to read /proc/net/dev")?;
    Ok(parse_dev_str(&content))
}

/// Parse the text content of `/proc/net/dev`.  Exposed for testing.
///
/// Format: 2 header lines, then one line per interface:
///   <iface>: rx_bytes rx_pkts … (8 rx fields) tx_bytes tx_pkts …
pub(crate) fn parse_dev_str(content: &str) -> Vec<InterfaceStats> {
    let mut stats = Vec::new();
    for line in content.lines().skip(2) {
        let trimmed = line.trim();
        if let Some((name, data)) = trimmed.split_once(':') {
            let fields: Vec<u64> = data
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();
            if fields.len() >= 9 {
                stats.push(InterfaceStats {
                    name: name.trim().to_string(),
                    rx_bytes: fields[0],
                    tx_bytes: fields[8],
                });
            }
        }
    }
    stats
}

/// Find the default-route interface inside `/proc/net/route` text.
/// Returns `None` when no `00000000` (0.0.0.0) destination is found.
/// Exposed for testing.
pub(crate) fn default_iface_from_route(content: &str) -> Option<String> {
    for line in content.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // column [1] is destination; "00000000" = 0.0.0.0 = default route
        if fields.len() >= 2 && fields[1] == "00000000" {
            return Some(fields[0].to_string());
        }
    }
    None
}

pub fn format_speed(mbps: f64) -> String {
    if mbps < 0.001 {
        "0 bps".to_string()
    } else if mbps < 1.0 {
        format!("{:.1} Kbps", mbps * 1_000.0)
    } else if mbps >= 1_000.0 {
        format!("{:.2} Gbps", mbps / 1_000.0)
    } else {
        format!("{:.2} Mbps", mbps)
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;

    if bytes < KB {
        format!("{} B", bytes)
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else if bytes < GB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    }
}
