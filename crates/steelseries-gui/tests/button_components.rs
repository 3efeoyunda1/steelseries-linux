use std::rc::Rc;

use slint::{ComponentHandle, Model, platform::software_renderer::MinimalSoftwareWindow};

slint::slint! {
    import { AppButton, ButtonSize, ButtonVariant } from "../ui/components/app-button.slint";
    import { ToggleControl } from "../ui/components/toggle-control.slint";
    import { ButtonMetrics } from "../ui/theme.slint";

    export component ButtonFixture inherits Window {
        title: "SteelSeries button component validation";
        width: 640px;
        height: 420px;
        background: #181818;
        in-out property <bool> controls-enabled: true;
        in-out property <bool> toggle-value: false;
        in-out property <int> click-count: 0;
        in property <string> natural-text: "Add Stage";
        in property <length> action-row-width: 608px;
        out property <[length]> widths: [natural.width, toolbar.width, large.width,
            priority.width, apply.width, use-stage.width, remove.width, add-stage.width];
        out property <length> actions-width: actions.width;
        out property <length> action-gap-one: use-stage.x - (apply.x + apply.width);
        out property <length> action-gap-two: remove.x - (use-stage.x + use-stage.width);
        out property <length> add-row-width: add-row.width;
        out property <[length]> heights: [natural.height, toolbar.height, large.height];
        out property <[length]> metrics: [ButtonMetrics.compact-height, ButtonMetrics.compact-min-width,
            ButtonMetrics.compact-horizontal-padding, ButtonMetrics.toolbar-height,
            ButtonMetrics.toolbar-width, ButtonMetrics.large-height, ButtonMetrics.large-min-width,
            ButtonMetrics.large-horizontal-padding, ButtonMetrics.toggle-width,
            ButtonMetrics.priority-arrow-width];
        out property <length> toggle-right: toggle.x + toggle.width;
        out property <length> toggle-y: toggle.y + toggle.height / 2;
        out property <length> second-toggle-right: second-toggle.x + second-toggle.width;
        out property <brush> neutral-background: natural.background;
        out property <brush> toolbar-background: toolbar.background;
        out property <brush> toolbar-border: toolbar.border-color;
        out property <brush> large-neutral-background: back.background;
        out property <length> toolbar-x: first-row.x + toolbar.x + toolbar.width / 2;
        out property <length> toolbar-y: first-row.y + toolbar.y + toolbar.height / 2;
        out property <length> back-x: large-row.x + back.x + back.width / 2;
        out property <length> back-y: large-row.y + back.y + back.height / 2;
        out property <length> danger-x: actions.x + remove.x + remove.width / 2;
        out property <length> danger-y: actions.y + remove.y + remove.height / 2;
        out property <brush> accent-background: large.background;
        out property <brush> danger-background: remove.background;
        out property <length> danger-border-width: remove.border-width;
        out property <float> danger-opacity: remove.opacity;
        out property <float> compact-opacity: natural.opacity;
        out property <float> large-opacity: large.opacity;
        out property <length> click-x: first-row.x + natural.x + natural.width / 2;
        out property <length> click-y: first-row.y + natural.y + natural.height / 2;
        out property <length> accent-x: large-row.x + large.x + large.width / 2;
        out property <length> accent-y: large-row.y + large.y + large.height / 2;
        VerticalLayout {
            padding: 16px;
            spacing: 12px;
            first-row := HorizontalLayout {
                natural := AppButton {
                    text: root.natural-text;
                    enabled: root.controls-enabled;
                    clicked => { root.click-count += 1; }
                }
                Rectangle { horizontal-stretch: 1; }
                toolbar := AppButton { size: ButtonSize.toolbar; variant: ButtonVariant.toolbar; text: "Refresh"; }
            }
            large-row := HorizontalLayout {
                large := AppButton {
                    size: ButtonSize.large; variant: ButtonVariant.accent; text: "Retry";
                    enabled: root.controls-enabled;
                }
                back := AppButton { size: ButtonSize.large; text: "Back to Devices"; }
            }
            toggle := ToggleControl {
                label: "Short label"; value: root.toggle-value; enabled: root.controls-enabled;
                toggled => { root.toggle-value = !root.toggle-value; }
            }
            second-toggle := ToggleControl { label: "A much longer setting label"; value: !root.toggle-value; }
            HorizontalLayout {
                Rectangle { horizontal-stretch: 1; }
                priority := AppButton { width: ButtonMetrics.priority-arrow-width; text: "↑"; }
                AppButton { width: ButtonMetrics.priority-arrow-width; text: "↓"; enabled: false; }
            }
            actions := HorizontalLayout {
                width: root.action-row-width;
                spacing: 10px;
                apply := AppButton { fill: true; variant: ButtonVariant.accent; text: "Apply"; }
                use-stage := AppButton { fill: true; variant: ButtonVariant.accent; text: "Use"; }
                remove := AppButton { fill: true; variant: ButtonVariant.danger; text: "Remove"; enabled: root.controls-enabled; clicked => { root.click-count += 1; } }
            }
            add-row := HorizontalLayout {
                width: root.action-row-width;
                add-stage := AppButton { fill: true; text: "Add Stage"; }
            }
        }
    }
}

struct ButtonTestPlatform(Rc<MinimalSoftwareWindow>);

impl slint::platform::Platform for ButtonTestPlatform {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

#[test]
fn buttons_preserve_layout_state_and_interaction_contracts() {
    use slint::platform::{PointerEventButton, WindowEvent, software_renderer::RepaintBufferType};

    // Optional manual preview uses the real window backend and never contacts a device.
    if std::env::var_os("STEELSERIES_BUTTON_PREVIEW").is_some() {
        let preview = ButtonFixture::new().unwrap();
        if let Some(path) = std::env::var_os("STEELSERIES_BUTTON_SNAPSHOT") {
            let weak = preview.as_weak();
            slint::Timer::single_shot(std::time::Duration::from_secs(1), move || {
                let image = weak.upgrade().unwrap().window().take_snapshot().unwrap();
                let mut ppm =
                    format!("P6\n{} {}\n255\n", image.width(), image.height()).into_bytes();
                for pixel in image.as_slice() {
                    ppm.extend_from_slice(&[pixel.r, pixel.g, pixel.b]);
                }
                std::fs::write(path, ppm).unwrap();
                slint::quit_event_loop().unwrap();
            });
        }
        preview.run().unwrap();
        return;
    }

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(ButtonTestPlatform(window.clone()))).unwrap();
    let ui = ButtonFixture::new().unwrap();
    ui.window().set_size(slint::PhysicalSize::new(640, 420));
    ui.show().unwrap();
    let render = || {
        let size = ui.window().size();
        let mut pixels = vec![slint::Rgb8Pixel::default(); (size.width * size.height) as usize];
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, size.width as usize);
        });
        pixels
    };
    let off_pixels = render();
    let values = |model: slint::ModelRc<f32>| model.iter().collect::<Vec<_>>();
    assert_eq!(
        values(ui.get_metrics()),
        [28., 34., 10., 30., 96., 38., 100., 16., 56., 34.]
    );
    assert_eq!(values(ui.get_heights()), [28., 30., 38.]);
    let widths = values(ui.get_widths());
    assert!(widths[0] >= 34. && widths[0] < 160.);
    assert_eq!(&widths[1..4], &[96., 100., 34.]);
    let assert_fill = |widths: &[f32]| {
        assert!((widths[4] - widths[5]).abs() < 0.01);
        assert!((widths[5] - widths[6]).abs() < 0.01);
        assert!((widths[4] + widths[5] + widths[6] + 20. - ui.get_actions_width()).abs() < 0.01);
        assert!((ui.get_action_gap_one() - 10.).abs() < 0.01);
        assert!((ui.get_action_gap_two() - 10.).abs() < 0.01);
        assert_eq!(widths[7], ui.get_add_row_width());
    };
    assert_fill(&widths);
    assert_eq!(ui.get_danger_border_width(), 0.);
    assert_eq!(
        ui.get_accent_background().color(),
        slint::Color::from_rgb_u8(232, 117, 36)
    );
    assert_eq!(ui.get_toggle_right(), ui.get_second_toggle_right());
    let toggle_pixel = |pixels: &[slint::Rgb8Pixel], offset: usize| {
        let x = ui.get_toggle_right() as usize - offset;
        let y = ui.get_toggle_y() as usize - 8;
        let p = pixels[y * 640 + x];
        (p.r, p.g, p.b)
    };
    assert_eq!(toggle_pixel(&off_pixels, 55), (38, 38, 38));
    assert_eq!(toggle_pixel(&off_pixels, 2), (38, 38, 38));
    assert_eq!(toggle_pixel(&off_pixels, 57), (24, 24, 24));
    assert_eq!(
        ui.get_danger_background().color(),
        slint::Color::from_rgb_u8(183, 53, 46)
    );

    let position = slint::LogicalPosition::new(ui.get_click_x(), ui.get_click_y());
    let click = || {
        ui.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        ui.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    };
    click();
    assert_eq!(ui.get_click_count(), 1);
    ui.set_controls_enabled(false);
    render();
    click();
    assert_eq!(
        ui.get_click_count(),
        1,
        "disabled buttons must not emit clicked"
    );
    assert_eq!(ui.get_compact_opacity(), 0.4);
    assert_eq!(ui.get_large_opacity(), 0.45);
    assert_eq!(ui.get_danger_opacity(), 0.4);
    let danger_position = slint::LogicalPosition::new(ui.get_danger_x(), ui.get_danger_y());
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: danger_position,
        button: PointerEventButton::Left,
    });
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: danger_position,
        button: PointerEventButton::Left,
    });
    assert_eq!(
        ui.get_click_count(),
        1,
        "disabled danger button must not emit clicked"
    );
    let toggle_position =
        slint::LogicalPosition::new(ui.get_toggle_right() - 28., ui.get_toggle_y());
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: toggle_position,
        button: PointerEventButton::Left,
    });
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: toggle_position,
        button: PointerEventButton::Left,
    });
    assert!(
        !ui.get_toggle_value(),
        "disabled toggles must not emit toggled"
    );
    ui.set_controls_enabled(true);
    let toggle_position =
        slint::LogicalPosition::new(ui.get_toggle_right() - 28., ui.get_toggle_y());
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: toggle_position,
        button: PointerEventButton::Left,
    });
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: toggle_position,
        button: PointerEventButton::Left,
    });
    assert!(
        ui.get_toggle_value(),
        "toggle click must emit the setting callback"
    );
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(0., 0.),
    });
    let on_pixels = render();
    assert_eq!(toggle_pixel(&on_pixels, 55), (232, 117, 36));
    assert_eq!(toggle_pixel(&on_pixels, 2), (232, 117, 36));
    assert_eq!(toggle_pixel(&on_pixels, 57), (24, 24, 24));
    assert_eq!(values(ui.get_widths()), widths);

    ui.set_controls_enabled(true);
    let position = slint::LogicalPosition::new(ui.get_accent_x(), ui.get_accent_y());
    ui.window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    assert_eq!(
        ui.get_accent_background().color(),
        slint::Color::from_rgb_u8(245, 133, 52)
    );
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    assert_eq!(
        ui.get_accent_background().color(),
        slint::Color::from_rgb_u8(217, 104, 22)
    );
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    // Each visual variant retains its own hover and pressed behavior.
    for (x, y, normal, hover, pressed, background) in [
        (
            ui.get_click_x(),
            ui.get_click_y(),
            (38, 38, 38),
            (48, 48, 48),
            (48, 48, 48),
            0,
        ),
        (
            ui.get_toolbar_x(),
            ui.get_toolbar_y(),
            (36, 36, 36),
            (45, 41, 38),
            (48, 36, 30),
            1,
        ),
        (
            ui.get_back_x(),
            ui.get_back_y(),
            (36, 36, 36),
            (48, 48, 48),
            (41, 41, 41),
            2,
        ),
        (
            ui.get_danger_x(),
            ui.get_danger_y(),
            (183, 53, 46),
            (199, 64, 56),
            (159, 43, 37),
            3,
        ),
    ] {
        let color = || {
            let brush = match background {
                0 => ui.get_neutral_background(),
                1 => ui.get_toolbar_background(),
                2 => ui.get_large_neutral_background(),
                _ => ui.get_danger_background(),
            };
            let c = brush.color();
            (c.red(), c.green(), c.blue())
        };
        ui.window().dispatch_event(WindowEvent::PointerExited);
        assert_eq!(color(), normal);
        let position = slint::LogicalPosition::new(x, y);
        ui.window()
            .dispatch_event(WindowEvent::PointerMoved { position });
        assert_eq!(color(), hover);
        ui.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        assert_eq!(color(), pressed);
        ui.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    }
    ui.set_natural_text("A longer content-sized action".into());
    render();
    let longer = values(ui.get_widths());
    assert!(longer[0] > widths[0]);
    assert_eq!(&longer[1..], &widths[1..]);
    ui.set_action_row_width(400.);
    render();
    let narrower = values(ui.get_widths());
    assert_eq!(
        &narrower[..4],
        &longer[..4],
        "content and fixed buttons must not stretch"
    );
    assert_fill(&narrower);
    assert!(narrower[4] < longer[4] && narrower[7] < longer[7]);
    ui.set_action_row_width(500.);
    render();
    let wider = values(ui.get_widths());
    assert_fill(&wider);
    assert!(wider[4] > narrower[4] && wider[7] > narrower[7]);
    ui.hide().unwrap();
}
