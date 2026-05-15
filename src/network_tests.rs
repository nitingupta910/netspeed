use crate::network::{
    default_iface_from_route, format_bytes, format_speed, parse_dev_str,
};

// ── format_speed ─────────────────────────────────────────────────────────────

#[test]
fn format_speed_zero_and_sub_threshold() {
    assert_eq!(format_speed(0.0), "0 bps");
    assert_eq!(format_speed(0.0009), "0 bps");
    // exactly at the threshold is already Kbps
    assert_ne!(format_speed(0.001), "0 bps");
}

#[test]
fn format_speed_kbps_range() {
    assert_eq!(format_speed(0.001), "1.0 Kbps");
    assert_eq!(format_speed(0.5), "500.0 Kbps");
    assert_eq!(format_speed(0.9999), "999.9 Kbps");
}

#[test]
fn format_speed_mbps_range() {
    assert_eq!(format_speed(1.0), "1.00 Mbps");
    assert_eq!(format_speed(50.0), "50.00 Mbps");
    assert_eq!(format_speed(999.99), "999.99 Mbps");
}

#[test]
fn format_speed_gbps_range() {
    assert_eq!(format_speed(1000.0), "1.00 Gbps");
    assert_eq!(format_speed(10_000.0), "10.00 Gbps");
    assert_eq!(format_speed(1_234.5), "1.23 Gbps");
}

// ── format_bytes ─────────────────────────────────────────────────────────────

#[test]
fn format_bytes_sub_kb() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(1), "1 B");
    assert_eq!(format_bytes(1_023), "1023 B");
}

#[test]
fn format_bytes_kb_range() {
    assert_eq!(format_bytes(1_024), "1.0 KB");
    assert_eq!(format_bytes(1_536), "1.5 KB");
    assert_eq!(format_bytes(1_024 * 1_024 - 1), "1024.0 KB");
}

#[test]
fn format_bytes_mb_range() {
    assert_eq!(format_bytes(1_024 * 1_024), "1.00 MB");
    assert_eq!(format_bytes(1_024 * 1_024 + 512 * 1_024), "1.50 MB");
}

#[test]
fn format_bytes_gb_range() {
    assert_eq!(format_bytes(1_024 * 1_024 * 1_024), "1.00 GB");
    assert_eq!(format_bytes(2 * 1_024 * 1_024 * 1_024), "2.00 GB");
}

// ── parse_dev_str ─────────────────────────────────────────────────────────────

/// Realistic /proc/net/dev excerpt (two-line header + three interfaces).
const SAMPLE_DEV: &str = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo:  1234567      8900    0    0    0     0          0         0  1234567      8900    0    0    0     0       0          0
  eth0: 987654321   54321    0    0    0     0          0         0 123456789  12345    0    0    0     0       0          0
 wlan0:    123456    1234    0    0    0     0          0         0    654321    5678    0    0    0     0       0          0
";

#[test]
fn parse_dev_returns_all_interfaces() {
    assert_eq!(parse_dev_str(SAMPLE_DEV).len(), 3);
}

#[test]
fn parse_dev_correct_rx_tx_for_eth0() {
    let stats = parse_dev_str(SAMPLE_DEV);
    let eth0 = stats.iter().find(|s| s.name == "eth0").expect("eth0 missing");
    assert_eq!(eth0.rx_bytes, 987_654_321);
    assert_eq!(eth0.tx_bytes, 123_456_789);
}

#[test]
fn parse_dev_loopback_rx_equals_tx() {
    let stats = parse_dev_str(SAMPLE_DEV);
    let lo = stats.iter().find(|s| s.name == "lo").expect("lo missing");
    assert_eq!(lo.rx_bytes, lo.tx_bytes);
    assert_eq!(lo.rx_bytes, 1_234_567);
}

#[test]
fn parse_dev_interface_name_is_trimmed() {
    let stats = parse_dev_str(SAMPLE_DEV);
    // Names must not carry leading/trailing whitespace
    for s in &stats {
        assert_eq!(s.name.trim(), s.name);
    }
}

#[test]
fn parse_dev_empty_content_returns_empty() {
    // Only the two header lines — no data lines
    assert!(parse_dev_str("Inter-|\n face |\n").is_empty());
}

#[test]
fn parse_dev_skips_lines_with_too_few_fields() {
    let content = "\
Inter-|\n face |\n\
    lo:  garbage here that wont parse\n\
  eth0: 987654321   54321    0    0    0     0          0         0 123456789  12345    0    0    0     0       0          0\n";
    let stats = parse_dev_str(content);
    assert!(stats.iter().any(|s| s.name == "eth0"));
    assert!(!stats.iter().any(|s| s.name == "lo"));
}

#[test]
fn parse_dev_handles_colons_in_interface_names() {
    // Some kernels expose virtual interfaces with colons (e.g. "eth0:1").
    let content = "\
Inter-|\n face |\n\
eth0:1: 111111  1111    0    0    0     0          0         0  222222    2222    0    0    0     0       0          0\n";
    let stats = parse_dev_str(content);
    // The parser splits on the FIRST colon, so the name becomes "eth0".
    // Verify at least one entry was produced and rx/tx make sense.
    assert!(!stats.is_empty());
}

// ── default_iface_from_route ──────────────────────────────────────────────────

const SAMPLE_ROUTE: &str = "\
Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT
eth0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0
eth0\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0
";

#[test]
fn route_finds_default_interface() {
    assert_eq!(
        default_iface_from_route(SAMPLE_ROUTE),
        Some("eth0".to_string())
    );
}

#[test]
fn route_returns_none_when_no_default_route() {
    // Only a specific subnet route — no 0.0.0.0 destination
    let content = "Iface\tDestination\n\
                   eth0\t0001A8C0\n";
    assert_eq!(default_iface_from_route(content), None);
}

#[test]
fn route_returns_first_default_when_multiple_exist() {
    let content = "Iface\tDestination\tGateway\n\
                   wlan0\t00000000\t0101A8C0\n\
                   eth0\t00000000\t0101A8C0\n";
    assert_eq!(
        default_iface_from_route(content),
        Some("wlan0".to_string())
    );
}

#[test]
fn route_empty_content_returns_none() {
    assert_eq!(default_iface_from_route(""), None);
    assert_eq!(default_iface_from_route("Iface\tDestination\n"), None);
}
