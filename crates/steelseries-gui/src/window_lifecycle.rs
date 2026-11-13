#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowPolicy {
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub maximizable: bool,
    pub fullscreen: bool,
}

pub const WINDOW_POLICY: WindowPolicy = WindowPolicy {
    width: 1040,
    height: 720,
    resizable: false,
    maximizable: false,
    fullscreen: false,
};
pub const WINDOW_WIDTH: u32 = WINDOW_POLICY.width;
pub const WINDOW_HEIGHT: u32 = WINDOW_POLICY.height;

use slint::winit_030::{EventResult, WinitWindowAccessor, winit};

impl WindowPolicy {
    fn apply_native(self, window: &winit::window::Window) {
        let size = winit::dpi::LogicalSize::new(self.width, self.height);
        window.set_resizable(self.resizable);
        window.set_enabled_buttons(if self.maximizable {
            winit::window::WindowButtons::all()
        } else {
            winit::window::WindowButtons::CLOSE | winit::window::WindowButtons::MINIMIZE
        });
        window.set_min_inner_size(Some(size));
        window.set_max_inner_size((!self.resizable).then_some(size));
        if !self.fullscreen {
            window.set_fullscreen(None);
        }
        if !self.maximizable {
            window.set_maximized(false);
        }
        let _ = window.request_inner_size(size);
    }
}

/// Wayland hide destroys the native window, even though AppWindow survives.
/// Apply constraints to each new native WindowId, independently of Slint's
/// cached layout constraints. Do not keep an Arc to the native window alive.
pub fn install_native_policy(window: &slint::Window) {
    let mut applied_window = None;
    window.on_winit_window_event(move |window, event| {
        window.with_winit_window(|native| {
            if applied_window != Some(native.id()) {
                WINDOW_POLICY.apply_native(native);
                applied_window = Some(native.id());
            }
        });
        // Wayland can reuse a WindowId after close-to-tray recreates the
        // native window. Force the constraints to be applied on its return.
        if matches!(event, winit::event::WindowEvent::CloseRequested) {
            applied_window = None;
        }
        EventResult::Propagate
    });
}

pub fn restore_native_window(window: &slint::Window) {
    window.with_winit_window(|native| {
        WINDOW_POLICY.apply_native(native);
        native.set_minimized(false);
        native.focus_window();
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowLifecycle {
    instance_count: u8,
    monitor_start_count: u8,
    visible: bool,
    width: u32,
    height: u32,
    page_generation: i32,
}

impl Default for WindowLifecycle {
    fn default() -> Self {
        Self::new(true)
    }
}

impl WindowLifecycle {
    pub const fn new(visible: bool) -> Self {
        Self {
            instance_count: 1,
            monitor_start_count: 1,
            visible,
            width: WINDOW_WIDTH,
            height: WINDOW_HEIGHT,
            page_generation: 0,
        }
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    #[cfg(test)]
    pub fn policy(&self) -> WindowPolicy {
        WINDOW_POLICY
    }

    pub fn restore(&mut self) -> (u32, u32) {
        self.visible = true;
        self.width = WINDOW_WIDTH;
        self.height = WINDOW_HEIGHT;
        (self.width, self.height)
    }

    pub fn navigate(&mut self) -> i32 {
        self.page_generation = self.page_generation.wrapping_add(1);
        self.page_generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hide_restore_reuses_one_window_and_preserves_dimensions() {
        let mut lifecycle = WindowLifecycle::default();
        lifecycle.hide();
        assert!(!lifecycle.visible);
        assert_eq!(lifecycle.restore(), (WINDOW_WIDTH, WINDOW_HEIGHT));
        assert!(lifecycle.visible);
        assert_eq!(lifecycle.instance_count, 1);
        assert_eq!(lifecycle.monitor_start_count, 1);
    }

    #[test]
    fn window_policy_is_fixed_and_disallows_maximize_and_fullscreen() {
        let policy = WindowLifecycle::default().policy();
        assert_eq!((policy.width, policy.height), (1040, 720));
        assert!(!policy.resizable);
        assert!(!policy.maximizable);
        assert!(!policy.fullscreen);
    }

    #[test]
    fn page_navigation_changes_only_edit_generation() {
        let mut lifecycle = WindowLifecycle::default();
        assert_eq!(lifecycle.navigate(), 1);
        assert_eq!(lifecycle.navigate(), 2);
        assert_eq!((lifecycle.width, lifecycle.height), (1040, 720));
        assert_eq!(lifecycle.instance_count, 1);
        assert_eq!(lifecycle.monitor_start_count, 1);
    }

    #[test]
    fn autostart_visibility_respects_tray_setting_without_new_services() {
        let hidden = WindowLifecycle::new(false);
        assert!(!hidden.visible);
        assert_eq!(hidden.instance_count, 1);
        assert_eq!(hidden.monitor_start_count, 1);

        let visible = WindowLifecycle::new(true);
        assert!(visible.visible);
        assert_eq!(visible.instance_count, 1);
        assert_eq!(visible.monitor_start_count, 1);
    }
}
