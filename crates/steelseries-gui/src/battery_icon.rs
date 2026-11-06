//! Shared battery asset selection for the visible GUI and native tray.

use ksni::Icon;
use slint::Image;

use crate::{settings::TrayIconTheme, tray::TrayState};

const APP_LOGO: &[u8] = include_bytes!("../../../assets/app/steelseries-linux.svg");

const COLORED_NORMAL: [&[u8]; 11] = [
    include_bytes!("../../../assets/battery/colored/normal/battery-000.svg"),
    include_bytes!("../../../assets/battery/colored/normal/battery-010.svg"),
    include_bytes!("../../../assets/battery/colored/normal/battery-020.svg"),
    include_bytes!("../../../assets/battery/colored/normal/battery-030.svg"),
    include_bytes!("../../../assets/battery/colored/normal/battery-040.svg"),
    include_bytes!("../../../assets/battery/colored/normal/battery-050.svg"),
    include_bytes!("../../../assets/battery/colored/normal/battery-060.svg"),
    include_bytes!("../../../assets/battery/colored/normal/battery-070.svg"),
    include_bytes!("../../../assets/battery/colored/normal/battery-080.svg"),
    include_bytes!("../../../assets/battery/colored/normal/battery-090.svg"),
    include_bytes!("../../../assets/battery/colored/normal/battery-100.svg"),
];

const COLORED_CHARGING: [&[u8]; 11] = [
    include_bytes!("../../../assets/battery/colored/charging/battery-000-charging.svg"),
    include_bytes!("../../../assets/battery/colored/charging/battery-010-charging.svg"),
    include_bytes!("../../../assets/battery/colored/charging/battery-020-charging.svg"),
    include_bytes!("../../../assets/battery/colored/charging/battery-030-charging.svg"),
    include_bytes!("../../../assets/battery/colored/charging/battery-040-charging.svg"),
    include_bytes!("../../../assets/battery/colored/charging/battery-050-charging.svg"),
    include_bytes!("../../../assets/battery/colored/charging/battery-060-charging.svg"),
    include_bytes!("../../../assets/battery/colored/charging/battery-070-charging.svg"),
    include_bytes!("../../../assets/battery/colored/charging/battery-080-charging.svg"),
    include_bytes!("../../../assets/battery/colored/charging/battery-090-charging.svg"),
    include_bytes!("../../../assets/battery/colored/charging/battery-100-charging.svg"),
];

const MONOCHROME_NORMAL: [&[u8]; 11] = [
    include_bytes!("../../../assets/battery/monochrome/normal/battery-000.svg"),
    include_bytes!("../../../assets/battery/monochrome/normal/battery-010.svg"),
    include_bytes!("../../../assets/battery/monochrome/normal/battery-020.svg"),
    include_bytes!("../../../assets/battery/monochrome/normal/battery-030.svg"),
    include_bytes!("../../../assets/battery/monochrome/normal/battery-040.svg"),
    include_bytes!("../../../assets/battery/monochrome/normal/battery-050.svg"),
    include_bytes!("../../../assets/battery/monochrome/normal/battery-060.svg"),
    include_bytes!("../../../assets/battery/monochrome/normal/battery-070.svg"),
    include_bytes!("../../../assets/battery/monochrome/normal/battery-080.svg"),
    include_bytes!("../../../assets/battery/monochrome/normal/battery-090.svg"),
    include_bytes!("../../../assets/battery/monochrome/normal/battery-100.svg"),
];

const MONOCHROME_CHARGING: [&[u8]; 11] = [
    include_bytes!("../../../assets/battery/monochrome/charging/battery-000-charging.svg"),
    include_bytes!("../../../assets/battery/monochrome/charging/battery-010-charging.svg"),
    include_bytes!("../../../assets/battery/monochrome/charging/battery-020-charging.svg"),
    include_bytes!("../../../assets/battery/monochrome/charging/battery-030-charging.svg"),
    include_bytes!("../../../assets/battery/monochrome/charging/battery-040-charging.svg"),
    include_bytes!("../../../assets/battery/monochrome/charging/battery-050-charging.svg"),
    include_bytes!("../../../assets/battery/monochrome/charging/battery-060-charging.svg"),
    include_bytes!("../../../assets/battery/monochrome/charging/battery-070-charging.svg"),
    include_bytes!("../../../assets/battery/monochrome/charging/battery-080-charging.svg"),
    include_bytes!("../../../assets/battery/monochrome/charging/battery-090-charging.svg"),
    include_bytes!("../../../assets/battery/monochrome/charging/battery-100-charging.svg"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryAssetStyle {
    Colored,
    Monochrome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryAsset {
    pub style: BatteryAssetStyle,
    pub level: u8,
    pub charging: bool,
}

impl BatteryAsset {
    pub const fn for_percentage(style: BatteryAssetStyle, percentage: u8, charging: bool) -> Self {
        Self {
            style,
            level: nearest_asset_level(percentage),
            charging,
        }
    }

    #[cfg(test)]
    pub fn file_name(self) -> String {
        if self.charging {
            format!("battery-{:03}-charging.svg", self.level)
        } else {
            format!("battery-{:03}.svg", self.level)
        }
    }

    fn svg(self) -> &'static [u8] {
        let index = usize::from(self.level / 10);
        match (self.style, self.charging) {
            (BatteryAssetStyle::Colored, false) => COLORED_NORMAL[index],
            (BatteryAssetStyle::Colored, true) => COLORED_CHARGING[index],
            (BatteryAssetStyle::Monochrome, false) => MONOCHROME_NORMAL[index],
            (BatteryAssetStyle::Monochrome, true) => MONOCHROME_CHARGING[index],
        }
    }
}

pub const fn nearest_asset_level(percentage: u8) -> u8 {
    let clamped = if percentage > 100 { 100 } else { percentage };
    (clamped + 5) / 10 * 10
}

pub fn colored_battery_image(percentage: Option<u8>, charging: bool) -> Image {
    let asset = BatteryAsset::for_percentage(
        BatteryAssetStyle::Colored,
        percentage.unwrap_or(0),
        charging,
    );
    Image::load_from_svg_data(asset.svg()).unwrap_or_default()
}

pub fn tray_asset(state: &TrayState, theme: TrayIconTheme) -> Option<BatteryAsset> {
    state.percentage.map(|percentage| {
        let style = match theme {
            TrayIconTheme::Colored => BatteryAssetStyle::Colored,
            TrayIconTheme::Monochrome => BatteryAssetStyle::Monochrome,
        };
        BatteryAsset::for_percentage(style, percentage, state.charging)
    })
}

pub fn tray_icon(state: &TrayState, theme: TrayIconTheme) -> Icon {
    let svg = tray_asset(state, theme).map_or(APP_LOGO, BatteryAsset::svg);
    svg_to_tray_icon(svg)
}

fn svg_to_tray_icon(svg: &[u8]) -> Icon {
    let Some(buffer) = Image::load_from_svg_data(svg)
        .ok()
        .and_then(|image| image.to_rgba8())
    else {
        return Icon {
            width: 1,
            height: 1,
            data: vec![0, 0, 0, 0],
        };
    };
    let mut argb = Vec::with_capacity(buffer.as_bytes().len());
    for rgba in buffer.as_bytes().as_chunks::<4>().0 {
        argb.extend_from_slice(&[rgba[3], rgba[0], rgba[1], rgba[2]]);
    }
    Icon {
        width: i32::try_from(buffer.width()).unwrap_or(1),
        height: i32::try_from(buffer.height()).unwrap_or(1),
        data: argb,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_ten_mapping_uses_documented_boundaries() {
        assert_eq!(nearest_asset_level(0), 0);
        assert_eq!(nearest_asset_level(4), 0);
        assert_eq!(nearest_asset_level(5), 10);
        assert_eq!(nearest_asset_level(14), 10);
        assert_eq!(nearest_asset_level(15), 20);
        assert_eq!(nearest_asset_level(88), 90);
        assert_eq!(nearest_asset_level(94), 90);
        assert_eq!(nearest_asset_level(95), 100);
        assert_eq!(nearest_asset_level(255), 100);
    }

    #[test]
    fn exact_state_is_preserved_while_asset_is_quantized() {
        let state = TrayState {
            device_name: Some("Device".into()),
            percentage: Some(88),
            charging: false,
        };
        let asset = tray_asset(&state, TrayIconTheme::Monochrome).unwrap();
        assert_eq!(state.percentage, Some(88));
        assert_eq!(asset.level, 90);
        assert_eq!(asset.file_name(), "battery-090.svg");
    }

    #[test]
    fn charging_and_disconnected_assets_are_selected_semantically() {
        let charging = TrayState {
            device_name: Some("Device".into()),
            percentage: Some(51),
            charging: true,
        };
        assert_eq!(
            tray_asset(&charging, TrayIconTheme::Monochrome)
                .unwrap()
                .file_name(),
            "battery-050-charging.svg"
        );
        assert_eq!(
            tray_asset(&TrayState::default(), TrayIconTheme::Colored),
            None
        );
        let fallback = tray_icon(&TrayState::default(), TrayIconTheme::Monochrome);
        assert!(fallback.width > 1 && fallback.height > 1);
    }

    #[test]
    fn tray_theme_selects_the_requested_existing_asset_set() {
        let state = TrayState {
            device_name: Some("Device".into()),
            percentage: Some(88),
            charging: false,
        };
        assert_eq!(
            tray_asset(&state, TrayIconTheme::Colored).unwrap().style,
            BatteryAssetStyle::Colored
        );
        assert_eq!(
            tray_asset(&state, TrayIconTheme::Monochrome).unwrap().style,
            BatteryAssetStyle::Monochrome
        );
    }
}
