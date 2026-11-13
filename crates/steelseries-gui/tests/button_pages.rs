//! Render the actual application views with local data; no backend or devices are used.
use std::{cell::Cell, rc::Rc};

use slint::{ComponentHandle, ModelRc, VecModel};

slint::slint! {
    export { AppWindow, DashboardData, DashboardRow, DeviceItem, KnownDeviceItem }
        from "../ui/app-window.slint";
}

const VIEWS: [&str; 6] = [
    "list",
    "settings",
    "dashboard",
    "empty",
    "error",
    "device-error",
];

fn populated_window() -> AppWindow {
    let ui = AppWindow::new().unwrap();
    let battery_icon = slint::Image::load_from_svg_data(include_bytes!(
        "../../../assets/battery/colored/normal/battery-050.svg"
    ))
    .unwrap();
    ui.set_fixed_window_width(1040);
    ui.set_fixed_window_height(720);
    ui.set_error_message("Example read-only error state".into());
    ui.set_devices(ModelRc::new(VecModel::from(vec![DeviceItem {
        name: "Example device".into(),
        connection: "Wireless".into(),
        identity_label: "Preview device".into(),
        battery_available: true,
        battery_percentage: 50,
        battery_icon: battery_icon.clone(),
        ..Default::default()
    }])));
    ui.set_known_devices(ModelRc::new(VecModel::from(vec![
        KnownDeviceItem {
            name: "First device".into(),
            connection: "Wireless".into(),
            active: true,
            ..Default::default()
        },
        KnownDeviceItem {
            name: "Second device".into(),
            ..Default::default()
        },
    ])));
    ui.set_dashboard(DashboardData {
        device_name: "Example device".into(),
        device_id_label: "Read-only button preview".into(),
        connection: "Wireless".into(),
        show_battery: true,
        battery: "50%".into(),
        battery_percentage: 50,
        battery_icon,
        show_dpi: true,
        active_dpi: "800 DPI".into(),
        dpi_min: 50,
        dpi_max: 26000,
        dpi_step: 50,
        dpi_max_stages: 5,
        dpi_independent_axes: true,
        show_power: true,
        show_sleep_timer: true,
        sleep_timer_edit: 5,
        sleep_timer_min: 1,
        sleep_timer_max: 20,
        sleep_timer_step: 1,
        show_low_power_mode: true,
        show_auto_low_power: true,
        auto_low_power_threshold_edit: 10,
        auto_low_power_threshold_min: 5,
        auto_low_power_threshold_max: 25,
        auto_low_power_threshold_step: 1,
        show_wireless_stability: true,
        wireless_stability_enabled: true,
        show_bluetooth_smoothing: true,
        show_scroll_jump: true,
        scroll_jump_enabled: true,
        scroll_jump_delay_min: 100,
        scroll_jump_delay_max: 1500,
        scroll_jump_delay_step: 100,
        scroll_jump_delay_edit: 500,
        ..Default::default()
    });
    ui.set_dpi_rows(ModelRc::new(VecModel::from(vec![
        DashboardRow {
            label: "Stage 1".into(),
            x: 800,
            y: 800,
            x_position: 0.3,
            ..Default::default()
        },
        DashboardRow {
            label: "Stage 2".into(),
            x: 1600,
            y: 1600,
            persisted: true,
            x_position: 0.5,
            ..Default::default()
        },
    ])));
    ui
}

#[test]
fn application_button_contexts_render_with_local_state() {
    // Opt-in graphical review captures only the application window, never the desktop.
    if let Some(directory) = std::env::var_os("STEELSERIES_BUTTON_PAGE_SNAPSHOTS") {
        let ui = populated_window();
        ui.set_view_state(VIEWS[0].into());
        let index = Cell::new(0);
        let weak = ui.as_weak();
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(1),
            move || {
                let ui = weak.upgrade().unwrap();
                let image = ui.window().take_snapshot().unwrap();
                let mut ppm =
                    format!("P6\n{} {}\n255\n", image.width(), image.height()).into_bytes();
                for pixel in image.as_slice() {
                    ppm.extend_from_slice(&[pixel.r, pixel.g, pixel.b]);
                }
                std::fs::write(
                    std::path::Path::new(&directory).join(format!("{}.ppm", VIEWS[index.get()])),
                    ppm,
                )
                .unwrap();
                index.set(index.get() + 1);
                if index.get() == VIEWS.len() {
                    slint::quit_event_loop().unwrap();
                } else {
                    ui.set_view_state(VIEWS[index.get()].into());
                }
            },
        );
        ui.run().unwrap();
        return;
    }

    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    struct TestPlatform {
        window: Rc<MinimalSoftwareWindow>,
        time: Rc<Cell<std::time::Duration>>,
    }
    impl slint::platform::Platform for TestPlatform {
        fn create_window_adapter(
            &self,
        ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
            Ok(self.window.clone())
        }

        fn duration_since_start(&self) -> std::time::Duration {
            self.time.get()
        }
    }
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    let time = Rc::new(Cell::new(std::time::Duration::ZERO));
    slint::platform::set_platform(Box::new(TestPlatform {
        window: window.clone(),
        time: time.clone(),
    }))
    .unwrap();
    let ui = populated_window();
    ui.window().set_size(slint::PhysicalSize::new(1040, 720));
    let apply_calls = Rc::new(Cell::new(0));
    let remove_calls = Rc::new(Cell::new(0));
    let apply_counter = apply_calls.clone();
    ui.on_dpi_apply_requested(move |_| apply_counter.set(apply_counter.get() + 1));
    let remove_counter = remove_calls.clone();
    ui.on_dpi_remove_requested(move |_| remove_counter.set(remove_counter.get() + 1));
    ui.show().unwrap();
    for state in VIEWS {
        ui.set_view_state(state.into());
        for busy in [false, true] {
            ui.set_mutation_busy(busy);
            window.request_redraw();
            let mut rendered = false;
            let mut dpi_actions = None;
            window.draw_if_needed(|renderer| {
                let mut pixels = vec![slint::Rgb8Pixel::default(); 1040 * 720];
                renderer.render(&mut pixels, 1040);
                assert!(
                    pixels.iter().any(|pixel| pixel.r > 100),
                    "missing content in {state}"
                );
                if state == "dashboard" && !busy {
                    let value_center = |left: usize, right: usize| {
                        let ys: Vec<_> = (170..215)
                            .filter(|&y| {
                                (left..right).any(|x| {
                                    let p = pixels[y * 1040 + x];
                                    p.r >= 190 && p.r.abs_diff(p.g) <= 2 && p.r.abs_diff(p.b) <= 2
                                })
                            })
                            .collect();
                        (ys.first().unwrap() + ys.last().unwrap()) as f32 / 2.
                    };
                    let connection = value_center(38, 320);
                    let battery = value_center(410, 680);
                    let dpi = value_center(710, 1010);
                    assert!(
                        (connection - battery).abs() <= 2.,
                        "battery value must align: connection={connection}, battery={battery}, dpi={dpi}"
                    );
                    assert!(
                        (dpi - battery).abs() <= 2.,
                        "battery value must align: connection={connection}, battery={battery}, dpi={dpi}"
                    );
                    for (y, row) in pixels.as_chunks::<1040>().0.iter().enumerate() {
                        let run = |color| {
                            let mut result = None;
                            let mut start = None;
                            for (x, pixel) in row.iter().enumerate() {
                                if (pixel.r, pixel.g, pixel.b) == color {
                                    start.get_or_insert(x);
                                } else if let Some(begin) = start.take()
                                    && x - begin >= 25
                                {
                                    result = Some((begin, x - begin));
                                    break;
                                }
                            }
                            result
                        };
                        if let (Some((orange_x, orange_width)), Some((red_x, red_width))) =
                            (run((232, 117, 36)), run((183, 53, 46)))
                            && orange_width.abs_diff(red_width) <= 2
                            && red_x > orange_x
                        {
                            dpi_actions =
                                Some((orange_x + orange_width / 2, red_x + red_width / 2, y));
                            break;
                        }
                    }
                    assert!(
                        dpi_actions.is_some(),
                        "equal-width orange and red DPI actions must render"
                    );
                }
                if matches!(state, "empty" | "error") {
                    let expected = if state == "error" {
                        (232, 117, 36)
                    } else {
                        (36, 36, 36)
                    };
                    let xs: Vec<_> = pixels
                        .as_chunks::<1040>()
                        .0
                        .iter()
                        .flat_map(|row| {
                            // Locate broad areas of button fill, excluding text antialiasing.
                            row.windows(20).enumerate().filter_map(move |(x, run)| {
                                run.iter()
                                    .all(|pixel| (pixel.r, pixel.g, pixel.b) == expected)
                                    .then_some(x + 10)
                            })
                        })
                        .collect();
                    let center = (xs.iter().min().unwrap() + xs.iter().max().unwrap()) / 2;
                    assert!(
                        (500..=540).contains(&center),
                        "action must remain centered in {state}"
                    );
                }
                rendered = true;
            });
            assert!(rendered, "view did not render: {state}");
            if let Some((apply_x, remove_x, y)) = dpi_actions {
                use slint::platform::{PointerEventButton, WindowEvent};
                for x in [apply_x, remove_x] {
                    let position = slint::LogicalPosition::new(x as f32, y as f32);
                    ui.window().dispatch_event(WindowEvent::PointerPressed {
                        position,
                        button: PointerEventButton::Left,
                    });
                    ui.window().dispatch_event(WindowEvent::PointerReleased {
                        position,
                        button: PointerEventButton::Left,
                    });
                }
                assert_eq!(apply_calls.get(), 1, "Apply callback must remain connected");
                assert_eq!(
                    remove_calls.get(),
                    1,
                    "Remove callback must remain connected"
                );
            }
        }
    }

    // Polling is the last visible card for this device profile. Scrolling must
    // stop at that card even as the number of DPI stages changes.
    use slint::platform::WindowEvent;
    ui.set_view_state("dashboard".into());
    ui.set_mutation_busy(false);
    let mut dashboard = ui.get_dashboard();
    dashboard.show_polling = true;
    dashboard.show_wireless_polling = true;
    dashboard.show_wired_polling = true;
    dashboard.show_lod = true;
    dashboard.show_low_power_polling = true;
    ui.set_dashboard(dashboard.clone());
    ui.set_low_power_polling_options(ModelRc::new(VecModel::from(vec![
        "125 Hz".into(),
        "250 Hz".into(),
        "500 Hz".into(),
    ])));
    ui.set_lod_options(ModelRc::new(VecModel::from(vec![
        "Low".into(),
        "High".into(),
    ])));
    ui.set_wireless_polling_options(ModelRc::new(VecModel::from(vec![
        "125 Hz".into(),
        "1000 Hz".into(),
    ])));
    ui.set_wired_polling_options(ModelRc::new(VecModel::from(vec![
        "125 Hz".into(),
        "1000 Hz".into(),
    ])));
    let stages = |count: usize| {
        ModelRc::new(VecModel::from(
            (0..count)
                .map(|i| DashboardRow {
                    label: format!("Stage {}", i + 1).into(),
                    x: 800,
                    y: 800,
                    persisted: true,
                    x_position: 0.3,
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        ))
    };
    let render_dashboard = || {
        let mut pixels = vec![slint::Rgb8Pixel::default(); 1040 * 720];
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 1040);
        });
        pixels
    };
    let scroll_down = || {
        ui.window().dispatch_event(WindowEvent::PointerScrolled {
            position: slint::LogicalPosition::new(35., 500.),
            delta_x: 0.,
            delta_y: -5000.,
        });
        // Mouse wheel scrolling animates for 180ms; advance the test clock so
        // each assertion inspects the settled position, without sleeping.
        time.set(time.get() + std::time::Duration::from_millis(250));
        slint::platform::update_timers_and_animations();
    };
    // Match the connected device's two stages and supported panel sections.
    ui.set_dpi_rows(stages(2));
    render_dashboard();
    scroll_down();
    let device_bottom = render_dashboard();
    assert!(
        device_bottom[680 * 1040 + 40].r > 20 || device_bottom[680 * 1040 + 500].r > 20,
        "one of the final cards must reach the viewport bottom for the connected device profile"
    );
    scroll_down();
    let device_after = render_dashboard();
    assert!(
        device_bottom
            .as_chunks::<1040>()
            .0
            .iter()
            .zip(device_after.as_chunks::<1040>().0.iter())
            .all(|(a, b)| a[26..1010] == b[26..1010]),
        "the connected device profile must stop at the final card"
    );
    dashboard.show_power = false;
    dashboard.show_wireless_stability = false;
    dashboard.show_bluetooth_smoothing = false;
    dashboard.show_scroll_jump = false;
    dashboard.show_lod = false;
    ui.set_dashboard(dashboard);
    for count in [3, 5] {
        ui.set_dpi_rows(stages(count));
        let top = render_dashboard();
        scroll_down();
        let bottom = render_dashboard();
        assert!(
            top != bottom,
            "long dashboard content must remain scrollable"
        );
        let last_card = bottom[680 * 1040 + 40];
        assert!(
            last_card.r > 20,
            "the final Polling card must be near the viewport bottom (stages={count}, color={last_card:?})"
        );
        scroll_down();
        let next = render_dashboard();
        let unchanged = bottom
            .as_chunks::<1040>()
            .0
            .iter()
            .zip(next.as_chunks::<1040>().0.iter())
            .all(|(a, b)| a[26..1010] == b[26..1010]);
        assert!(
            unchanged,
            "scrolling past Polling must not reveal empty space"
        );
    }
    ui.set_dpi_rows(stages(1));
    render_dashboard();
    scroll_down();
    let short_bottom = render_dashboard();
    scroll_down();
    let short_after = render_dashboard();
    assert!(
        short_bottom
            .as_chunks::<1040>()
            .0
            .iter()
            .zip(short_after.as_chunks::<1040>().0.iter())
            .all(|(a, b)| a[26..1010] == b[26..1010]),
        "scroll position must stay in bounds when stages are removed"
    );
    ui.hide().unwrap();
}
