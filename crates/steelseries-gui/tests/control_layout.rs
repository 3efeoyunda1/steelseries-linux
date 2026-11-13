use slint::{ComponentHandle, platform::software_renderer::MinimalSoftwareWindow};
use std::rc::Rc;

slint::slint! {
    import { SelectControl } from "../ui/components/select-control.slint";
    import { SliderNumericControl } from "../ui/components/slider-numeric-control.slint";
    import { ControlMetrics } from "../ui/theme.slint";

    export component ControlFixture inherits Window {
        width: 500px;
        height: 300px;
        background: #181818;
        in-out property <int> value: 1600;
        in-out property <int> selected-index: 0;
        in-out property <string> last-committed: "";
        in-out property <int> previews: 0;
        in-out property <int> commits: 0;
        in property <string> unit: "units";
        in property <length> row-width: 468px;
        in-out property <string> setting-label: "Setting";
        out property <length> select-height: select.height;
        out property <length> select-right: select.x + select.width;
        out property <length> select-y: select.y + select.height / 2;
        out property <length> numeric-y: numeric.y;
        out property <length> numeric-right: numeric.x + numeric.width;
        out property <length> unit-width: unit-measure.preferred-width;
        out property <length> numeric-width: ControlMetrics.numeric-input-width;
        out property <length> select-width: ControlMetrics.select-width;
        out property <string> selected-value: options[root.selected-index];
        property <[string]> options: ["Short", "A very long option that must stay complete in the underlying model"];

        unit-measure := Text { visible: false; text: root.unit; font-size: 11px; }

        VerticalLayout {
            padding: 16px;
            spacing: 15px;
            alignment: start;
            select := SelectControl {
                width: root.row-width;
                label: root.setting-label;
                options: root.options;
                current-index: root.selected-index;
                selected(index) => { root.selected-index = index; }
            }
            numeric := SliderNumericControl {
                width: root.row-width;
                label: "Value"; unit: root.unit;
                value: root.value; minimum: 1; maximum: 26000; step: 1;
                previewed(value) => { root.previews += 1; root.value = value; }
                committed(value) => { root.commits += 1; root.last-committed = value; return true; }
            }
        }
    }
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
fn fixed_controls_keep_values_and_wrap_long_labels() {
    use slint::platform::{
        Key, PointerEventButton, WindowEvent, software_renderer::RepaintBufferType,
    };
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Platform(window.clone()))).unwrap();
    let ui = ControlFixture::new().unwrap();
    ui.window().set_size(slint::PhysicalSize::new(500, 300));
    ui.show().unwrap();
    let render = || {
        let mut pixels = vec![slint::Rgb8Pixel::default(); 500 * 300];
        window.request_redraw();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, 500);
        });
        pixels
    };
    let click = |x, y| {
        let position = slint::LogicalPosition::new(x, y);
        ui.window().dispatch_event(WindowEvent::PointerPressed {
            position,
            button: PointerEventButton::Left,
        });
        ui.window().dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
    };
    let key = |text: slint::SharedString| {
        ui.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        ui.window()
            .dispatch_event(WindowEvent::KeyReleased { text });
    };
    let pixels = render();
    assert_eq!(ui.get_numeric_width(), 88.);
    assert_eq!(ui.get_select_width(), 168.);
    assert_eq!(ui.get_select_height(), 34.);
    let right = ui.get_select_right();
    // Sample a solid area near each widget's top edge, away from text and rounded corners.
    let select_y = ui.get_select_y() as usize - 12;
    let extent = |pixels: &[slint::Rgb8Pixel], y: usize, color: slint::Rgb8Pixel| {
        let row = &pixels[y * 500..(y + 1) * 500];
        let mut runs = Vec::new();
        let mut start = None;
        for (x, pixel) in row.iter().enumerate() {
            if *pixel == color {
                start.get_or_insert(x);
            } else if let Some(begin) = start.take() {
                runs.push((begin, x - begin));
            }
        }
        runs
    };
    assert!(
        extent(
            &pixels,
            select_y,
            pixels[select_y * 500 + right as usize - 8]
        )
        .iter()
        .any(
            |&(x, width)| (164..=168).contains(&width) && (x + width).abs_diff(right as usize) <= 2
        )
    );
    let numeric_y = ui.get_numeric_y() as usize;
    let numeric_span = |pixels: &[slint::Rgb8Pixel]| {
        let row = &pixels[(numeric_y + 5) * 500..(numeric_y + 6) * 500];
        let mut x = 0;
        for run in row.chunk_by(|a, b| a == b) {
            if (84..=88).contains(&run.len()) {
                return (x, run.len());
            }
            x += run.len();
        }
        panic!("numeric editor should be 88px wide");
    };
    let (numeric_x, numeric_inner_width) = numeric_span(&pixels);
    let numeric_center = (numeric_x + numeric_inner_width / 2) as f32;
    let commits_before = ui.get_commits();
    for row_width in [360., 468.] {
        ui.set_row_width(row_width);
        for unit in ["DPI", "min", "%", "units"] {
            ui.set_unit(unit.into());
            let pixels = render();
            let (x, width) = numeric_span(&pixels);
            let expected_right = ui.get_numeric_right() - ui.get_unit_width() - 10.;
            assert!(
                ((x + width) as f32 - expected_right).abs() <= 2.,
                "input and unit must form a right-aligned group"
            );
        }
    }
    assert_eq!(
        ui.get_commits(),
        commits_before,
        "layout changes must not commit settings"
    );

    for value in ["1", "1600", "26000"] {
        ui.set_value(100);
        render();
        click(numeric_center, numeric_y as f32 + 16.);
        ui.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::Control.into(),
        });
        key("a".into());
        ui.window().dispatch_event(WindowEvent::KeyReleased {
            text: Key::Control.into(),
        });
        for digit in value.chars() {
            key(digit.to_string().into());
        }
        key(Key::Return.into());
        assert_eq!(ui.get_last_committed().as_str(), value);
    }
    ui.set_value(100);
    render();
    click(numeric_center, numeric_y as f32 + 16.);
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    key("a".into());
    ui.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    for digit in "777".chars() {
        key(digit.to_string().into());
    }
    click(right - 84., ui.get_select_y());
    assert_eq!(
        ui.get_last_committed().as_str(),
        "777",
        "focus loss must commit edits"
    );
    key(Key::Escape.into());
    click(numeric_center, numeric_y as f32 + 16.);
    ui.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    key("a".into());
    ui.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
    for digit in "555".chars() {
        key(digit.to_string().into());
    }
    key(Key::Escape.into());
    click(20., ui.get_select_y());
    assert_eq!(
        ui.get_last_committed().as_str(),
        "777",
        "Escape must cancel edits"
    );
    let slider_start = slint::LogicalPosition::new(35., numeric_y as f32 + 47.);
    let slider_end = slint::LogicalPosition::new(120., numeric_y as f32 + 47.);
    ui.window().dispatch_event(WindowEvent::PointerPressed {
        position: slider_start,
        button: PointerEventButton::Left,
    });
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: slider_end,
    });
    ui.window().dispatch_event(WindowEvent::PointerReleased {
        position: slider_end,
        button: PointerEventButton::Left,
    });
    assert!(ui.get_previews() > 0, "slider must emit preview");
    let slider_value = ui.get_value();
    click(numeric_center, numeric_y as f32 + 16.);
    key(Key::End.into());
    key("1".into());
    key(Key::Return.into());
    assert_eq!(ui.get_last_committed().as_str(), format!("{slider_value}1"));
    click(right - 84., ui.get_select_y());
    key(Key::DownArrow.into());
    assert_eq!(ui.get_selected_index(), 1);
    assert_eq!(
        ui.get_selected_value().as_str(),
        "A very long option that must stay complete in the underlying model"
    );
    key(Key::Escape.into());
    render();
    assert_eq!(ui.get_select_right(), right);
    ui.set_setting_label("A setting label long enough to need several lines while the selection control keeps its width".into());
    render();
    assert!(
        ui.get_select_height() > 34.,
        "long labels must wrap without clipping"
    );
    assert_eq!(ui.get_select_right(), right);
    assert_eq!(
        ui.get_selected_index(),
        1,
        "selection must survive layout updates"
    );
    assert_eq!(
        ui.get_selected_value().as_str(),
        "A very long option that must stay complete in the underlying model"
    );
    ui.hide().unwrap();
}
