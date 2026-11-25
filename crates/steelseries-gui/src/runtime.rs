use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use crate::{
    cli::{CliClient, CliClientError},
    monitor::MonitorState,
    notifications::{self, BatteryNotifications},
    settings::GuiSettings,
};

#[derive(Debug, Clone)]
pub struct MonitorEvent {
    pub scan_revision: u64,
    pub settings: GuiSettings,
    pub state: MonitorState,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum MonitorCommand {
    Refresh,
    SettingsChanged,
}

#[derive(Clone)]
pub struct MonitorController {
    scan_revision: Arc<AtomicU64>,
    sender: mpsc::Sender<MonitorCommand>,
    pub settings: Arc<Mutex<GuiSettings>>,
    settings_path: Arc<PathBuf>,
}

impl MonitorController {
    pub fn start(
        client: CliClient,
        settings: GuiSettings,
        settings_path: PathBuf,
        scan_revision: Arc<AtomicU64>,
        on_event: impl Fn(MonitorEvent) + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        let settings = Arc::new(Mutex::new(settings));
        let shared_settings = Arc::clone(&settings);
        let shared_path = Arc::new(settings_path);
        let worker_path = Arc::clone(&shared_path);
        let worker_revision = Arc::clone(&scan_revision);
        thread::spawn(move || {
            monitor_loop(
                client,
                shared_settings,
                worker_path,
                worker_revision,
                receiver,
                on_event,
            );
        });
        Self {
            scan_revision,
            sender,
            settings,
            settings_path: shared_path,
        }
    }

    pub fn refresh(&self) {
        self.scan_revision.fetch_add(1, Ordering::Relaxed);
        let _ = self.sender.send(MonitorCommand::Refresh);
    }

    pub fn event_is_current(&self, event: &MonitorEvent) -> bool {
        event.scan_revision == self.scan_revision.load(Ordering::Relaxed)
    }

    pub fn settings_changed(&self) {
        self.save_settings();
        let _ = self.sender.send(MonitorCommand::SettingsChanged);
    }

    pub fn save_settings(&self) {
        if let Ok(settings) = self.settings.lock()
            && let Err(error) = settings.save(&self.settings_path)
        {
            eprintln!("steelseries-linux-gui: could not save settings: {error}");
        }
    }
}

fn monitor_loop(
    client: CliClient,
    settings: Arc<Mutex<GuiSettings>>,
    settings_path: Arc<PathBuf>,
    scan_revision: Arc<AtomicU64>,
    receiver: mpsc::Receiver<MonitorCommand>,
    on_event: impl Fn(MonitorEvent),
) {
    let mut state = MonitorState::default();
    let mut notifications = BatteryNotifications::default();
    loop {
        let revision = scan_revision.load(Ordering::Relaxed);
        state.poll_in_progress = true;
        let error = refresh_monitor(
            &client,
            &settings,
            &settings_path,
            &mut state,
            &mut notifications,
        )
        .err()
        .map(|error| {
            eprintln!("steelseries-linux-gui: {error}");
            error.user_message()
        });
        state.poll_in_progress = false;
        if let Ok(settings) = settings.lock() {
            on_event(MonitorEvent {
                scan_revision: revision,
                settings: settings.clone(),
                state: state.clone(),
                error,
            });
        }

        let interval = settings
            .lock()
            .map_or(60, |settings| settings.refresh_interval_seconds);
        match receiver.recv_timeout(Duration::from_secs(interval)) {
            Ok(MonitorCommand::Refresh | MonitorCommand::SettingsChanged)
            | Err(mpsc::RecvTimeoutError::Timeout) => while receiver.try_recv().is_ok() {},
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn refresh_monitor(
    client: &CliClient,
    settings: &Mutex<GuiSettings>,
    settings_path: &Path,
    state: &mut MonitorState,
    notifications: &mut BatteryNotifications,
) -> Result<(), CliClientError> {
    let devices = client.list_devices()?;
    if let Ok(mut settings) = settings.lock() {
        state.reconcile_devices(&mut settings, devices);
        if let Err(error) = settings.save(settings_path) {
            eprintln!("steelseries-linux-gui: could not save device registry: {error}");
        }
    }

    while let Some(target) = state.tray_target.clone() {
        match client.get_battery(&target) {
            Ok(battery) => {
                let warnings = settings.lock().map_or_else(
                    |_| Vec::new(),
                    |settings| notifications.observe(&target, &battery, &settings),
                );
                if let Some(device) = state
                    .active_devices
                    .iter()
                    .find(|device| device.id == target)
                {
                    for warning in warnings {
                        if let Err(error) = notifications::send(
                            &device.name,
                            battery.percentage.unwrap_or_default(),
                            warning,
                        ) {
                            eprintln!(
                                "steelseries-linux-gui: could not send battery notification: {error}"
                            );
                        }
                    }
                }
                state.cache_battery(&target, battery);
                break;
            }
            Err(error) if error.is_device_not_found() => {
                if let Ok(settings) = settings.lock() {
                    state.mark_device_not_found(&settings, &target);
                }
            }
            Err(error) if error.is_unsupported_feature() => {
                state.batteries.remove(&target);
                break;
            }
            Err(error) => {
                state.batteries.remove(&target);
                eprintln!("steelseries-linux-gui: battery query failed: {error}");
                break;
            }
        }
    }
    Ok(())
}
