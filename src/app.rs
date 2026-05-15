use std::collections::VecDeque;
use crate::speedtest::SpeedTestResult;

pub const HISTORY_SIZE: usize = 60;

#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    Monitoring,
    SpeedTesting,
    SpeedTestDone,
}

pub struct App {
    pub interface: String,
    pub available_interfaces: Vec<String>,
    pub download_speed: f64,
    pub upload_speed: f64,
    pub peak_download: f64,
    pub peak_upload: f64,
    pub download_history: VecDeque<f64>,
    pub upload_history: VecDeque<f64>,
    pub total_rx: u64,
    pub total_tx: u64,
    pub state: AppState,
    pub speed_test_result: Option<SpeedTestResult>,
    pub speed_test_progress: Option<String>,
    pub interface_selector_open: bool,
    pub selected_interface_idx: usize,
}

impl App {
    pub fn new(interface: String, available_interfaces: Vec<String>) -> Self {
        let idx = available_interfaces.iter()
            .position(|i| i == &interface)
            .unwrap_or(0);

        Self {
            interface,
            available_interfaces,
            download_speed: 0.0,
            upload_speed: 0.0,
            peak_download: 0.0,
            peak_upload: 0.0,
            download_history: VecDeque::with_capacity(HISTORY_SIZE),
            upload_history: VecDeque::with_capacity(HISTORY_SIZE),
            total_rx: 0,
            total_tx: 0,
            state: AppState::Monitoring,
            speed_test_result: None,
            speed_test_progress: None,
            interface_selector_open: false,
            selected_interface_idx: idx,
        }
    }

    pub fn push_speeds(&mut self, download: f64, upload: f64) {
        self.download_speed = download;
        self.upload_speed = upload;
        if download > self.peak_download { self.peak_download = download; }
        if upload > self.peak_upload { self.peak_upload = upload; }
        push_capped(&mut self.download_history, download, HISTORY_SIZE);
        push_capped(&mut self.upload_history, upload, HISTORY_SIZE);
    }

    pub fn reset_for_interface(&mut self) {
        self.download_speed = 0.0;
        self.upload_speed = 0.0;
        self.peak_download = 0.0;
        self.peak_upload = 0.0;
        self.download_history.clear();
        self.upload_history.clear();
        self.total_rx = 0;
        self.total_tx = 0;
    }
}

fn push_capped(deque: &mut VecDeque<f64>, val: f64, cap: usize) {
    if deque.len() >= cap {
        deque.pop_front();
    }
    deque.push_back(val);
}
