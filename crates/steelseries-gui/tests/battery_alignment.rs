use slint::{ComponentHandle, platform::software_renderer::MinimalSoftwareWindow};
use std::{path::PathBuf, rc::Rc};

slint::slint! {
    import { BatteryIndicator } from "../ui/components/battery-indicator.slint";
    export component BatteryFixture inherits Window {
        width: 400px;
        height: 140px;
        background: #181818;
        in property <image> battery-icon;
        in property <string> text: "1%";
        BatteryIndicator { x: 16px; y: 16px; width: 360px; height: 40px; icon: root.battery-icon; text: root.text; }
        BatteryIndicator { x: 16px; y: 80px; width: 360px; height: 30px; icon: root.battery-icon; text: root.text; text-size: 12px; text-weight: 400; text-color: #cfcfcf; }
    }
}

fn asset(style: &str, level: u8, charging: bool) -> slint::Image {
    let state = if charging { "charging" } else { "normal" };
    let suffix = if charging { "-charging" } else { "" };
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../assets/battery/{style}/{state}/battery-{level:03}{suffix}.svg"
    ));
    slint::Image::load_from_path(&path).unwrap()
}

struct Platform(Rc<MinimalSoftwareWindow>);
impl slint::platform::Platform for Platform {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

#[test]
fn battery_icon_stays_centered_across_assets_and_text_widths() {
    if let Some(directory) = std::env::var_os("STEELSERIES_BATTERY_SNAPSHOTS") {
        let ui = BatteryFixture::new().unwrap();
        ui.set_battery_icon(asset("colored", 0, false));
        let weak = ui.as_weak();
        let index = std::cell::Cell::new(0usize);
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_secs(1),
            move || {
                let ui = weak.upgrade().unwrap();
                let image = ui.window().take_snapshot().unwrap();
                let mut ppm =
                    format!("P6\n{} {}\n255\n", image.width(), image.height()).into_bytes();
                for p in image.as_slice() {
                    ppm.extend_from_slice(&[p.r, p.g, p.b]);
                }
                std::fs::write(
                    PathBuf::from(&directory).join(format!("battery-{}.ppm", index.get())),
                    ppm,
                )
                .unwrap();
                index.set(index.get() + 1);
                if index.get() == 6 {
                    slint::quit_event_loop().unwrap();
                } else {
                    let percentage = [1, 50, 100][index.get() % 3];
                    ui.set_battery_icon(asset(
                        "colored",
                        if percentage == 1 { 0 } else { percentage },
                        index.get() >= 3,
                    ));
                    ui.set_text(format!("{percentage}%").into());
                }
            },
        );
        ui.run().unwrap();
        return;
    }
    let window = MinimalSoftwareWindow::new(
        slint::platform::software_renderer::RepaintBufferType::NewBuffer,
    );
    slint::platform::set_platform(Box::new(Platform(window.clone()))).unwrap();
    let ui = BatteryFixture::new().unwrap();
    ui.window().set_size(slint::PhysicalSize::new(400, 140));
    ui.show().unwrap();
    for style in ["colored", "monochrome"] {
        for charging in [false, true] {
            ui.hide().unwrap();
            ui.show().unwrap();
            for level in (0..=100).step_by(10) {
                ui.set_battery_icon(asset(style, level, charging));
                ui.set_text(
                    format!(
                        "{}%{}",
                        if level == 0 { 1 } else { level },
                        if charging { " · Charging" } else { "" }
                    )
                    .into(),
                );
                let mut pixels = vec![slint::Rgb8Pixel::default(); 400 * 140];
                window.request_redraw();
                window.draw_if_needed(|renderer| {
                    renderer.render(&mut pixels, 400);
                });
                for (top, height) in [(16usize, 40usize), (80, 30)] {
                    let visible_y: Vec<_> = (top..top + height)
                        .filter(|&y| {
                            (16..48).any(|x| {
                                let p = pixels[y * 400 + x];
                                (p.r, p.g, p.b) != (24, 24, 24)
                            })
                        })
                        .collect();
                    let min = *visible_y.first().expect("battery icon must render");
                    let max = *visible_y.last().unwrap();
                    assert!(
                        (min + max + 1).abs_diff(2 * top + height) <= 2,
                        "icon center shifted for {style}, {level}, charging={charging}"
                    );
                    let text_y: Vec<_> = (top..top + height)
                        .filter(|&y| {
                            (56..376).any(|x| {
                                let p = pixels[y * 400 + x];
                                (p.r, p.g, p.b) != (24, 24, 24)
                            })
                        })
                        .collect();
                    let text_center_twice = text_y.first().unwrap() + text_y.last().unwrap() + 1;
                    assert!(
                        text_center_twice.abs_diff(2 * top + height) <= 6,
                        "percentage text must remain centered alongside its icon"
                    );
                }
            }
        }
    }
    ui.hide().unwrap();
}
