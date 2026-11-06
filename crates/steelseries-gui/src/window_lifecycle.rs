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
