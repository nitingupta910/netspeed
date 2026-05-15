use anyhow::{Context, Result, bail};
use std::io::{self, Read};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct SpeedTestResult {
    pub download_mbps: f64,
    pub upload_mbps: f64,
}

#[derive(Debug, Clone)]
pub enum SpeedTestProgress {
    Downloading(f64),
    Uploading(f64),
    Done(SpeedTestResult),
    Error(String),
}

const TEST_DURATION: Duration = Duration::from_secs(10);
const DOWNLOAD_CONCURRENCY: usize = 4;
const UPLOAD_CONCURRENCY: usize = 2;
pub(crate) const MIN_TRANSFER_BYTES: u64 = 1_000_000;
pub(crate) const DOWNLOAD_TRANSFER_SIZES: [u64; 3] = [MIN_TRANSFER_BYTES, 2_000_000, 5_000_000];
pub(crate) const UPLOAD_TRANSFER_SIZES: [u64; 3] = [MIN_TRANSFER_BYTES, 5_000_000, 10_000_000];
const CLOUDFLARE_DOWNLOAD_BASE_URL: &str = "https://speed.cloudflare.com/__down";
const CLOUDFLARE_UPLOAD_URL: &str = "https://speed.cloudflare.com/__up";

pub(crate) fn download_url(bytes: u64) -> String {
    format!("{CLOUDFLARE_DOWNLOAD_BASE_URL}?bytes={bytes}")
}

pub(crate) fn cloudflare_status_error(
    direction: &str,
    status: reqwest::StatusCode,
    retry_after: Option<&str>,
) -> String {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry = retry_after
            .map(|value| format!(" Try again after {value} seconds."))
            .unwrap_or_default();
        return format!(
            "Cloudflare speed test rate limit hit during {direction} (HTTP 429).{retry}"
        );
    }

    format!("Cloudflare {direction} speed test failed with HTTP {status}")
}

pub async fn run_speed_test(tx: mpsc::Sender<SpeedTestProgress>) {
    if let Err(e) = do_test(&tx).await {
        let _ = tx.send(SpeedTestProgress::Error(e.to_string())).await;
    }
}

async fn do_test(tx: &mpsc::Sender<SpeedTestProgress>) -> Result<()> {
    let download_mbps = test_download(tx).await?;
    let upload_mbps = test_upload(tx).await?;
    let _ = tx
        .send(SpeedTestProgress::Done(SpeedTestResult {
            download_mbps,
            upload_mbps,
        }))
        .await;
    Ok(())
}

/// Convert a byte count and elapsed time into Mbps.
pub(crate) fn bytes_to_mbps(bytes: u64, elapsed_secs: f64) -> f64 {
    (bytes as f64 * 8.0) / (elapsed_secs * 1_000_000.0)
}

// ── Download ─────────────────────────────────────────────────────────────────
//
// reqwest's async streaming is too slow at high speeds (>100 Mbps): every
// stream.next().await yield goes through tokio's scheduler, which at 650 Mbps
// means millions of round-trips per second.  Instead we run the download in a
// spawn_blocking thread using reqwest::blocking + std::io::Read, letting the
// kernel handle TCP ACKs and buffering with no async overhead.
//
// The AtomicU64 counter is shared between the blocking thread and an async
// progress reporter that fires every 300 ms on the tokio executor.

async fn test_download(tx: &mpsc::Sender<SpeedTestProgress>) -> Result<f64> {
    let _ = tx.send(SpeedTestProgress::Downloading(0.0)).await;

    let bytes_recvd = Arc::new(AtomicU64::new(0));
    let start = Instant::now();

    let progress_tx = tx.clone();
    let progress_counter = bytes_recvd.clone();
    let progress_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let b = progress_counter.load(Ordering::Relaxed);
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed > 0.0 && b > 0 {
                let mbps = bytes_to_mbps(b, elapsed);
                if progress_tx
                    .send(SpeedTestProgress::Downloading(mbps))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    });

    let mut handles = Vec::with_capacity(DOWNLOAD_CONCURRENCY);
    for _ in 0..DOWNLOAD_CONCURRENCY {
        let counter = bytes_recvd.clone();
        handles.push(tokio::task::spawn_blocking(move || {
            run_download_worker(start, counter)
        }));
    }

    let mut successful_workers = 0usize;
    let mut first_error = None;
    for handle in handles {
        match handle.await? {
            Ok(()) => successful_workers += 1,
            Err(e) if first_error.is_none() => first_error = Some(e),
            Err(_) => {}
        }
    }

    progress_handle.abort();

    if successful_workers == 0 {
        if let Some(e) = first_error {
            return Err(e);
        }
    }

    let total = bytes_recvd.load(Ordering::Relaxed);
    let elapsed = start.elapsed().as_secs_f64().max(0.001);
    Ok(bytes_to_mbps(total, elapsed))
}

fn run_download_worker(start: Instant, counter: Arc<AtomicU64>) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()?;

    let mut buf = vec![0u8; 256 * 1024];
    let mut size_idx = DOWNLOAD_TRANSFER_SIZES.len() - 1;

    while start.elapsed() < TEST_DURATION {
        let request_bytes = DOWNLOAD_TRANSFER_SIZES[size_idx];
        let mut resp = client
            .get(download_url(request_bytes))
            .send()
            .with_context(|| format!("download request failed for {request_bytes} bytes"))?;

        let status = resp.status();
        if !status.is_success() {
            if size_idx > 0 {
                size_idx -= 1;
                continue;
            }

            bail!(cloudflare_status_error(
                "download",
                status,
                resp.headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
            ));
        }

        loop {
            let n = resp.read(&mut buf)?;
            if n == 0 {
                break;
            }
            counter.fetch_add(n as u64, Ordering::Relaxed);
            if start.elapsed() >= TEST_DURATION {
                return Ok(());
            }
        }
    }

    Ok(())
}

// ── Upload ───────────────────────────────────────────────────────────────────

struct ZeroReader {
    remaining: u64,
    counter: Arc<AtomicU64>,
}

impl ZeroReader {
    fn new(bytes: u64, counter: Arc<AtomicU64>) -> Self {
        Self {
            remaining: bytes,
            counter,
        }
    }
}

impl Read for ZeroReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }

        let n = buf.len().min(self.remaining as usize);
        buf[..n].fill(0);
        self.remaining -= n as u64;
        self.counter.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

async fn test_upload(tx: &mpsc::Sender<SpeedTestProgress>) -> Result<f64> {
    let _ = tx.send(SpeedTestProgress::Uploading(0.0)).await;

    let start = Instant::now();
    let bytes_sent = Arc::new(AtomicU64::new(0));

    let progress_tx = tx.clone();
    let progress_counter = bytes_sent.clone();
    let progress_handle = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let b = progress_counter.load(Ordering::Relaxed);
            let elapsed = start.elapsed().as_secs_f64();
            if elapsed > 0.0 && b > 0 {
                let mbps = bytes_to_mbps(b, elapsed);
                if progress_tx
                    .send(SpeedTestProgress::Uploading(mbps))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    });

    let mut handles = Vec::with_capacity(UPLOAD_CONCURRENCY);
    for _ in 0..UPLOAD_CONCURRENCY {
        let counter = bytes_sent.clone();
        handles.push(tokio::task::spawn_blocking(move || {
            run_upload_worker(start, counter)
        }));
    }

    let mut successful_workers = 0usize;
    let mut first_error = None;
    for handle in handles {
        match handle.await? {
            Ok(()) => successful_workers += 1,
            Err(e) if first_error.is_none() => first_error = Some(e),
            Err(_) => {}
        }
    }

    progress_handle.abort();

    if successful_workers == 0 {
        if let Some(e) = first_error {
            return Err(e);
        }
    }

    let total = bytes_sent.load(Ordering::Relaxed);
    let elapsed = start.elapsed().as_secs_f64().max(0.001);
    let mbps = bytes_to_mbps(total, elapsed);
    let _ = tx.send(SpeedTestProgress::Uploading(mbps)).await;
    Ok(mbps)
}

fn run_upload_worker(start: Instant, counter: Arc<AtomicU64>) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()?;

    let mut size_idx = UPLOAD_TRANSFER_SIZES.len() - 1;

    while start.elapsed() < TEST_DURATION {
        let request_bytes = UPLOAD_TRANSFER_SIZES[size_idx];
        let body = reqwest::blocking::Body::sized(
            ZeroReader::new(request_bytes, counter.clone()),
            request_bytes,
        );
        let resp = client
            .post(CLOUDFLARE_UPLOAD_URL)
            .body(body)
            .send()
            .with_context(|| format!("upload request failed for {request_bytes} bytes"))?;

        let status = resp.status();
        if !status.is_success() {
            if size_idx > 0 {
                size_idx -= 1;
                continue;
            }

            bail!(cloudflare_status_error(
                "upload",
                status,
                resp.headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
            ));
        }
    }

    Ok(())
}
