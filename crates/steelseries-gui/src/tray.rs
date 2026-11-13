use std::sync::mpsc::Sender;

use ksni::{
    Icon, MenuItem, ToolTip, Tray,
    blocking::{Handle, TrayMethods},
    menu::StandardItem,
};

use crate::{battery_icon::tray_icon, settings::TrayIconTheme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrayState {
    pub device_name: Option<String>,
    pub percentage: Option<u8>,
    pub charging: bool,
}

pub struct TrayController {
    handle: Option<Handle<AppTray>>,
    action_sender: Sender<TrayAction>,
    state: TrayState,
    icon_theme: TrayIconTheme,
}

impl TrayController {
    pub fn new(action_sender: Sender<TrayAction>) -> Self {
        Self {
            handle: None,
            action_sender,
            state: TrayState::default(),
            icon_theme: TrayIconTheme::default(),
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if enabled && self.handle.is_none() {
            let tray = AppTray {
                state: self.state.clone(),
                icon_theme: self.icon_theme,
                action_sender: self.action_sender.clone(),
            };
            match tray.assume_sni_available(true).spawn() {
                Ok(handle) => self.handle = Some(handle),
                Err(error) => eprintln!("steelseries-linux-gui: system tray unavailable: {error}"),
            }
        } else if !enabled && let Some(handle) = self.handle.take() {
            handle.shutdown().wait();
        }
    }

    pub fn update(&mut self, state: TrayState) {
        self.state = state.clone();
        if let Some(handle) = &self.handle {
            let _ = handle.update(move |tray| tray.state = state);
        }
    }

    pub fn set_icon_theme(&mut self, icon_theme: TrayIconTheme) {
        self.icon_theme = icon_theme;
        if let Some(handle) = &self.handle {
            let _ = handle.update(move |tray| tray.icon_theme = icon_theme);
        }
    }
}

impl Drop for TrayController {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.shutdown().wait();
        }
    }
}

struct AppTray {
    state: TrayState,
    icon_theme: TrayIconTheme,
    action_sender: Sender<TrayAction>,
}

impl Tray for AppTray {
    fn id(&self) -> String {
        "steelseries-linux".to_owned()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    fn title(&self) -> String {
        "SteelSeries Linux".to_owned()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![tray_icon(&self.state, self.icon_theme)]
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: self.title(),
            description: tray_description(&self.state),
            icon_pixmap: self.icon_pixmap(),
            ..ToolTip::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.action_sender.send(TrayAction::Open);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: "Open SteelSeries Linux".to_owned(),
                icon_name: "window-new".to_owned(),
                activate: Box::new(|tray: &mut AppTray| {
                    let _ = tray.action_sender.send(TrayAction::Open);
                }),
                ..StandardItem::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".to_owned(),
                icon_name: "application-exit".to_owned(),
                activate: Box::new(|tray: &mut AppTray| {
                    let _ = tray.action_sender.send(TrayAction::Quit);
                }),
                ..StandardItem::default()
            }
            .into(),
        ]
    }
}

fn tray_description(state: &TrayState) -> String {
    let Some(name) = &state.device_name else {
        return "No active device".to_owned();
    };
    match (state.percentage, state.charging) {
        (Some(percentage), true) => format!("{name} · {percentage}% · Charging"),
        (Some(percentage), false) => format!("{name} · {percentage}%"),
        (None, _) => name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_is_compact_and_does_not_repeat_application_name() {
        let state = TrayState {
            device_name: Some("Aerox 3 Wireless Gen 2".into()),
            percentage: Some(88),
            charging: true,
        };
        assert_eq!(
            tray_description(&state),
            "Aerox 3 Wireless Gen 2 · 88% · Charging"
        );
        assert_eq!(tray_description(&TrayState::default()), "No active device");
    }
}
