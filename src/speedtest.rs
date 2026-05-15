use anyhow::Result;
use bytes::Bytes;
use futures_util::stream;
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
pub(crate) const DOWNLOAD_BYTES_PER_REQUEST: u64 = 50_000_000;
const CLOUDFLARE_DOWNLOAD_BASE_URL: &str = "https://speed.cloudflare.com/__down";

pub(crate) fn download_url() -> String {
    format!("{CLOUDFLARE_DOWNLOAD_BASE_URL}?bytes={DOWNLOAD_BYTES_PER_REQUEST}")
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

    let counter = bytes_recvd.clone();
    let final_mbps = tokio::task::spawn_blocking(move || -> Result<f64> {
        use std::io::Read;

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(90))
            .build()?;

        let mut buf = vec![0u8; 256 * 1024]; // 256 KB read buffer

        while start.elapsed() < TEST_DURATION {
            let mut resp = client.get(download_url()).send()?.error_for_status()?;

            loop {
                let n = resp.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                counter.fetch_add(n as u64, Ordering::Relaxed);
                if start.elapsed() >= TEST_DURATION {
                    let t = counter.load(Ordering::Relaxed);
                    let e = start.elapsed().as_secs_f64().max(0.001);
                    return Ok(bytes_to_mbps(t, e));
                }
            }
        }

        let t = counter.load(Ordering::Relaxed);
        let e = start.elapsed().as_secs_f64().max(0.001);
        Ok(bytes_to_mbps(t, e))
    })
    .await??;

    progress_handle.abort();
    Ok(final_mbps)
}

// ── Upload ───────────────────────────────────────────────────────────────────

async fn test_upload(tx: &mpsc::Sender<SpeedTestProgress>) -> Result<f64> {
    let _ = tx.send(SpeedTestProgress::Uploading(0.0)).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()?;

    let start = Instant::now();
    let bytes_sent = Arc::new(AtomicU64::new(0));

    const CHUNK: usize = 64 * 1024;
    let chunk = Bytes::from(vec![0u8; CHUNK]);

    let counter = bytes_sent.clone();
    let upload_stream = stream::unfold(
        (start, chunk, counter),
        |(start, chunk, counter)| async move {
            if start.elapsed() >= TEST_DURATION {
                return None;
            }
            counter.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            Some((
                Ok::<Bytes, std::io::Error>(chunk.clone()),
                (start, chunk, counter),
            ))
        },
    );

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

    let body = reqwest::Body::wrap_stream(upload_stream);
    let result = client
        .post("https://speed.cloudflare.com/__up")
        .body(body)
        .send()
        .await;

    progress_handle.abort();
    result?;

    let total = bytes_sent.load(Ordering::Relaxed);
    let elapsed = start.elapsed().as_secs_f64().max(0.001);
    let mbps = bytes_to_mbps(total, elapsed);
    let _ = tx.send(SpeedTestProgress::Uploading(mbps)).await;
    Ok(mbps)
}
