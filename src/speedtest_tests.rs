use crate::speedtest::{
    DOWNLOAD_TRANSFER_SIZES, MIN_TRANSFER_BYTES, SpeedTestProgress, SpeedTestResult,
    UPLOAD_TRANSFER_SIZES, bytes_to_mbps, cloudflare_status_error, download_url,
};

// ── bytes_to_mbps ─────────────────────────────────────────────────────────────

#[test]
fn bytes_to_mbps_one_megabyte_per_second_is_8_mbps() {
    // 1 000 000 bytes / 1 s → 8 Mbps
    assert!((bytes_to_mbps(1_000_000, 1.0) - 8.0).abs() < 0.001);
}

#[test]
fn bytes_to_mbps_100_mbps_reference() {
    // 125 MB (125 000 000 bytes) in 10 s → 100 Mbps
    assert!((bytes_to_mbps(125_000_000, 10.0) - 100.0).abs() < 0.001);
}

#[test]
fn bytes_to_mbps_1_gbps_reference() {
    // 1 250 000 000 bytes in 10 s → 1 000 Mbps
    assert!((bytes_to_mbps(1_250_000_000, 10.0) - 1_000.0).abs() < 0.001);
}

#[test]
fn bytes_to_mbps_scales_linearly_with_bytes() {
    let base = bytes_to_mbps(1_000_000, 1.0);
    let double = bytes_to_mbps(2_000_000, 1.0);
    assert!((double - 2.0 * base).abs() < 0.001);
}

#[test]
fn bytes_to_mbps_scales_inversely_with_time() {
    let fast = bytes_to_mbps(1_000_000, 1.0);
    let slow = bytes_to_mbps(1_000_000, 2.0);
    assert!((fast - 2.0 * slow).abs() < 0.001);
}

#[test]
fn bytes_to_mbps_zero_bytes_gives_zero() {
    assert_eq!(bytes_to_mbps(0, 1.0), 0.0);
}

// ── download_url ─────────────────────────────────────────────────────────────

#[test]
fn download_url_uses_requested_payload_size() {
    assert_eq!(
        download_url(MIN_TRANSFER_BYTES),
        "https://speed.cloudflare.com/__down?bytes=1000000"
    );
}

#[test]
fn download_transfer_sizes_start_small_and_stop_before_cloudflare_limit() {
    assert_eq!(DOWNLOAD_TRANSFER_SIZES[0], MIN_TRANSFER_BYTES);
    assert_eq!(
        DOWNLOAD_TRANSFER_SIZES[DOWNLOAD_TRANSFER_SIZES.len() - 1],
        5_000_000
    );
}

#[test]
fn upload_transfer_sizes_start_small_and_ramp_up() {
    assert_eq!(UPLOAD_TRANSFER_SIZES[0], MIN_TRANSFER_BYTES);
    assert_eq!(
        UPLOAD_TRANSFER_SIZES[UPLOAD_TRANSFER_SIZES.len() - 1],
        10_000_000
    );
}

#[test]
fn cloudflare_rate_limit_error_is_explicit() {
    let msg = cloudflare_status_error(
        "download",
        reqwest::StatusCode::TOO_MANY_REQUESTS,
        Some("60"),
    );
    assert!(msg.contains("rate limit"));
    assert!(msg.contains("HTTP 429"));
    assert!(msg.contains("60"));
}

#[test]
fn cloudflare_status_error_names_direction() {
    let msg = cloudflare_status_error("upload", reqwest::StatusCode::BAD_REQUEST, None);
    assert!(msg.contains("upload"));
    assert!(msg.contains("HTTP 400"));
}

// ── SpeedTestResult ───────────────────────────────────────────────────────────

#[test]
fn speed_test_result_fields_accessible() {
    let r = SpeedTestResult {
        download_mbps: 123.4,
        upload_mbps: 56.7,
    };
    assert_eq!(r.download_mbps, 123.4);
    assert_eq!(r.upload_mbps, 56.7);
}

#[test]
fn speed_test_result_is_cloneable() {
    let r = SpeedTestResult {
        download_mbps: 100.0,
        upload_mbps: 50.0,
    };
    let r2 = r.clone();
    assert_eq!(r2.download_mbps, r.download_mbps);
    assert_eq!(r2.upload_mbps, r.upload_mbps);
}

// ── SpeedTestProgress ─────────────────────────────────────────────────────────

#[test]
fn progress_downloading_holds_speed() {
    if let SpeedTestProgress::Downloading(mbps) = SpeedTestProgress::Downloading(75.0) {
        assert_eq!(mbps, 75.0);
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn progress_uploading_holds_speed() {
    if let SpeedTestProgress::Uploading(mbps) = SpeedTestProgress::Uploading(30.0) {
        assert_eq!(mbps, 30.0);
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn progress_done_holds_result() {
    let result = SpeedTestResult {
        download_mbps: 200.0,
        upload_mbps: 40.0,
    };
    if let SpeedTestProgress::Done(r) = SpeedTestProgress::Done(result) {
        assert_eq!(r.download_mbps, 200.0);
        assert_eq!(r.upload_mbps, 40.0);
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn progress_error_holds_message() {
    if let SpeedTestProgress::Error(msg) = SpeedTestProgress::Error("timeout".into()) {
        assert_eq!(msg, "timeout");
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn progress_is_cloneable() {
    let p = SpeedTestProgress::Downloading(50.0);
    let _p2 = p.clone();
}
