use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

mod app;
mod network;
mod speedtest;
mod ui;

#[cfg(test)]
mod app_tests;
#[cfg(test)]
mod main_tests;
#[cfg(test)]
mod network_tests;
#[cfg(test)]
mod speedtest_tests;

use app::{App, AppState};
use speedtest::{SpeedTestProgress, run_speed_test};

#[derive(Parser)]
#[command(name = "netspeed", about = "Network speed test and monitor")]
struct Cli {
    /// Network interface to monitor (default: auto-detected from routing table)
    #[arg(short, long, value_name = "IFACE")]
    interface: Option<String>,

    /// Launch the interactive terminal UI instead of running one CLI speed test
    #[arg(long)]
    tui: bool,

    /// Output format for CLI mode
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    output: OutputFormat,

    /// Print detailed speed test progress to stderr in CLI mode
    #[arg(long)]
    progress: bool,

    #[arg(long, hide = true)]
    speed_test_once: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliProgress {
    Quiet,
    Basic,
    Detailed,
}

enum Msg {
    Key(event::KeyEvent),
    Stats {
        download: f64,
        upload: f64,
        rx_delta: u64,
        tx_delta: u64,
    },
    InterfaceStatus(network::InterfaceStatus),
    SpeedTest(SpeedTestProgress),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let interface = match cli.interface {
        Some(i) => i,
        None => network::get_default_interface()?,
    };

    if !cli.tui || cli.speed_test_once {
        let progress = cli_progress_mode(cli.output, cli.progress || cli.speed_test_once);
        return run_cli_speed_test(interface, cli.output, progress).await;
    }

    let available = network::list_interfaces()?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, interface, available).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(ref e) = result {
        eprintln!("Error: {e}");
    }
    result
}

async fn run_cli_speed_test(
    interface: String,
    output: OutputFormat,
    progress_mode: CliProgress,
) -> Result<()> {
    let status = network::get_interface_status(&interface)?;
    if !status.is_usable() {
        bail!(
            "interface '{}' {}; speed test not started",
            interface,
            status.description()
        );
    }

    print_cli_progress_start(&interface, progress_mode);

    let (tx, mut rx) = mpsc::channel(32);
    tokio::spawn(run_speed_test(tx, interface.clone()));

    while let Some(progress) = rx.recv().await {
        match progress {
            SpeedTestProgress::Downloading(mbps) => {
                print_cli_progress_download(mbps, progress_mode);
            }
            SpeedTestProgress::Uploading(mbps) => {
                print_cli_progress_upload(mbps, progress_mode);
            }
            SpeedTestProgress::Done(result) => {
                print_cli_result(&interface, output, &result);
                return Ok(());
            }
            SpeedTestProgress::Error(error) => bail!(error),
        }
    }

    bail!("speed test ended without a result")
}

fn cli_progress_mode(output: OutputFormat, requested: bool) -> CliProgress {
    if requested {
        return CliProgress::Detailed;
    }

    match output {
        OutputFormat::Text => CliProgress::Basic,
        OutputFormat::Json => CliProgress::Quiet,
    }
}

fn print_cli_progress_start(interface: &str, mode: CliProgress) {
    match mode {
        CliProgress::Quiet => {}
        CliProgress::Basic => eprintln!("Testing network speed on {interface}..."),
        CliProgress::Detailed => eprintln!("testing_interface={interface}"),
    }
}

fn print_cli_progress_download(mbps: f64, mode: CliProgress) {
    match mode {
        CliProgress::Quiet => {}
        CliProgress::Basic if mbps < 0.001 => eprintln!("Testing download..."),
        CliProgress::Basic => {}
        CliProgress::Detailed => eprintln!("download_mbps={mbps:.2}"),
    }
}

fn print_cli_progress_upload(mbps: f64, mode: CliProgress) {
    match mode {
        CliProgress::Quiet => {}
        CliProgress::Basic if mbps < 0.001 => eprintln!("Testing upload..."),
        CliProgress::Basic => {}
        CliProgress::Detailed => eprintln!("upload_mbps={mbps:.2}"),
    }
}

fn print_cli_result(interface: &str, output: OutputFormat, result: &speedtest::SpeedTestResult) {
    match output {
        OutputFormat::Text => println!(
            "interface={} download_mbps={:.2} upload_mbps={:.2}",
            interface, result.download_mbps, result.upload_mbps
        ),
        OutputFormat::Json => println!(
            "{{\"interface\":\"{}\",\"download_mbps\":{:.2},\"upload_mbps\":{:.2}}}",
            json_escape(interface),
            result.download_mbps,
            result.upload_mbps
        ),
    }
}

fn json_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| match c {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            c => vec![c],
        })
        .collect()
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    interface: String,
    available: Vec<String>,
) -> Result<()> {
    let mut app = App::new(interface.clone(), available);
    app.interface_status = network::get_interface_status(&interface)
        .unwrap_or_else(|e| network::InterfaceStatus::Unknown(e.to_string()));
    let (tx, mut rx) = mpsc::channel::<Msg>(64);

    // Dedicated OS thread for blocking crossterm key events
    {
        let key_tx = tx.clone();
        std::thread::spawn(move || {
            loop {
                match event::poll(Duration::from_millis(100)) {
                    Ok(true) => {
                        if let Ok(Event::Key(k)) = event::read() {
                            if key_tx.blocking_send(Msg::Key(k)).is_err() {
                                break;
                            }
                        }
                    }
                    Ok(false) => {}
                    Err(_) => break,
                }
            }
        });
    }

    // Async task: poll /proc/net/dev every second
    let mut stats_handle = tokio::spawn(stats_task(tx.clone(), interface));

    loop {
        terminal.draw(|f| ui::render(f, &app))?;

        // Wait for the next message (100 ms refresh floor)
        let msg = tokio::select! {
            m = rx.recv() => m,
            _ = tokio::time::sleep(Duration::from_millis(100)) => None,
        };

        let should_quit = if let Some(msg) = msg {
            process(&mut app, msg, &tx, &mut stats_handle).await
        } else {
            false
        };

        // Drain any remaining queued messages without blocking
        while let Ok(msg) = rx.try_recv() {
            if process(&mut app, msg, &tx, &mut stats_handle).await {
                return Ok(());
            }
        }

        if should_quit {
            return Ok(());
        }
    }
}

async fn process(
    app: &mut App,
    msg: Msg,
    tx: &mpsc::Sender<Msg>,
    stats_handle: &mut tokio::task::JoinHandle<()>,
) -> bool {
    match msg {
        Msg::Key(key) => return handle_key(app, key, tx, stats_handle).await,

        Msg::Stats {
            download,
            upload,
            rx_delta,
            tx_delta,
        } => {
            app.push_speeds(download, upload);
            app.total_rx += rx_delta;
            app.total_tx += tx_delta;
        }

        Msg::InterfaceStatus(status) => {
            app.interface_status = status;
        }

        Msg::SpeedTest(progress) => handle_speedtest(app, progress),
    }
    false
}

async fn handle_key(
    app: &mut App,
    key: event::KeyEvent,
    tx: &mpsc::Sender<Msg>,
    stats_handle: &mut tokio::task::JoinHandle<()>,
) -> bool {
    use KeyCode::*;

    match key.code {
        // Quit (only when popup is closed)
        Char('q') | Char('Q') if !app.interface_selector_open => return true,
        Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,

        // Speed test
        Char('s') | Char('S')
            if !app.interface_selector_open && app.state != AppState::SpeedTesting =>
        {
            if !app.interface_status.is_usable() {
                app.speed_test_progress = Some(format!(
                    "Error: interface '{}' {}; speed test not started",
                    app.interface,
                    app.interface_status.description()
                ));
                app.state = AppState::Monitoring;
                return false;
            }

            app.state = AppState::SpeedTesting;
            app.speed_test_progress = Some("Connecting…".to_string());
            let interface = app.interface.clone();
            let st_tx = tx.clone();
            tokio::spawn(async move {
                let (ptx, mut prx) = mpsc::channel(32);
                tokio::spawn(run_speed_test(ptx, interface));
                while let Some(p) = prx.recv().await {
                    if st_tx.send(Msg::SpeedTest(p)).await.is_err() {
                        break;
                    }
                }
            });
        }

        // Interface selector toggle
        Char('i') | Char('I') => {
            app.interface_selector_open = !app.interface_selector_open;
        }

        // Navigate popup
        Down if app.interface_selector_open => {
            let n = app.available_interfaces.len();
            if n > 0 {
                app.selected_interface_idx = (app.selected_interface_idx + 1) % n;
            }
        }
        Up if app.interface_selector_open => {
            let n = app.available_interfaces.len();
            if n > 0 {
                app.selected_interface_idx = (app.selected_interface_idx + n - 1) % n;
            }
        }

        // Confirm interface selection
        Enter if app.interface_selector_open => {
            let new_iface = app.available_interfaces[app.selected_interface_idx].clone();
            if new_iface != app.interface {
                app.interface = new_iface.clone();
                app.reset_for_interface();
                app.interface_status = network::get_interface_status(&new_iface)
                    .unwrap_or_else(|e| network::InterfaceStatus::Unknown(e.to_string()));
                stats_handle.abort();
                *stats_handle = tokio::spawn(stats_task(tx.clone(), new_iface));
            }
            app.interface_selector_open = false;
        }

        // Close popup
        Esc if app.interface_selector_open => {
            app.interface_selector_open = false;
        }

        _ => {}
    }

    false
}

fn handle_speedtest(app: &mut App, progress: SpeedTestProgress) {
    match progress {
        SpeedTestProgress::Downloading(mbps) => {
            app.speed_test_progress = if mbps < 0.001 {
                Some("↓ Downloading…".to_string())
            } else {
                Some(format!("↓ Downloading: {}", network::format_speed(mbps)))
            };
        }
        SpeedTestProgress::Uploading(mbps) => {
            app.speed_test_progress = if mbps < 0.001 {
                Some("↑ Uploading…".to_string())
            } else {
                Some(format!("↑ Uploading: {}", network::format_speed(mbps)))
            };
        }
        SpeedTestProgress::Done(result) => {
            app.speed_test_result = Some(result);
            app.speed_test_progress = None;
            app.state = AppState::SpeedTestDone;
        }
        SpeedTestProgress::Error(e) => {
            app.speed_test_progress = Some(format!("Error: {e}"));
            app.state = AppState::Monitoring;
        }
    }
}

async fn stats_task(tx: mpsc::Sender<Msg>, interface: String) {
    let mut prev = network::get_interface_stats(&interface).ok();
    let mut prev_time = Instant::now();

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;

        if let Ok(status) = network::get_interface_status(&interface) {
            if tx.send(Msg::InterfaceStatus(status)).await.is_err() {
                break;
            }
        }

        let now = Instant::now();
        let elapsed = now.duration_since(prev_time).as_secs_f64();
        prev_time = now;

        match network::get_interface_stats(&interface) {
            Ok(curr) => {
                if let Some(p) = &prev {
                    let rx_delta = curr.rx_bytes.saturating_sub(p.rx_bytes);
                    let tx_delta = curr.tx_bytes.saturating_sub(p.tx_bytes);
                    let download = (rx_delta as f64 * 8.0) / (elapsed * 1_000_000.0);
                    let upload = (tx_delta as f64 * 8.0) / (elapsed * 1_000_000.0);

                    if tx
                        .send(Msg::Stats {
                            download,
                            upload,
                            rx_delta,
                            tx_delta,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                prev = Some(curr);
            }
            Err(_) => {
                prev = None;
            }
        }
    }
}
