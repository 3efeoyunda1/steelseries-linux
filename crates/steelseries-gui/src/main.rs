mod battery_icon;
mod cli;
mod dashboard;
mod dpi_curve;
mod monitor;
mod runtime;
mod settings;
mod single_instance;
mod tray;
mod window_lifecycle;

use std::{
    cell::RefCell,
    collections::HashMap,
    env,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

use battery_icon::colored_battery_image;
use cli::{CliClient, CliClientError, Device, DeviceDescription, DeviceSnapshot, DpiValue};
use dashboard::DashboardView;
use dpi_curve::DpiCurve;
use monitor::{CachedBattery, MonitorState};
use runtime::{MonitorController, MonitorEvent};
use settings::{
    GuiSettings, KnownDevice, REFRESH_INTERVAL_OPTIONS, TrayIconTheme, autostart_path,
    set_autostart_enabled, settings_path,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use tray::{TrayAction, TrayController, TrayState};
use window_lifecycle::{WINDOW_HEIGHT, WINDOW_WIDTH, WindowLifecycle};

#[allow(unsafe_code)]
mod ui {
    slint::include_modules!();
}

use ui::{AppWindow, DashboardData, DashboardRow, DeviceItem, DpiReferenceItem, KnownDeviceItem};

#[derive(Debug, Clone)]
enum MutationAction {
    Dpi(Vec<DpiValue>),
    ActiveDpi(usize),
    Lod {
        stage: usize,
        value: String,
    },
    Polling {
        mode: &'static str,
        hz: u16,
    },
    LowPower(bool),
    LowPowerPolling(u16),
    PowerDraft {
        sleep_timer: Option<u64>,
        auto_low_power_threshold: Option<u8>,
    },
    AutoLowPower(bool),
    WirelessStability(bool),
    BluetoothSmoothing(bool),
    ScrollJump(bool),
    ScrollJumpDelay(u16),
}

struct MutationFailure {
    error: CliClientError,
    persisted_snapshot: Option<DeviceSnapshot>,
}

impl MutationAction {
    fn execute(&self, client: &CliClient, device_id: &str) -> Result<(), CliClientError> {
        match self {
            Self::Dpi(stages) => client.set_dpi(device_id, stages),
            Self::ActiveDpi(stage) => client.set_active_dpi_stage(device_id, *stage),
            Self::Lod { stage, value } => client.set_lod(device_id, *stage, value),
            Self::Polling { mode, hz } => client.set_polling(device_id, mode, *hz),
            Self::LowPower(enabled) => client.set_low_power(device_id, *enabled),
            Self::LowPowerPolling(hz) => client.set_low_power_polling(device_id, *hz),
            Self::PowerDraft {
                sleep_timer,
                auto_low_power_threshold,
            } => {
                if let Some(minutes) = sleep_timer {
                    client.set_sleep_timer(device_id, *minutes)?;
                }
                if let Some(percent) = auto_low_power_threshold {
                    client.set_auto_low_power_threshold(device_id, *percent)?;
                }
                Ok(())
            }
            Self::AutoLowPower(enabled) => client.set_auto_low_power(device_id, *enabled),
            Self::WirelessStability(enabled) => client.set_wireless_stability(device_id, *enabled),
            Self::BluetoothSmoothing(enabled) => {
                client.set_bluetooth_smoothing(device_id, *enabled)
            }
            Self::ScrollJump(enabled) => client.set_scroll_jump(device_id, *enabled),
            Self::ScrollJumpDelay(delay) => client.set_scroll_jump_delay(device_id, *delay),
        }
    }

    fn is_dpi_configuration(&self) -> bool {
        matches!(self, Self::Dpi(_))
    }

    fn execute_and_refresh(
        &self,
        client: &CliClient,
        device_id: &str,
    ) -> Result<DeviceSnapshot, Box<MutationFailure>> {
        match self.execute(client, device_id) {
            Ok(()) => client.snapshot_device(device_id).map_err(|error| {
                Box::new(MutationFailure {
                    error,
                    persisted_snapshot: None,
                })
            }),
            Err(error) => Err(Box::new(MutationFailure {
                persisted_snapshot: client.snapshot_device(device_id).ok(),
                error,
            })),
        }
    }
}

#[derive(Clone)]
struct MutationContext {
    client: CliClient,
    description: Arc<Mutex<Option<DeviceDescription>>>,
    busy: Arc<std::sync::atomic::AtomicBool>,
}

thread_local! {
    // Slint components live on the event-loop thread. Keep the same component
    // after the first activation so hiding to the tray preserves its state.
    static MAIN_WINDOW: RefCell<Option<AppWindow>> = const { RefCell::new(None) };
}

#[derive(Clone)]
struct WindowContext {
    client: CliClient,
    monitor: MonitorController,
    tray: Arc<Mutex<TrayController>>,
    lifecycle: Arc<Mutex<WindowLifecycle>>,
    latest_event: Arc<Mutex<Option<MonitorEvent>>>,
    settings_error: Option<String>,
}

fn main() -> Result<(), slint::PlatformError> {
    let launch_from_autostart = env::args_os().any(|argument| argument == "--autostart");
    let Some(instance_socket) = single_instance::claim(!launch_from_autostart)
        .map_err(|error| slint::PlatformError::Other(format!("GUI activation failed: {error}")))?
    else {
        return Ok(());
    };
    // Initialize the platform without creating a window, then set its identity.
    slint::BackendSelector::new().select()?;
    slint::set_xdg_app_id("steelseries-linux-gui")?;
    let client = CliClient::from_environment();
    let config_path = settings_path();
    let initial_settings =
        settings_for_launch(GuiSettings::load(&config_path), launch_from_autostart);
    let settings_error =
        settings::migrate_autostart(&autostart_path(), initial_settings.start_at_login)
            .err()
            .map(|error| {
                eprintln!("steelseries-linux-gui: could not migrate autostart entry: {error}");
                format!("Could not update Start at login: {error}")
            });
    let show_initially = initial_window_visible(launch_from_autostart);
    let window_lifecycle = Arc::new(Mutex::new(WindowLifecycle::new(show_initially)));
    let (tray_action_sender, tray_action_receiver) = mpsc::channel();
    let activation_sender = tray_action_sender.clone();
    thread::spawn(move || {
        let mut message = [0; 16];
        loop {
            match instance_socket.recv(&mut message) {
                Ok(size) if &message[..size] == b"open" => {
                    if activation_sender.send(TrayAction::Open).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    eprintln!("GUI activation listener failed: {error}");
                    break;
                }
            }
        }
    });
    let tray = Arc::new(Mutex::new(TrayController::new(tray_action_sender)));
    if let Ok(mut tray) = tray.lock() {
        tray.set_icon_theme(initial_settings.tray_icon_theme);
        tray.set_enabled(initial_settings.tray_enabled);
    }

    let latest_event = Arc::new(Mutex::new(None));
    let monitor_latest_event = Arc::clone(&latest_event);
    let monitor_tray = Arc::clone(&tray);
    let monitor = MonitorController::start(
        client.clone(),
        initial_settings,
        config_path,
        move |event| {
            update_tray(&monitor_tray, &event);
            if let Ok(mut latest) = monitor_latest_event.lock() {
                *latest = Some(event.clone());
            }
            let _ = slint::invoke_from_event_loop(move || {
                MAIN_WINDOW.with_borrow(|window| {
                    if let Some(ui) = window {
                        apply_monitor_event(ui, event);
                    }
                });
            });
        },
    );
    let context = WindowContext {
        client,
        monitor,
        tray,
        lifecycle: window_lifecycle,
        latest_event,
        settings_error,
    };
    if show_initially {
        open_main_window(&context)?;
    }

    thread::spawn(move || {
        while let Ok(action) = tray_action_receiver.recv() {
            match action {
                TrayAction::Open => {
                    let context = context.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Err(error) = open_main_window(&context) {
                            eprintln!("Could not restore GUI: {error}");
                        }
                    });
                }
                TrayAction::Quit => {
                    let _ = slint::invoke_from_event_loop(|| {
                        let _ = slint::quit_event_loop();
                    });
                    break;
                }
            }
        }
    });
    slint::run_event_loop_until_quit()
}

fn open_main_window(context: &WindowContext) -> Result<(), slint::PlatformError> {
    MAIN_WINDOW.with_borrow_mut(|window| {
        if window.is_none() {
            let ui = AppWindow::new()?;
            window_lifecycle::install_native_policy(ui.window());
            if let Some(error) = &context.settings_error {
                ui.set_settings_error(error.as_str().into());
            }
            wire_callbacks(
                &ui,
                &context.client,
                &Arc::new(AtomicU64::new(0)),
                &Arc::new(Mutex::new(None)),
                &Arc::new(std::sync::atomic::AtomicBool::new(false)),
                &context.monitor,
                &context.tray,
                &context.lifecycle,
            );
            let close_settings = Arc::clone(&context.monitor.settings);
            let close_lifecycle = Arc::clone(&context.lifecycle);
            ui.window().on_close_requested(move || {
                if close_settings
                    .lock()
                    .is_ok_and(|settings| settings.tray_enabled)
                {
                    if let Ok(mut lifecycle) = close_lifecycle.lock() {
                        lifecycle.hide();
                    }
                } else {
                    let _ = slint::quit_event_loop();
                }
                slint::CloseRequestResponse::HideWindow
            });
            if let Ok(latest) = context.latest_event.lock()
                && let Some(event) = latest.as_ref()
            {
                apply_monitor_event(&ui, event.clone());
            }
            *window = Some(ui);
        }
        let ui = window.as_ref().expect("window initialized above");
        let size = context
            .lifecycle
            .lock()
            .map_or((WINDOW_WIDTH, WINDOW_HEIGHT), |mut lifecycle| {
                lifecycle.restore()
            });
        normalize_window(ui, size.0, size.1);
        ui.show()?;
        window_lifecycle::restore_native_window(ui.window());
        Ok(())
    })
}

const fn initial_window_visible(launch_from_autostart: bool) -> bool {
    !launch_from_autostart
}

fn settings_for_launch(mut settings: GuiSettings, launch_from_autostart: bool) -> GuiSettings {
    // A hidden autostart needs a tray entry so the user can reopen or quit it.
    if launch_from_autostart {
        settings.tray_enabled = true;
    }
    settings
}

#[allow(clippy::too_many_arguments)]
fn wire_callbacks(
    ui: &AppWindow,
    client: &CliClient,
    dashboard_generation: &Arc<AtomicU64>,
    description: &Arc<Mutex<Option<DeviceDescription>>>,
    mutation_busy: &Arc<std::sync::atomic::AtomicBool>,
    monitor: &MonitorController,
    tray: &Arc<Mutex<TrayController>>,
    lifecycle: &Arc<Mutex<WindowLifecycle>>,
) {
    let mutation_context = MutationContext {
        client: client.clone(),
        description: Arc::clone(description),
        busy: Arc::clone(mutation_busy),
    };
    let refresh_monitor = monitor.clone();
    let refresh_generation = Arc::clone(dashboard_generation);
    ui.on_refresh_requested(move || {
        refresh_generation.fetch_add(1, Ordering::Relaxed);
        refresh_monitor.refresh();
    });

    let selection_ui = ui.as_weak();
    let selection_client = client.clone();
    let selection_generation = Arc::clone(dashboard_generation);
    let selection_description = Arc::clone(description);
    let selection_lifecycle = Arc::clone(lifecycle);
    ui.on_device_selected(move |index| {
        let Some(ui) = selection_ui.upgrade() else {
            return;
        };
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(device) = ui.get_devices().row_data(index) else {
            return;
        };
        begin_page_change(&ui, &selection_lifecycle);
        ui.set_selected_name(device.name);
        ui.set_selected_connection(device.connection);
        ui.set_selected_connection_key(device.connection_key);
        ui.set_selected_id(device.id.clone());
        ui.set_selected_model_key(device.model_key);
        start_dashboard_load(
            selection_ui.clone(),
            selection_client.clone(),
            device.id.to_string(),
            Arc::clone(&selection_generation),
            Arc::clone(&selection_description),
        );
    });

    let dashboard_ui = ui.as_weak();
    let dashboard_client = client.clone();
    let refresh_dashboard_generation = Arc::clone(dashboard_generation);
    let refresh_description = Arc::clone(description);
    ui.on_dashboard_refresh_requested(move || {
        let Some(ui) = dashboard_ui.upgrade() else {
            return;
        };
        let device_id = ui.get_selected_id().to_string();
        if device_id.is_empty() {
            return;
        }
        start_dashboard_load(
            dashboard_ui.clone(),
            dashboard_client.clone(),
            device_id,
            Arc::clone(&refresh_dashboard_generation),
            Arc::clone(&refresh_description),
        );
    });

    let back_ui = ui.as_weak();
    let back_generation = Arc::clone(dashboard_generation);
    let back_lifecycle = Arc::clone(lifecycle);
    ui.on_back_requested(move || {
        back_generation.fetch_add(1, Ordering::Relaxed);
        if let Some(ui) = back_ui.upgrade() {
            begin_page_change(&ui, &back_lifecycle);
            ui.set_view_state(if ui.get_devices().row_count() == 0 {
                "empty".into()
            } else {
                "list".into()
            });
        }
    });

    wire_settings_callbacks(ui, monitor, tray, lifecycle);

    wire_dpi_callbacks(ui, &mutation_context);
    wire_feature_callbacks(ui, &mutation_context);
}

fn wire_settings_callbacks(
    ui: &AppWindow,
    monitor: &MonitorController,
    tray: &Arc<Mutex<TrayController>>,
    lifecycle: &Arc<Mutex<WindowLifecycle>>,
) {
    let settings_ui = ui.as_weak();
    let settings_lifecycle = Arc::clone(lifecycle);
    ui.on_settings_requested(move || {
        if let Some(ui) = settings_ui.upgrade() {
            begin_page_change(&ui, &settings_lifecycle);
            ui.set_view_state("settings".into());
        }
    });

    let back_ui = ui.as_weak();
    let back_lifecycle = Arc::clone(lifecycle);
    ui.on_settings_back_requested(move || {
        if let Some(ui) = back_ui.upgrade() {
            begin_page_change(&ui, &back_lifecycle);
            ui.set_view_state(if ui.get_devices().row_count() == 0 {
                "empty".into()
            } else {
                "list".into()
            });
        }
    });

    let tray_monitor = monitor.clone();
    let tray_controller = Arc::clone(tray);
    ui.on_tray_enabled_changed(move || {
        let enabled = if let Ok(mut settings) = tray_monitor.settings.lock() {
            settings.tray_enabled = !settings.tray_enabled;
            settings.tray_enabled
        } else {
            return;
        };
        if let Ok(mut tray) = tray_controller.lock() {
            tray.set_enabled(enabled);
        }
        tray_monitor.settings_changed();
    });

    let interval_monitor = monitor.clone();
    ui.on_refresh_interval_selected(move |index| {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(interval) = REFRESH_INTERVAL_OPTIONS.get(index).copied() else {
            return;
        };
        if let Ok(mut settings) = interval_monitor.settings.lock() {
            settings.refresh_interval_seconds = interval;
        } else {
            return;
        }
        interval_monitor.settings_changed();
    });

    let theme_monitor = monitor.clone();
    let theme_tray = Arc::clone(tray);
    let theme_ui = ui.as_weak();
    ui.on_tray_icon_theme_selected(move |index| {
        let Some(theme) = TrayIconTheme::from_index(index) else {
            return;
        };
        if let Ok(mut settings) = theme_monitor.settings.lock() {
            settings.tray_icon_theme = theme;
        } else {
            return;
        }
        if let Ok(mut tray) = theme_tray.lock() {
            tray.set_icon_theme(theme);
        }
        if let Some(ui) = theme_ui.upgrade() {
            ui.set_tray_icon_theme_index(theme.index());
        }
        theme_monitor.save_settings();
    });

    let autostart_monitor = monitor.clone();
    let autostart_ui = ui.as_weak();
    ui.on_start_at_login_changed(move || {
        let enabled = match autostart_monitor.settings.lock() {
            Ok(settings) => !settings.start_at_login,
            Err(_) => return,
        };
        if let Err(error) = set_autostart_enabled(&autostart_path(), enabled) {
            if let Some(ui) = autostart_ui.upgrade() {
                ui.set_settings_error(format!("Could not update start at login: {error}").into());
            }
            return;
        }
        if let Ok(mut settings) = autostart_monitor.settings.lock() {
            settings.start_at_login = enabled;
        } else {
            return;
        }
        autostart_monitor.save_settings();
        if let Some(ui) = autostart_ui.upgrade() {
            ui.set_start_at_login(enabled);
            ui.set_settings_error(SharedString::default());
        }
    });

    let priority_monitor = monitor.clone();
    ui.on_priority_move_requested(move |index, direction| {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let Some(target) = index.checked_add_signed(direction as isize) else {
            return;
        };
        if let Ok(mut settings) = priority_monitor.settings.lock() {
            if index >= settings.device_priority.len() || target >= settings.device_priority.len() {
                return;
            }
            settings.device_priority.swap(index, target);
        } else {
            return;
        }
        priority_monitor.settings_changed();
    });
}

fn begin_page_change(ui: &AppWindow, lifecycle: &Mutex<WindowLifecycle>) {
    let generation = lifecycle.lock().map_or_else(
        |_| ui.get_page_generation().wrapping_add(1),
        |mut value| value.navigate(),
    );
    ui.set_page_generation(generation);
}

fn normalize_window(ui: &AppWindow, width: u32, height: u32) {
    ui.window().set_fullscreen(false);
    ui.window().set_maximized(false);
    ui.set_fixed_window_width(i32::try_from(width).unwrap_or(i32::MAX));
    ui.set_fixed_window_height(i32::try_from(height).unwrap_or(i32::MAX));
    ui.window()
        .set_size(slint::LogicalSize::new(width as f32, height as f32));
}

fn wire_dpi_callbacks(ui: &AppWindow, context: &MutationContext) {
    let preview_ui = ui.as_weak();
    ui.on_dpi_position_previewed(move |index, axis, position| {
        let Some(ui) = preview_ui.upgrade() else {
            return;
        };
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let dashboard = ui.get_dashboard();
        let Some(curve) = dpi_curve_from_dashboard(&dashboard) else {
            return;
        };
        let value = i32::from(curve.position_to_dpi(f64::from(position)));
        let model = ui.get_dpi_rows();
        let Some(mut row) = model.row_data(index) else {
            return;
        };
        if !update_dpi_axis_with_position(&mut row, axis.as_str(), value, position) {
            return;
        }
        update_dpi_row_text(&mut row);
        model.set_row_data(index, row);
        ui.set_dpi_error(SharedString::default());
    });

    let release_ui = ui.as_weak();
    let release_context = context.clone();
    ui.on_dpi_position_released(move |index, axis, position| {
        let Some(ui) = release_ui.upgrade() else {
            return;
        };
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let dashboard = ui.get_dashboard();
        let Some(curve) = dpi_curve_from_dashboard(&dashboard) else {
            return;
        };
        let value = i32::from(curve.position_to_dpi(f64::from(position)));
        finalize_dpi_change(
            &ui,
            release_ui.clone(),
            release_context.clone(),
            index,
            axis.as_str(),
            value,
            curve,
        );
    });

    let edit_ui = ui.as_weak();
    let edit_context = context.clone();
    ui.on_dpi_value_edited(move |index, axis, text| {
        let Some(ui) = edit_ui.upgrade() else {
            return false;
        };
        let Ok(index) = usize::try_from(index) else {
            return false;
        };
        let dashboard = ui.get_dashboard();
        let Some(curve) = dpi_curve_from_dashboard(&dashboard) else {
            return false;
        };
        let parsed = match text.trim().parse::<i64>() {
            Ok(value) => value,
            Err(_) => {
                ui.set_dpi_error(
                    format!(
                        "Enter a whole number between {} and {}.",
                        dashboard.dpi_min, dashboard.dpi_max
                    )
                    .into(),
                );
                return false;
            }
        };
        let value = match curve.finalize_text_value(parsed) {
            Ok(value) => i32::from(value),
            Err(_) => {
                ui.set_dpi_error(
                    format!(
                        "DPI must be between {} and {}.",
                        dashboard.dpi_min, dashboard.dpi_max
                    )
                    .into(),
                );
                return false;
            }
        };
        finalize_dpi_change(
            &ui,
            edit_ui.clone(),
            edit_context.clone(),
            index,
            axis.as_str(),
            value,
            curve,
        );
        true
    });

    let reference_ui = ui.as_weak();
    let reference_context = context.clone();
    ui.on_dpi_reference_selected(move |index, axis, value| {
        let Some(ui) = reference_ui.upgrade() else {
            return;
        };
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let dashboard = ui.get_dashboard();
        let Some(curve) = dpi_curve_from_dashboard(&dashboard) else {
            return;
        };
        let value = i32::from(curve.snap(f64::from(value)));
        finalize_dpi_change(
            &ui,
            reference_ui.clone(),
            reference_context.clone(),
            index,
            axis.as_str(),
            value,
            curve,
        );
    });

    let separate_ui = ui.as_weak();
    let separate_context = context.clone();
    ui.on_dpi_separate_requested(move |index| {
        let Some(ui) = separate_ui.upgrade() else {
            return;
        };
        if !ui.get_dashboard().dpi_independent_axes {
            return;
        }
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let mut rows = model_rows(&ui.get_dpi_rows());
        let Some(row) = rows.get_mut(index) else {
            return;
        };
        let enabling = !row.separate;
        set_separate_axes(row, enabling);
        if !enabling {
            row.y_position = row.x_position;
        }
        update_dpi_row_text(row);
        if enabling || !row.persisted {
            set_dpi_rows(&ui, rows);
            return;
        }
        let Some(stages) = dpi_values(&rows) else {
            ui.set_dpi_error("DPI stage values are invalid.".into());
            return;
        };
        set_dpi_rows(&ui, rows);
        drop(ui);
        start_mutation(
            separate_ui.clone(),
            separate_context.clone(),
            MutationAction::Dpi(stages),
        );
    });

    let apply_ui = ui.as_weak();
    let apply_context = context.clone();
    ui.on_dpi_apply_requested(move |_index| {
        let Some(ui) = apply_ui.upgrade() else {
            return;
        };
        let rows = model_rows(&ui.get_dpi_rows());
        let Some(stages) = dpi_values(&rows) else {
            ui.set_dpi_error("DPI stage values are invalid.".into());
            return;
        };
        drop(ui);
        start_mutation(
            apply_ui.clone(),
            apply_context.clone(),
            MutationAction::Dpi(stages),
        );
    });

    let add_ui = ui.as_weak();
    ui.on_dpi_add_requested(move || {
        let Some(ui) = add_ui.upgrade() else { return };
        let mut rows = model_rows(&ui.get_dpi_rows());
        let dashboard = ui.get_dashboard();
        if !can_add_stage(rows.len(), dashboard.dpi_max_stages) {
            return;
        }
        let value = rows.last().map_or(dashboard.dpi_min, |row| row.x);
        let Some(curve) = dpi_curve_from_dashboard(&dashboard) else {
            return;
        };
        rows.push(new_dpi_stage(rows.len() + 1, value, curve));
        set_dpi_rows(&ui, rows);
    });

    let remove_ui = ui.as_weak();
    let remove_context = context.clone();
    ui.on_dpi_remove_requested(move |index| {
        let Some(ui) = remove_ui.upgrade() else {
            return;
        };
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let mut rows = model_rows(&ui.get_dpi_rows());
        if rows.len() <= 1 || index >= rows.len() {
            return;
        }
        let removed = rows.remove(index);
        let removed_active = removed.active;
        for (index, row) in rows.iter_mut().enumerate() {
            row.label = format!("Stage {}", index + 1).into();
        }
        if removed_active {
            rows[0].active = true;
        }
        let Some(stages) = dpi_values(&rows) else {
            ui.set_dpi_error("DPI stage values are invalid.".into());
            return;
        };
        let has_unsaved = rows.iter().any(|row| !row.persisted);
        set_dpi_rows(&ui, rows);
        if !removed.persisted || has_unsaved {
            return;
        }
        drop(ui);
        start_mutation(
            remove_ui.clone(),
            remove_context.clone(),
            MutationAction::Dpi(stages),
        );
    });

    let active_ui = ui.as_weak();
    let active_context = context.clone();
    ui.on_active_dpi_requested(move |stage| {
        let Ok(stage) = usize::try_from(stage) else {
            return;
        };
        let Some(ui) = active_ui.upgrade() else {
            return;
        };
        let rows = model_rows(&ui.get_dpi_rows());
        if stage == 0 || !rows.get(stage - 1).is_some_and(dpi_stage_is_usable) {
            return;
        }
        drop(ui);
        start_mutation(
            active_ui.clone(),
            active_context.clone(),
            MutationAction::ActiveDpi(stage),
        );
    });

    let lod_ui = ui.as_weak();
    let lod_context = context.clone();
    ui.on_lod_selected(move |stage, option_index| {
        let Some(ui) = lod_ui.upgrade() else { return };
        let Ok(stage_index) = usize::try_from(stage - 1) else {
            return;
        };
        let Ok(option_index) = usize::try_from(option_index) else {
            return;
        };
        let rows = model_rows(&ui.get_dpi_rows());
        let Some(row) = rows.get(stage_index) else {
            return;
        };
        if !row.persisted {
            return;
        }
        let values = lod_context
            .description
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .map(|value| value.capabilities.lod.values)
            .unwrap_or_default();
        let Some(next) = values.get(option_index).map(|value| value.value.as_str()) else {
            return;
        };
        if next == row.raw_value.as_str() {
            return;
        }
        start_mutation(
            lod_ui.clone(),
            lod_context.clone(),
            MutationAction::Lod {
                stage: stage_index + 1,
                value: next.to_owned(),
            },
        );
    });
}

fn wire_feature_callbacks(ui: &AppWindow, context: &MutationContext) {
    let choice_ui = ui.as_weak();
    let choice_context = context.clone();
    ui.on_choice_selected(move |feature, option_index| {
        if choice_ui.upgrade().is_none() {
            return;
        }
        let Ok(option_index) = usize::try_from(option_index) else {
            return;
        };
        let Some(description) = choice_context
            .description
            .lock()
            .ok()
            .and_then(|value| value.clone())
        else {
            return;
        };
        let action = match feature.as_str() {
            "wireless-polling" => description
                .capabilities
                .polling
                .modes
                .wireless
                .rates_hz
                .get(option_index)
                .copied()
                .map(|hz| MutationAction::Polling {
                    mode: "wireless",
                    hz,
                }),
            "wired-polling" => description
                .capabilities
                .polling
                .modes
                .wired
                .rates_hz
                .get(option_index)
                .copied()
                .map(|hz| MutationAction::Polling { mode: "wired", hz }),
            "low-power-polling" => description
                .capabilities
                .power
                .low_power_polling
                .rates_hz
                .get(option_index)
                .copied()
                .map(MutationAction::LowPowerPolling),
            _ => None,
        };
        if let Some(action) = action {
            start_mutation(choice_ui.clone(), choice_context.clone(), action);
        }
    });

    let toggle_ui = ui.as_weak();
    let toggle_context = context.clone();
    ui.on_toggle_requested(move |feature| {
        let Some(ui) = toggle_ui.upgrade() else {
            return;
        };
        let dashboard = ui.get_dashboard();
        let action = match feature.as_str() {
            "low-power" => Some(MutationAction::LowPower(!dashboard.low_power_enabled)),
            "auto-low-power" => Some(MutationAction::AutoLowPower(
                !dashboard.auto_low_power_enabled,
            )),
            "wireless-stability" => Some(MutationAction::WirelessStability(
                !dashboard.wireless_stability_enabled,
            )),
            "bluetooth-smoothing" => Some(MutationAction::BluetoothSmoothing(
                !dashboard.bluetooth_smoothing_enabled,
            )),
            "scroll-jump" => Some(MutationAction::ScrollJump(!dashboard.scroll_jump_enabled)),
            _ => None,
        };
        if let Some(action) = action {
            start_mutation(toggle_ui.clone(), toggle_context.clone(), action);
        }
    });

    let preview_ui = ui.as_weak();
    ui.on_numeric_value_previewed(move |feature, value| {
        let Some(ui) = preview_ui.upgrade() else {
            return;
        };
        let mut data = ui.get_dashboard();
        match feature.as_str() {
            "sleep-timer" => {
                let Some(value) = snap_to_step(
                    value,
                    data.sleep_timer_min,
                    data.sleep_timer_max,
                    data.sleep_timer_step,
                ) else {
                    return;
                };
                data.sleep_timer_edit = value;
            }
            "auto-low-power-threshold" => {
                let Some(value) = snap_to_step(
                    value,
                    data.auto_low_power_threshold_min,
                    data.auto_low_power_threshold_max,
                    data.auto_low_power_threshold_step,
                ) else {
                    return;
                };
                data.auto_low_power_threshold_edit = value;
            }
            "scroll-jump-delay" => {
                let Some(value) = snap_to_step(
                    value,
                    data.scroll_jump_delay_min,
                    data.scroll_jump_delay_max,
                    data.scroll_jump_delay_step,
                ) else {
                    return;
                };
                data.scroll_jump_delay_edit = value;
            }
            _ => return,
        }
        ui.set_dashboard(data);
    });

    let edit_ui = ui.as_weak();
    let edit_context = context.clone();
    ui.on_numeric_value_edited(move |feature, text| {
        let Some(ui) = edit_ui.upgrade() else {
            return false;
        };
        let mut data = ui.get_dashboard();
        let constraints = match feature.as_str() {
            "sleep-timer" => (
                data.sleep_timer_min,
                data.sleep_timer_max,
                data.sleep_timer_step,
            ),
            "auto-low-power-threshold" => (
                data.auto_low_power_threshold_min,
                data.auto_low_power_threshold_max,
                data.auto_low_power_threshold_step,
            ),
            "scroll-jump-delay" => (
                data.scroll_jump_delay_min,
                data.scroll_jump_delay_max,
                data.scroll_jump_delay_step,
            ),
            _ => return false,
        };
        let value =
            match parse_stepped_value(text.as_str(), constraints.0, constraints.1, constraints.2) {
                Ok(value) => value,
                Err(message) => {
                    ui.set_mutation_error(message.into());
                    return false;
                }
            };
        let action = match feature.as_str() {
            "sleep-timer" => {
                data.sleep_timer_edit = value;
                u64::try_from(value)
                    .ok()
                    .map(|minutes| MutationAction::PowerDraft {
                        sleep_timer: Some(minutes),
                        auto_low_power_threshold: None,
                    })
            }
            "auto-low-power-threshold" => {
                data.auto_low_power_threshold_edit = value;
                u8::try_from(value)
                    .ok()
                    .map(|percent| MutationAction::PowerDraft {
                        sleep_timer: None,
                        auto_low_power_threshold: Some(percent),
                    })
            }
            "scroll-jump-delay" => {
                data.scroll_jump_delay_edit = value;
                u16::try_from(value)
                    .ok()
                    .map(MutationAction::ScrollJumpDelay)
            }
            _ => return false,
        };
        ui.set_mutation_error(SharedString::default());
        ui.set_dashboard(data);
        drop(ui);
        let Some(action) = action else {
            return false;
        };
        start_mutation(edit_ui.clone(), edit_context.clone(), action);
        true
    });
}

fn start_mutation(ui: slint::Weak<AppWindow>, context: MutationContext, action: MutationAction) {
    if context
        .busy
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let Some(window) = ui.upgrade() else {
        context.busy.store(false, Ordering::Release);
        return;
    };
    let device_id = window.get_selected_id().to_string();
    let separate_axes = model_rows(&window.get_dpi_rows())
        .into_iter()
        .map(|row| row.separate)
        .collect::<Vec<_>>();
    window.set_mutation_busy(true);
    window.set_mutation_error(SharedString::default());
    if action.is_dpi_configuration() {
        window.set_dpi_error(SharedString::default());
    }
    drop(window);

    thread::spawn(move || {
        let result = action.execute_and_refresh(&context.client, &device_id);
        if let Err(failure) = &result {
            eprintln!("steelseries-linux-gui: {}", failure.error);
        }
        let description = context
            .description
            .lock()
            .ok()
            .and_then(|value| value.clone());
        let is_dpi = action.is_dpi_configuration();
        let attempted_action = action.clone();
        let busy = Arc::clone(&context.busy);
        let _ = ui.upgrade_in_event_loop(move |window| {
            busy.store(false, Ordering::Release);
            window.set_mutation_busy(false);
            if window.get_selected_id().as_str() != device_id
                || window.get_view_state() != "dashboard"
            {
                return;
            }
            match (result, description) {
                (Ok(snapshot), Some(description)) => {
                    window.set_mutation_error(SharedString::default());
                    window.set_dpi_error(SharedString::default());
                    apply_dashboard(&window, DashboardView::new(description, snapshot));
                    restore_dpi_separate_modes(&window, &separate_axes);
                }
                (Err(failure), description) => {
                    if let (Some(snapshot), Some(description)) =
                        (failure.persisted_snapshot, description)
                    {
                        apply_dashboard(&window, DashboardView::new(description, snapshot));
                        restore_dpi_separate_modes(&window, &separate_axes);
                    }
                    rollback_local_edit(&window, &attempted_action);
                    let mut message = failure.error.user_message();
                    if let Some(hint) = failure.error.validation_hint() {
                        message.push(' ');
                        message.push_str(&hint);
                    }
                    if is_dpi
                        || failure
                            .error
                            .field()
                            .is_some_and(|field| field.starts_with("stages"))
                    {
                        window.set_dpi_error(message.into());
                    } else {
                        window.set_mutation_error(message.into());
                    }
                }
                (Ok(_), None) => window.set_mutation_error(
                    "Device capability metadata is unavailable. Refresh the device.".into(),
                ),
            }
        });
    });
}

fn rollback_local_edit(window: &AppWindow, action: &MutationAction) {
    let mut data = window.get_dashboard();
    match action {
        MutationAction::ScrollJumpDelay(_) => {
            data.scroll_jump_delay_edit = data.scroll_jump_delay_current;
        }
        MutationAction::PowerDraft {
            sleep_timer,
            auto_low_power_threshold,
        } => {
            if sleep_timer.is_some() {
                data.sleep_timer_edit = data
                    .sleep_timer_current
                    .clamp(data.sleep_timer_min, data.sleep_timer_max);
            }
            if auto_low_power_threshold.is_some() {
                data.auto_low_power_threshold_edit = data.auto_low_power_threshold_current;
            }
        }
        _ => return,
    }
    window.set_dashboard(data);
}

fn can_add_stage(current: usize, max: i32) -> bool {
    usize::try_from(max).is_ok_and(|max| current < max)
}

fn model_rows(model: &ModelRc<DashboardRow>) -> Vec<DashboardRow> {
    (0..model.row_count())
        .filter_map(|index| model.row_data(index))
        .collect()
}

fn set_dpi_rows(ui: &AppWindow, rows: Vec<DashboardRow>) {
    ui.set_dpi_rows(Rc::new(VecModel::from(rows)).into());
}

fn restore_dpi_separate_modes(ui: &AppWindow, separate_axes: &[bool]) {
    if !ui.get_dashboard().dpi_independent_axes {
        return;
    }
    let mut rows = model_rows(&ui.get_dpi_rows());
    for (index, row) in rows.iter_mut().enumerate() {
        row.separate = dpi_separate_mode(
            row.x,
            row.y,
            separate_axes.get(index).copied().unwrap_or(false),
        );
    }
    set_dpi_rows(ui, rows);
}

const fn dpi_separate_mode(x: i32, y: i32, requested: bool) -> bool {
    x != y || requested
}

fn dpi_values(rows: &[DashboardRow]) -> Option<Vec<DpiValue>> {
    rows.iter()
        .map(|row| {
            Some(DpiValue {
                x: u16::try_from(row.x).ok()?,
                y: u16::try_from(row.y).ok()?,
            })
        })
        .collect()
}

fn new_dpi_stage(stage: usize, value: i32, curve: DpiCurve) -> DashboardRow {
    let position = u16::try_from(value).map_or(0.0, |value| curve.dpi_to_position(value) as f32);
    DashboardRow {
        label: format!("Stage {stage}").into(),
        value: format!("{value} DPI").into(),
        detail: "".into(),
        active: false,
        x: value,
        y: value,
        separate: false,
        raw_value: "".into(),
        persisted: false,
        option_index: 0,
        x_position: position,
        y_position: position,
        persisted_x: value,
        persisted_y: value,
    }
}

fn dpi_curve_from_dashboard(dashboard: &DashboardData) -> Option<DpiCurve> {
    DpiCurve::new(
        u16::try_from(dashboard.dpi_min).ok()?,
        u16::try_from(dashboard.dpi_max).ok()?,
        u16::try_from(dashboard.dpi_step).ok()?,
    )
    .ok()
}

fn update_dpi_axis_with_position(
    row: &mut DashboardRow,
    axis: &str,
    value: i32,
    position: f32,
) -> bool {
    if !update_dpi_axis(row, axis, value) {
        return false;
    }
    match axis {
        "x" => row.x_position = position,
        "y" => row.y_position = position,
        "both" => {
            row.x_position = position;
            row.y_position = position;
        }
        _ => return false,
    }
    true
}

fn dpi_row_matches_persisted(row: &DashboardRow) -> bool {
    row.x == row.persisted_x && row.y == row.persisted_y
}

fn dpi_change_requires_mutation(row: &DashboardRow, has_unsaved: bool) -> bool {
    row.persisted && !has_unsaved && !dpi_row_matches_persisted(row)
}

fn finalize_dpi_change(
    ui: &AppWindow,
    weak: slint::Weak<AppWindow>,
    context: MutationContext,
    index: usize,
    axis: &str,
    value: i32,
    curve: DpiCurve,
) {
    let mut rows = model_rows(&ui.get_dpi_rows());
    let has_unsaved = rows.iter().any(|row| !row.persisted);
    let Some(row) = rows.get_mut(index) else {
        return;
    };
    let position = u16::try_from(value).map_or(0.0, |value| curve.dpi_to_position(value) as f32);
    if !update_dpi_axis_with_position(row, axis, value, position) {
        return;
    }
    update_dpi_row_text(row);
    let unchanged = dpi_row_matches_persisted(row);
    if !dpi_change_requires_mutation(row, has_unsaved) {
        if row.persisted && !unchanged {
            row.persisted = false;
        }
        set_dpi_rows(ui, rows);
        ui.set_dpi_error(SharedString::default());
        return;
    }
    set_dpi_rows(ui, rows.clone());
    ui.set_dpi_error(SharedString::default());
    let Some(stages) = dpi_values(&rows) else {
        ui.set_dpi_error("DPI stage values are invalid.".into());
        return;
    };
    start_mutation(weak, context, MutationAction::Dpi(stages));
}

const fn dpi_stage_is_usable(row: &DashboardRow) -> bool {
    row.persisted
}

fn update_dpi_row_text(row: &mut DashboardRow) {
    row.value = if row.x == row.y {
        format!("{} DPI", row.x).into()
    } else {
        format!("X {} / Y {} DPI", row.x, row.y).into()
    };
}

fn update_dpi_axis(row: &mut DashboardRow, axis: &str, value: i32) -> bool {
    match axis {
        "x" => row.x = value,
        "y" => row.y = value,
        "both" => {
            row.x = value;
            row.y = value;
        }
        _ => return false,
    }
    true
}

fn set_separate_axes(row: &mut DashboardRow, separate: bool) {
    row.separate = separate;
    if !separate {
        row.y = row.x;
    }
}

fn parse_stepped_value(text: &str, min: i32, max: i32, step: i32) -> Result<i32, String> {
    let value = text
        .trim()
        .parse::<i32>()
        .map_err(|_| format!("Enter a whole number between {min} and {max}."))?;
    if snap_to_step(value, min, max, step) != Some(value) {
        return Err(format!(
            "Value must be between {min} and {max} in steps of {step}."
        ));
    }
    Ok(value)
}

fn snap_to_step(value: i32, min: i32, max: i32, step: i32) -> Option<i32> {
    if step <= 0 || min > max {
        return None;
    }
    let clamped = value.clamp(min, max);
    let offset = i64::from(clamped) - i64::from(min);
    let step = i64::from(step);
    let snapped = i64::from(min) + ((offset + step / 2) / step) * step;
    Some(i32::try_from(snapped.min(i64::from(max))).unwrap_or(max))
}

fn start_dashboard_load(
    ui: slint::Weak<AppWindow>,
    client: CliClient,
    device_id: String,
    generation: Arc<AtomicU64>,
    description_state: Arc<Mutex<Option<DeviceDescription>>>,
) {
    let request = generation.fetch_add(1, Ordering::Relaxed) + 1;
    if let Some(ui) = ui.upgrade() {
        ui.set_page_generation(ui.get_page_generation().wrapping_add(1));
        ui.set_error_message(SharedString::default());
        ui.set_mutation_error(SharedString::default());
        ui.set_dpi_error(SharedString::default());
        ui.set_view_state("device-loading".into());
    }

    thread::spawn(move || {
        let result = client.describe_device(&device_id).and_then(|description| {
            client
                .snapshot_device(&device_id)
                .map(|snapshot| (description, snapshot))
        });
        if let Err(error) = &result {
            eprintln!("steelseries-linux-gui: {error}");
        }

        let callback_generation = Arc::clone(&generation);
        let _ = ui.upgrade_in_event_loop(move |ui| {
            if callback_generation.load(Ordering::Relaxed) != request {
                return;
            }
            match result {
                Ok((description, snapshot)) => {
                    if let Ok(mut current) = description_state.lock() {
                        *current = Some(description.clone());
                    }
                    apply_dashboard(&ui, DashboardView::new(description, snapshot));
                }
                Err(error) => {
                    ui.set_error_message(error.device_message().into());
                    ui.set_view_state("device-error".into());
                }
            }
        });
    });
}

fn apply_monitor_event(ui: &AppWindow, event: MonitorEvent) {
    apply_settings(ui, &event.settings, &event.state);
    apply_devices(
        ui,
        event.state.active_devices.clone(),
        &event.state.batteries,
    );

    let view = ui.get_view_state();
    if let Some(error) = event.error {
        if matches!(view.as_str(), "loading" | "list" | "empty" | "error")
            && event.state.active_devices.is_empty()
        {
            ui.set_error_message(error.into());
            ui.set_view_state("error".into());
        }
        return;
    }
    ui.set_error_message(SharedString::default());
    if matches!(view.as_str(), "loading" | "list" | "empty" | "error") {
        ui.set_view_state(if event.state.active_devices.is_empty() {
            "empty".into()
        } else {
            "list".into()
        });
    }

    let selected_id = ui.get_selected_id();
    if view == "dashboard"
        && let Some(battery) = event.state.batteries.get(selected_id.as_str())
    {
        let mut dashboard = ui.get_dashboard();
        dashboard.battery_percentage = battery.percentage.map_or(-1, i32::from);
        dashboard.battery_charging = battery.charging == Some(true);
        dashboard.battery_icon =
            colored_battery_image(battery.percentage, dashboard.battery_charging);
        dashboard.battery = battery.percentage.map_or_else(
            || "Battery unavailable".into(),
            |percentage| {
                if dashboard.battery_charging {
                    format!("{percentage}% · Charging").into()
                } else {
                    format!("{percentage}%").into()
                }
            },
        );
        ui.set_dashboard(dashboard);
    }
}

fn update_tray(tray: &Mutex<TrayController>, event: &MonitorEvent) {
    let state = tray_state(event);
    if let Ok(mut tray) = tray.lock() {
        tray.set_enabled(event.settings.tray_enabled);
        tray.update(state);
    }
}

fn tray_state(event: &MonitorEvent) -> TrayState {
    let target = event.state.tray_target.as_ref().and_then(|target| {
        event
            .state
            .active_devices
            .iter()
            .find(|device| &device.id == target)
    });
    let battery = event.state.tray_battery();
    TrayState {
        device_name: target.map(|device| device.name.clone()),
        percentage: battery.and_then(|battery| battery.percentage),
        charging: battery.is_some_and(|battery| battery.charging == Some(true)),
    }
}

fn apply_settings(ui: &AppWindow, settings: &GuiSettings, state: &MonitorState) {
    ui.set_tray_enabled(settings.tray_enabled);
    ui.set_tray_icon_theme_index(settings.tray_icon_theme.index());
    ui.set_start_at_login(settings.start_at_login);
    ui.set_refresh_interval_index(
        REFRESH_INTERVAL_OPTIONS
            .iter()
            .position(|value| *value == settings.refresh_interval_seconds)
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(2),
    );
    let known = settings
        .device_priority
        .iter()
        .filter_map(|id| {
            settings
                .known_devices
                .iter()
                .find(|device| &device.id == id)
        })
        .map(|device| known_device_row(device, state))
        .collect::<Vec<_>>();
    ui.set_known_devices(Rc::new(VecModel::from(known)).into());
}

fn known_device_row(device: &KnownDevice, state: &MonitorState) -> KnownDeviceItem {
    let active = state
        .active_devices
        .iter()
        .find(|active| active.id == device.id);
    let connection = active.map_or(device.last_connection.as_str(), |active| {
        active.connection.as_str()
    });
    KnownDeviceItem {
        id: device.id.clone().into(),
        name: device.name.clone().into(),
        connection: connection_label(connection).into(),
        active: active.is_some(),
    }
}

fn apply_devices(ui: &AppWindow, devices: Vec<Device>, batteries: &HashMap<String, CachedBattery>) {
    let is_empty = devices.is_empty();
    let rows = devices
        .into_iter()
        .map(|device| {
            let battery = batteries.get(&device.id);
            device_row(device, battery)
        })
        .collect::<Vec<_>>();
    ui.set_devices(Rc::new(VecModel::from(rows)).into());
    if matches!(ui.get_view_state().as_str(), "loading" | "list" | "empty") {
        ui.set_view_state(if is_empty {
            "empty".into()
        } else {
            "list".into()
        });
    }
}

fn apply_dashboard(ui: &AppWindow, view: DashboardView) {
    ui.set_mutation_busy(false);
    ui.set_selected_name(view.device_name.clone().into());
    ui.set_selected_connection(view.connection.clone().into());
    ui.set_selected_connection_key(view.connection_key.clone().into());
    ui.set_selected_id(view.device_id.clone().into());
    ui.set_selected_model_key(view.model_key.clone().into());

    let dpi_curve = DpiCurve::new(view.dpi_min, view.dpi_max, view.dpi_step).ok();
    let dpi_rows = view
        .dpi_rows
        .iter()
        .map(|row| dashboard_row(row, dpi_curve))
        .collect::<Vec<_>>();
    ui.set_dpi_rows(Rc::new(VecModel::from(dpi_rows)).into());
    let references = dpi_curve.map_or_else(Vec::new, |curve| {
        curve
            .references()
            .into_iter()
            .map(|reference| DpiReferenceItem {
                value: i32::from(reference.value),
                label: reference.value.to_string().into(),
                tick_position: reference.position as f32,
                label_position: reference.label_position as f32,
            })
            .collect::<Vec<_>>()
    });
    ui.set_dpi_references(Rc::new(VecModel::from(references)).into());
    ui.set_lod_options(string_model(view.lod_options.clone()));
    ui.set_wireless_polling_options(string_model(view.wireless_polling_options.clone()));
    ui.set_wired_polling_options(string_model(view.wired_polling_options.clone()));
    ui.set_low_power_polling_options(string_model(view.low_power_polling_options.clone()));
    let device_id_label = identity_label(&view.device_id);
    ui.set_dashboard(DashboardData {
        device_name: view.device_name.into(),
        connection: view.connection.into(),
        connection_key: view.connection_key.into(),
        device_id: view.device_id.into(),
        device_id_label: device_id_label.into(),
        model_key: view.model_key.into(),
        show_battery: view.show_battery,
        battery: view.battery.into(),
        battery_percentage: view.battery_percentage.map_or(-1, i32::from),
        battery_charging: view.battery_charging,
        battery_icon: colored_battery_image(view.battery_percentage, view.battery_charging),
        show_dpi: view.show_dpi,
        active_dpi: view.active_dpi.into(),
        dpi_capabilities: view.dpi_capabilities.into(),
        dpi_independent_axes: view.dpi_independent_axes,
        dpi_min: i32::from(view.dpi_min),
        dpi_max: i32::from(view.dpi_max),
        dpi_step: i32::from(view.dpi_step),
        dpi_max_stages: i32::try_from(view.dpi_max_stages).unwrap_or(i32::MAX),
        show_lod: view.show_lod,
        active_lod: view.active_lod.into(),
        lod_capabilities: view.lod_capabilities.into(),
        show_polling: view.show_polling,
        show_wireless_polling: view.show_wireless_polling,
        show_wired_polling: view.show_wired_polling,
        wireless_polling: view.wireless_polling.into(),
        wired_polling: view.wired_polling.into(),
        wireless_polling_hz: i32::from(view.wireless_polling_hz),
        wired_polling_hz: i32::from(view.wired_polling_hz),
        wireless_polling_primary: view.wireless_polling_primary,
        wired_polling_primary: view.wired_polling_primary,
        wireless_polling_index: i32::try_from(view.wireless_polling_index).unwrap_or(0),
        wired_polling_index: i32::try_from(view.wired_polling_index).unwrap_or(0),
        polling_capabilities: view.polling_capabilities.into(),
        show_power: view.show_power,
        show_low_power_mode: view.show_low_power_mode,
        show_low_power_polling: view.show_low_power_polling,
        show_sleep_timer: view.show_sleep_timer,
        show_auto_low_power: view.show_auto_low_power,
        low_power_mode: view.low_power_mode.into(),
        low_power_enabled: view.low_power_enabled,
        low_power_polling: view.low_power_polling.into(),
        low_power_polling_hz: i32::from(view.low_power_polling_hz),
        low_power_polling_index: i32::try_from(view.low_power_polling_index).unwrap_or(0),
        sleep_timer: view.sleep_timer.into(),
        sleep_timer_edit: i32::try_from(view.sleep_timer_edit).unwrap_or(i32::MAX),
        sleep_timer_current: i32::try_from(view.sleep_timer_current).unwrap_or(i32::MAX),
        sleep_timer_min: i32::try_from(view.sleep_timer_min).unwrap_or(i32::MAX),
        sleep_timer_max: i32::try_from(view.sleep_timer_max).unwrap_or(i32::MAX),
        sleep_timer_step: i32::try_from(view.sleep_timer_step).unwrap_or(i32::MAX),
        auto_low_power: view.auto_low_power.into(),
        auto_low_power_enabled: view.auto_low_power_enabled,
        auto_low_power_threshold: view.auto_low_power_threshold.into(),
        auto_low_power_threshold_edit: i32::from(view.auto_low_power_threshold_edit),
        auto_low_power_threshold_current: i32::from(view.auto_low_power_threshold_current),
        auto_low_power_threshold_min: i32::from(view.auto_low_power_threshold_min),
        auto_low_power_threshold_max: i32::from(view.auto_low_power_threshold_max),
        auto_low_power_threshold_step: i32::from(view.auto_low_power_threshold_step),
        show_wireless_stability: view.show_wireless_stability,
        wireless_stability: view.wireless_stability.into(),
        wireless_stability_enabled: view.wireless_stability_enabled,
        show_bluetooth_smoothing: view.show_bluetooth_smoothing,
        bluetooth_smoothing: view.bluetooth_smoothing.into(),
        bluetooth_smoothing_enabled: view.bluetooth_smoothing_enabled,
        show_scroll_jump: view.show_scroll_jump,
        scroll_jump: view.scroll_jump.into(),
        scroll_jump_enabled: view.scroll_jump_enabled,
        scroll_jump_delay: view.scroll_jump_delay.into(),
        scroll_jump_delay_edit: i32::from(view.scroll_jump_delay_edit),
        scroll_jump_delay_current: i32::from(view.scroll_jump_delay_current),
        scroll_jump_capabilities: view.scroll_jump_capabilities.into(),
        scroll_jump_delay_min: i32::from(view.scroll_jump_delay_min),
        scroll_jump_delay_max: i32::from(view.scroll_jump_delay_max),
        scroll_jump_delay_step: i32::from(view.scroll_jump_delay_step),
        scroll_jump_delay_unit: view.scroll_jump_delay_unit.into(),
    });
    ui.set_view_state("dashboard".into());
}

fn dashboard_row(row: &dashboard::DashboardRow, curve: Option<DpiCurve>) -> DashboardRow {
    let x_position = curve.map_or(0.0, |curve| curve.dpi_to_position(row.x) as f32);
    let y_position = curve.map_or(0.0, |curve| curve.dpi_to_position(row.y) as f32);
    DashboardRow {
        label: row.label.clone().into(),
        value: row.value.clone().into(),
        detail: row.detail.clone().into(),
        active: row.active,
        x: i32::from(row.x),
        y: i32::from(row.y),
        separate: row.separate,
        raw_value: row.raw_value.clone().into(),
        persisted: row.persisted,
        option_index: i32::try_from(row.option_index).unwrap_or(0),
        x_position,
        y_position,
        persisted_x: i32::from(row.x),
        persisted_y: i32::from(row.y),
    }
}

fn string_model(values: Vec<String>) -> ModelRc<SharedString> {
    Rc::new(VecModel::from(
        values
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
    .into()
}

fn device_row(device: Device, battery: Option<&CachedBattery>) -> DeviceItem {
    let identity_label = identity_label(&device.id);
    DeviceItem {
        name: device.name.into(),
        connection: connection_label(&device.connection).into(),
        connection_key: device.connection.into(),
        model_key: device.model_key.into(),
        id: device.id.into(),
        identity_label: identity_label.into(),
        battery_available: battery.and_then(|battery| battery.percentage).is_some(),
        battery_percentage: battery
            .and_then(|battery| battery.percentage)
            .map_or(0, i32::from),
        battery_charging: battery.is_some_and(|battery| battery.charging == Some(true)),
        battery_icon: colored_battery_image(
            battery.and_then(|battery| battery.percentage),
            battery.is_some_and(|battery| battery.charging == Some(true)),
        ),
    }
}

fn identity_label(identity: &str) -> String {
    format!("ID: {identity}")
}

fn connection_label(connection: &str) -> &str {
    match connection {
        "wired" => "Wired",
        "2.4_ghz" => "2.4 GHz",
        value => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dpi_curve() -> DpiCurve {
        DpiCurve::new(50, 26_000, 50).unwrap()
    }

    #[test]
    fn autostart_launch_is_hidden_and_enables_tray() {
        let settings = GuiSettings {
            tray_enabled: false,
            ..GuiSettings::default()
        };
        assert!(!initial_window_visible(true));
        assert!(settings_for_launch(settings, true).tray_enabled);
    }

    #[test]
    fn normal_launch_shows_window_and_preserves_tray_preference() {
        let settings = GuiSettings {
            tray_enabled: false,
            ..GuiSettings::default()
        };
        assert!(initial_window_visible(false));
        assert!(!settings_for_launch(settings, false).tray_enabled);
    }

    #[test]
    fn connection_labels_preserve_unknown_values() {
        assert_eq!(connection_label("wired"), "Wired");
        assert_eq!(connection_label("2.4_ghz"), "2.4 GHz");
        assert_eq!(connection_label("future_mode"), "future_mode");
    }

    #[test]
    fn device_row_preserves_identity_model_and_raw_connection() {
        let row = device_row(
            Device {
                connection: "wired".to_owned(),
                id: "AAA".to_owned(),
                model_key: "aerox_3_wireless_gen2".to_owned(),
                name: "Aerox 3 Wireless Gen 2".to_owned(),
            },
            None,
        );
        assert_eq!(row.connection, "Wired");
        assert_eq!(row.connection_key, "wired");
        assert_eq!(row.id, "AAA");
        assert_eq!(row.identity_label, "ID: AAA");
        assert!(!row.identity_label.contains("aerox_3_wireless_gen2"));
        assert_eq!(row.model_key, "aerox_3_wireless_gen2");
        assert_eq!(row.name, "Aerox 3 Wireless Gen 2");
    }

    #[test]
    fn tray_and_device_row_derive_battery_from_the_same_monitor_cache() {
        let device = Device {
            connection: "wired".to_owned(),
            id: "AAA".to_owned(),
            model_key: "aerox_3_wireless_gen2".to_owned(),
            name: "Aerox 3 Wireless Gen 2".to_owned(),
        };
        let battery = CachedBattery {
            percentage: Some(88),
            charging: Some(true),
        };
        let mut state = MonitorState {
            active_devices: vec![device.clone()],
            tray_target: Some(device.id.clone()),
            ..MonitorState::default()
        };
        state.batteries.insert(device.id.clone(), battery.clone());
        let event = MonitorEvent {
            settings: GuiSettings::default(),
            state,
            error: None,
        };

        let tray = tray_state(&event);
        let row = device_row(device, Some(&battery));

        assert_eq!(tray.percentage, Some(88));
        assert!(tray.charging);
        assert_eq!(row.battery_percentage, 88);
        assert!(row.battery_charging);
    }

    #[test]
    fn dpi_stage_limit_and_capability_steps_are_enforced_locally() {
        assert!(can_add_stage(4, 5));
        assert!(!can_add_stage(5, 5));
        assert!(!can_add_stage(1, -1));
        assert_eq!(parse_stepped_value("400", 50, 26_000, 50), Ok(400));
        assert_eq!(parse_stepped_value("26000", 50, 26_000, 50), Ok(26_000));
        assert!(parse_stepped_value("425", 50, 26_000, 50).is_err());
        assert!(parse_stepped_value("26050", 50, 26_000, 50).is_err());
    }

    #[test]
    fn sleep_timer_gui_validation_uses_recommended_range() {
        assert_eq!(parse_stepped_value("5", 5, 20, 1), Ok(5));
        assert_eq!(parse_stepped_value("20", 5, 20, 1), Ok(20));
        let error = parse_stepped_value("30", 5, 20, 1).unwrap_err();
        assert!(error.contains("between 5 and 20"));
    }

    #[test]
    fn generic_slider_preview_replaces_stale_text_and_commits_once() {
        let mut current = 5;
        let mut edit_buffer = current.to_string();
        let mut textbox_commits = 0;
        let mut slider_release_commits = 0;
        assert_eq!(edit_buffer, "5");

        current = snap_to_step(10, 0, 20, 1).unwrap();
        edit_buffer = current.to_string();
        assert_eq!(edit_buffer, "10");

        current = parse_stepped_value("5", 0, 20, 1).unwrap();
        edit_buffer = current.to_string();
        textbox_commits += 1;
        assert_eq!(current, 5);
        assert_eq!(edit_buffer, "5");

        for raw in [6, 7] {
            current = snap_to_step(raw, 0, 20, 1).unwrap();
            edit_buffer = current.to_string();
            assert_eq!(edit_buffer, raw.to_string());
        }
        slider_release_commits += 1;
        assert_eq!(textbox_commits, 1);
        assert_eq!(slider_release_commits, 1);
    }

    #[test]
    fn generic_stepped_slider_preview_uses_snapped_text() {
        assert_eq!(snap_to_step(12, 0, 20, 5).unwrap().to_string(), "10");
        assert_eq!(snap_to_step(14, 0, 20, 5).unwrap().to_string(), "15");
    }

    #[test]
    fn dpi_slider_values_snap_to_capability_steps() {
        assert_eq!(snap_to_step(50, 50, 26_000, 50), Some(50));
        assert_eq!(snap_to_step(423, 50, 26_000, 50), Some(400));
        assert_eq!(snap_to_step(425, 50, 26_000, 50), Some(450));
        assert_eq!(snap_to_step(26_031, 50, 26_000, 50), Some(26_000));
        assert_eq!(snap_to_step(400, 50, 26_000, 0), None);
        assert_eq!(snap_to_step(1573, 50, 26_000, 50), Some(1550));
        assert_eq!(snap_to_step(1584, 50, 26_000, 50), Some(1600));
    }

    #[test]
    fn dpi_drag_preview_keeps_only_snapped_values_and_updates_axes_independently() {
        let curve = test_dpi_curve();
        let mut row = new_dpi_stage(1, 800, test_dpi_curve());
        row.persisted = true;
        row.persisted_x = 800;
        row.persisted_y = 1600;
        row.separate = true;
        row.y = 1600;

        let x = i32::from(curve.snap(1_573.0));
        assert!(update_dpi_axis(&mut row, "x", x));
        assert_eq!((row.x, row.y), (1550, 1600));
        assert!(row.persisted);

        let y = i32::from(curve.snap(1_584.0));
        assert!(update_dpi_axis(&mut row, "y", y));
        assert_eq!((row.x, row.y), (1550, 1600));
        assert!(row.persisted);
        assert!(!update_dpi_axis(&mut row, "invalid", 400));
    }

    #[test]
    fn newly_added_dpi_stage_is_unavailable_until_apply_finishes() {
        let row = new_dpi_stage(3, 1600, test_dpi_curve());
        assert_eq!(row.label, "Stage 3");
        assert!(!row.persisted);
        assert!(!dpi_stage_is_usable(&row));
    }

    #[test]
    fn persisted_dpi_stage_is_immediately_usable() {
        let mut row = new_dpi_stage(1, 800, test_dpi_curve());
        row.persisted = true;
        assert!(dpi_stage_is_usable(&row));
    }

    #[test]
    fn dpi_mutation_is_skipped_for_unchanged_and_unsaved_values() {
        let mut row = new_dpi_stage(1, 800, test_dpi_curve());
        row.persisted = true;
        assert!(!dpi_change_requires_mutation(&row, false));

        row.x = 1_600;
        row.y = 1_600;
        assert!(dpi_change_requires_mutation(&row, false));
        assert!(!dpi_change_requires_mutation(&row, true));

        row.persisted = false;
        assert!(!dpi_change_requires_mutation(&row, false));
    }

    #[test]
    fn dpi_reference_values_set_exact_effective_values_when_in_range() {
        let curve = test_dpi_curve();
        let references = curve.references();
        for expected in [50, 400, 800, 1_600, 3_200, 26_000] {
            let reference = references
                .iter()
                .find(|reference| reference.value == expected)
                .unwrap();
            assert_eq!(curve.position_to_dpi(reference.position), expected);
        }
    }

    #[test]
    fn disabling_separate_axes_unifies_y_with_x_before_automatic_write() {
        let mut row = new_dpi_stage(1, 800, test_dpi_curve());
        row.persisted = true;
        row.separate = true;
        row.y = 1600;
        set_separate_axes(&mut row, false);
        assert!(!row.separate);
        assert_eq!((row.x, row.y), (800, 800));
    }

    #[test]
    fn separate_axes_presentation_survives_equal_axis_snapshots() {
        assert!(dpi_separate_mode(800, 800, true));
        assert!(dpi_separate_mode(800, 1600, false));
        assert!(!dpi_separate_mode(800, 800, false));
    }
}
