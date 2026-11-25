use std::{fmt, process::ExitCode, str::FromStr};

mod completion;
mod output;

use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::engine::ArgValueCandidates;
use steelseries_core::{
    BatteryStatus, Device, DeviceDriver, DeviceFeature, DeviceModel, DpiConfig, DpiStage, Error,
    IdentitySource, LiftOffDistance, LowPowerPollingRate, PhysicalDevice, PollingCapabilities,
    PollingRate, PowerConfig,
    validation::{
        auto_low_power_threshold, sleep_timer_from_minutes, validate_dpi_request,
        validate_dpi_values, validate_scroll_jump_delay, validate_stage_id,
    },
};

use output::{
    ActiveStageData, AutoLowPowerEnabledData, AutoLowPowerThresholdData, BatteryData,
    CommandOutput, DelayData, DeviceDescribeData, DeviceSnapshotData, DevicesData, DpiData,
    EnabledData, HelpData, LodData, LodSetData, LowPowerEnabledData, LowPowerPollingData,
    PollingData, PollingSetData, PowerData, ScrollJumpData, SleepTimerData, error_document,
    invalid_argument_document, lod_name,
};

const DPI_HELP: &str = "DPI commands:\n  steelseriesctl dpi get\n  steelseriesctl dpi set <dpi1|X1xY1> [dpi2|X2xY2] ... [dpi5|X5xY5]\n  steelseriesctl dpi use <stage-id>\n\nA single value applies to both axes; use XxY for independent axis values.";
const LOD_HELP: &str = "Lift-off distance commands:\n  steelseriesctl lod get\n  steelseriesctl lod set low <stage-id>\n  steelseriesctl lod set high <stage-id>";
const POLLING_HELP: &str = "Polling commands:\n  steelseriesctl polling get\n  steelseriesctl polling set <HZ>\n  steelseriesctl polling set wired <HZ>\n  steelseriesctl polling set wireless <HZ>\n\nOmit the mode if the selected device supports only one polling mode.\nUse device describe to see the supported modes and rates.";
const BATTERY_HELP: &str = "Battery commands:\n  steelseriesctl battery get";
const POWER_HELP: &str = "Power Management commands:\n  steelseriesctl power get\n  steelseriesctl power low-power set <on|off>\n  steelseriesctl power low-power polling <125|250|500>\n  steelseriesctl power auto-low-power set <on|off>\n  steelseriesctl power auto-low-power threshold <5..25>\n  steelseriesctl power sleep set <minutes>";
const WIRELESS_STABILITY_HELP: &str = "Wireless Stability Enhancement commands:\n  steelseriesctl wireless-stability get\n  steelseriesctl wireless-stability set <on|off>";
const BLUETOOTH_SMOOTHING_HELP: &str = "Bluetooth Smoothing commands:\n  steelseriesctl bluetooth-smoothing get\n  steelseriesctl bluetooth-smoothing set <on|off>";
const SCROLL_JUMP_HELP: &str = "Scroll Jump Protection commands:\n  steelseriesctl scroll-jump get\n  steelseriesctl scroll-jump set <on|off>\n  steelseriesctl scroll-jump delay <milliseconds>";
const DEVICE_HELP: &str = "Device commands:\n  steelseriesctl device list\n  steelseriesctl device describe\n  steelseriesctl device snapshot";
const ROOT_HELP: &str = "SteelSeries Linux CLI\n\nCommands:\n  device               List, describe, or inspect supported SteelSeries devices\n  dpi\n  lod\n  polling\n  battery\n  power\n  wireless-stability\n  bluetooth-smoothing\n  scroll-jump\n\nOptions:\n  -d, --device <ID>  Select a physical device by device ID\n      --json         Emit machine-readable JSON";

#[derive(Debug, Parser)]
#[command(name = "steelseriesctl", about = "SteelSeries Linux CLI", version)]
struct Cli {
    /// Select a physical device by device ID.
    #[arg(short = 'd', long, global = true, value_name = "ID", add = ArgValueCandidates::new(completion::devices))]
    device: Option<String>,
    /// Emit a stable machine-readable JSON document.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List, describe, or inspect supported SteelSeries devices.
    Device {
        #[command(subcommand)]
        command: Option<DeviceCommand>,
    },
    /// Read or change DPI stages.
    Dpi {
        #[command(subcommand)]
        command: Option<DpiCommand>,
    },
    /// Read or change lift-off distance by DPI stage.
    Lod {
        #[command(subcommand)]
        command: Option<LodCommand>,
    },
    /// Read or change USB/2.4 GHz polling rates.
    Polling {
        #[command(subcommand)]
        command: Option<PollingCommand>,
    },
    /// Read battery status.
    Battery {
        #[command(subcommand)]
        command: Option<BatteryCommand>,
    },
    /// Read or change power-management settings.
    Power {
        #[command(subcommand)]
        command: Option<PowerCommand>,
    },
    /// Read or change Wireless Stability Enhancement.
    WirelessStability {
        #[command(subcommand)]
        command: Option<FeatureToggleCommand>,
    },
    /// Read or change Bluetooth Smoothing.
    BluetoothSmoothing {
        #[command(subcommand)]
        command: Option<FeatureToggleCommand>,
    },
    /// Read or change Scroll Jump Protection.
    ScrollJump {
        #[command(subcommand)]
        command: Option<ScrollJumpCommand>,
    },
}

#[derive(Debug, Subcommand)]
enum DeviceCommand {
    /// List usable supported physical devices.
    List,
    /// Describe the selected device and its capabilities.
    Describe,
    /// Read the selected device's complete supported state.
    Snapshot,
}

#[derive(Debug, Subcommand)]
enum DpiCommand {
    /// Show all DPI stages and the active stage.
    Get,
    /// Replace the DPI stages using DPI or XxY values.
    Set {
        #[arg(required = true, num_args = 1.., value_name = "DPI|XxY")]
        dpis: Vec<DpiInput>,
    },
    /// Select an existing stage by its one-based ID.
    Use {
        #[arg(value_name = "STAGE_ID", value_parser = parse_stage_id)]
        stage_id: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DpiInput {
    x: u16,
    y: u16,
}

impl FromStr for DpiInput {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let separators = value
            .chars()
            .filter(|character| matches!(character, 'x' | 'X'))
            .count();
        match separators {
            0 => {
                let dpi = parse_dpi_axis(value, value)?;
                Ok(Self { x: dpi, y: dpi })
            }
            1 => {
                let (x, y) = value
                    .split_once(['x', 'X'])
                    .expect("one separator was counted");
                Ok(Self {
                    x: parse_dpi_axis(x, value)?,
                    y: parse_dpi_axis(y, value)?,
                })
            }
            _ => Err(format!("invalid DPI value '{value}': expected DPI or XxY")),
        }
    }
}

impl fmt::Display for DpiInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.x == self.y {
            write!(formatter, "{}", self.x)
        } else {
            write!(formatter, "{}x{}", self.x, self.y)
        }
    }
}

fn parse_dpi_axis(axis: &str, input: &str) -> Result<u16, String> {
    axis.parse::<u16>()
        .map_err(|_| format!("invalid DPI value '{input}': expected DPI or XxY using u16 values"))
}

fn parse_stage_id(value: &str) -> Result<usize, String> {
    let stage_id = value
        .parse::<usize>()
        .map_err(|_| format!("invalid stage ID '{value}'"))?;
    validate_stage_id(stage_id).map_err(|error| error.to_string())?;
    Ok(stage_id)
}

#[derive(Debug, Subcommand)]
enum LodCommand {
    /// Show lift-off distance for every configured stage.
    Get,
    /// Change one stage while preserving all other sensor settings.
    Set {
        distance: LodValue,
        #[arg(value_name = "STAGE_ID", value_parser = parse_stage_id)]
        stage_id: usize,
    },
}

#[derive(Debug, Subcommand)]
enum PollingCommand {
    /// Show polling rates.
    Get,
    /// Set a polling rate. Specify the mode when the device supports both.
    #[command(
        after_help = "Examples:\n  steelseriesctl polling set 500\n  steelseriesctl polling set wired 1000\n  steelseriesctl polling set wireless 4000"
    )]
    Set {
        #[arg(value_name = "HZ|wired|wireless", add = ArgValueCandidates::new(completion::polling_inputs))]
        mode_or_rate: PollingInput,
        #[arg(value_name = "HZ", required_if_eq_any([("mode_or_rate", "wired"), ("mode_or_rate", "wireless")]))]
        rate: Option<u16>,
    },
}

#[derive(Debug, Subcommand)]
enum BatteryCommand {
    /// Show the current battery and charging status.
    Get,
}

#[derive(Debug, Subcommand)]
enum PowerCommand {
    /// Show all power-management settings.
    Get,
    /// Configure Low Power Mode.
    LowPower {
        #[command(subcommand)]
        command: LowPowerCommand,
    },
    /// Configure Auto Low Power.
    AutoLowPower {
        #[command(subcommand)]
        command: AutoLowPowerCommand,
    },
    /// Configure the inactivity sleep timer.
    Sleep {
        #[command(subcommand)]
        command: SleepCommand,
    },
}

#[derive(Debug, Subcommand)]
enum FeatureToggleCommand {
    /// Show the current setting.
    Get,
    /// Enable or disable the setting.
    Set { state: PowerState },
}

#[derive(Debug, Subcommand)]
enum ScrollJumpCommand {
    /// Show the current enabled state and delay.
    Get,
    /// Enable or disable Scroll Jump Protection.
    Set { state: PowerState },
    /// Set the protection delay in milliseconds.
    Delay { milliseconds: u16 },
}

#[derive(Debug, Subcommand)]
enum LowPowerCommand {
    /// Enable or disable Low Power Mode.
    Set { state: PowerState },
    /// Set the polling rate used by Low Power Mode.
    Polling {
        #[arg(add = ArgValueCandidates::new(completion::low_power_rates))]
        rate: u16,
    },
}

#[derive(Debug, Subcommand)]
enum AutoLowPowerCommand {
    /// Enable or disable Auto Low Power.
    Set { state: PowerState },
    /// Set the Auto Low Power battery threshold percentage.
    Threshold { percent: u8 },
}

#[derive(Debug, Subcommand)]
enum SleepCommand {
    /// Set the inactivity timer in whole minutes.
    Set { minutes: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum PollingMode {
    Wireless,
    Wired,
}

#[derive(Debug, Clone, Copy)]
enum PollingInput {
    Mode(PollingMode),
    Rate(u16),
}

impl FromStr for PollingInput {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "wired" => Ok(Self::Mode(PollingMode::Wired)),
            "wireless" => Ok(Self::Mode(PollingMode::Wireless)),
            _ => input.parse().map(Self::Rate).map_err(|_| {
                format!("expected a polling rate in Hz, 'wired', or 'wireless', got '{input}'")
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LodValue {
    Low,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum PowerState {
    On,
    Off,
}

impl PowerState {
    const fn enabled(self) -> bool {
        matches!(self, Self::On)
    }
}

impl fmt::Display for PowerState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::On => "On",
            Self::Off => "Off",
        })
    }
}

impl From<LodValue> for LiftOffDistance {
    fn from(value: LodValue) -> Self {
        match value {
            LodValue::Low => Self::Low,
            LodValue::High => Self::High,
        }
    }
}

fn main() -> ExitCode {
    if let Some(status) = completion::handle() {
        return status;
    }
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return handle_parse_error(error),
    };
    let json = cli.json;
    let command = command_identifier(&cli);
    match run(cli) {
        Ok(output) => {
            output.write(json);
            ExitCode::SUCCESS
        }
        Err(error) => {
            if json {
                eprintln!("{}", error_document(command, &error));
            } else {
                eprintln!("Error: {error}");
            }
            ExitCode::FAILURE
        }
    }
}

fn handle_parse_error(error: clap::Error) -> ExitCode {
    let exit_code = error.exit_code();
    let json_requested = std::env::args_os().any(|argument| argument == "--json");
    if json_requested && error.use_stderr() {
        eprintln!(
            "{}",
            invalid_argument_document("cli", error.to_string().trim_end())
        );
    } else if let Err(print_error) = error.print() {
        eprintln!("failed to print command-line error: {print_error}");
    }
    ExitCode::from(u8::try_from(exit_code).unwrap_or(1))
}

fn run(cli: Cli) -> Result<CommandOutput, Error> {
    let Cli {
        device,
        json: _,
        command,
    } = cli;
    let requested_identity = device.as_deref();

    match command {
        None => Ok(CommandOutput::new(
            "help",
            ROOT_HELP,
            HelpData { help: ROOT_HELP },
        )),
        Some(Command::Device { command: None }) => Ok(help_output("device", DEVICE_HELP)),
        Some(Command::Device {
            command: Some(command),
        }) => run_device(command, requested_identity),
        Some(Command::Dpi { command: None }) => Ok(help_output("dpi", DPI_HELP)),
        Some(Command::Dpi {
            command: Some(command),
        }) => run_dpi(command, requested_identity),
        Some(Command::Lod { command: None }) => Ok(help_output("lod", LOD_HELP)),
        Some(Command::Lod {
            command: Some(command),
        }) => run_lod(command, requested_identity),
        Some(Command::Polling { command: None }) => Ok(help_output("polling", POLLING_HELP)),
        Some(Command::Polling {
            command: Some(command),
        }) => run_polling(command, requested_identity),
        Some(Command::Battery { command: None }) => Ok(help_output("battery", BATTERY_HELP)),
        Some(Command::Battery {
            command: Some(command),
        }) => run_battery(command, requested_identity),
        Some(Command::Power { command: None }) => Ok(help_output("power", POWER_HELP)),
        Some(Command::Power {
            command: Some(command),
        }) => run_power(command, requested_identity),
        Some(Command::WirelessStability { command: None }) => {
            Ok(help_output("wireless-stability", WIRELESS_STABILITY_HELP))
        }
        Some(Command::WirelessStability {
            command: Some(command),
        }) => run_wireless_stability(command, requested_identity),
        Some(Command::BluetoothSmoothing { command: None }) => {
            Ok(help_output("bluetooth-smoothing", BLUETOOTH_SMOOTHING_HELP))
        }
        Some(Command::BluetoothSmoothing {
            command: Some(command),
        }) => run_bluetooth_smoothing(command, requested_identity),
        Some(Command::ScrollJump { command: None }) => {
            Ok(help_output("scroll-jump", SCROLL_JUMP_HELP))
        }
        Some(Command::ScrollJump {
            command: Some(command),
        }) => run_scroll_jump(command, requested_identity),
    }
}

fn help_output(command: &'static str, help: &'static str) -> CommandOutput {
    CommandOutput::new(command, help, HelpData { help })
}

fn command_identifier(cli: &Cli) -> &'static str {
    match &cli.command {
        None => "help",
        Some(Command::Device { command: None }) => "device",
        Some(Command::Device {
            command: Some(DeviceCommand::List),
        }) => "device.list",
        Some(Command::Device {
            command: Some(DeviceCommand::Describe),
        }) => "device.describe",
        Some(Command::Device {
            command: Some(DeviceCommand::Snapshot),
        }) => "device.snapshot",
        Some(Command::Dpi { command: None }) => "dpi",
        Some(Command::Dpi {
            command: Some(DpiCommand::Get),
        }) => "dpi.get",
        Some(Command::Dpi {
            command: Some(DpiCommand::Set { .. }),
        }) => "dpi.set",
        Some(Command::Dpi {
            command: Some(DpiCommand::Use { .. }),
        }) => "dpi.use",
        Some(Command::Lod { command: None }) => "lod",
        Some(Command::Lod {
            command: Some(LodCommand::Get),
        }) => "lod.get",
        Some(Command::Lod {
            command: Some(LodCommand::Set { .. }),
        }) => "lod.set",
        Some(Command::Polling { command: None }) => "polling",
        Some(Command::Polling {
            command: Some(PollingCommand::Get),
        }) => "polling.get",
        Some(Command::Polling {
            command: Some(PollingCommand::Set { .. }),
        }) => "polling.set",
        Some(Command::Battery { command: None }) => "battery",
        Some(Command::Battery {
            command: Some(BatteryCommand::Get),
        }) => "battery.get",
        Some(Command::Power { command: None }) => "power",
        Some(Command::Power {
            command: Some(PowerCommand::Get),
        }) => "power.get",
        Some(Command::Power {
            command:
                Some(PowerCommand::LowPower {
                    command: LowPowerCommand::Set { .. },
                }),
        }) => "power.low-power.set",
        Some(Command::Power {
            command:
                Some(PowerCommand::LowPower {
                    command: LowPowerCommand::Polling { .. },
                }),
        }) => "power.low-power.polling",
        Some(Command::Power {
            command:
                Some(PowerCommand::AutoLowPower {
                    command: AutoLowPowerCommand::Set { .. },
                }),
        }) => "power.auto-low-power.set",
        Some(Command::Power {
            command:
                Some(PowerCommand::AutoLowPower {
                    command: AutoLowPowerCommand::Threshold { .. },
                }),
        }) => "power.auto-low-power.threshold",
        Some(Command::Power {
            command:
                Some(PowerCommand::Sleep {
                    command: SleepCommand::Set { .. },
                }),
        }) => "power.sleep.set",
        Some(Command::WirelessStability { command: None }) => "wireless-stability",
        Some(Command::WirelessStability {
            command: Some(FeatureToggleCommand::Get),
        }) => "wireless-stability.get",
        Some(Command::WirelessStability {
            command: Some(FeatureToggleCommand::Set { .. }),
        }) => "wireless-stability.set",
        Some(Command::BluetoothSmoothing { command: None }) => "bluetooth-smoothing",
        Some(Command::BluetoothSmoothing {
            command: Some(FeatureToggleCommand::Get),
        }) => "bluetooth-smoothing.get",
        Some(Command::BluetoothSmoothing {
            command: Some(FeatureToggleCommand::Set { .. }),
        }) => "bluetooth-smoothing.set",
        Some(Command::ScrollJump { command: None }) => "scroll-jump",
        Some(Command::ScrollJump {
            command: Some(ScrollJumpCommand::Get),
        }) => "scroll-jump.get",
        Some(Command::ScrollJump {
            command: Some(ScrollJumpCommand::Set { .. }),
        }) => "scroll-jump.set",
        Some(Command::ScrollJump {
            command: Some(ScrollJumpCommand::Delay { .. }),
        }) => "scroll-jump.delay",
    }
}

fn run_device(
    command: DeviceCommand,
    requested_identity: Option<&str>,
) -> Result<CommandOutput, Error> {
    match command {
        DeviceCommand::List => run_device_list(),
        DeviceCommand::Describe => run_device_describe(requested_identity),
        DeviceCommand::Snapshot => run_device_snapshot(requested_identity),
    }
}

fn run_device_list() -> Result<CommandOutput, Error> {
    let devices = Device::discover()?;
    let mut human = "SteelSeries devices:".to_owned();
    if devices.is_empty() {
        human.push_str("\n  No supported SteelSeries devices found.");
    }
    for (index, device) in devices.iter().enumerate() {
        if index > 0 {
            human.push('\n');
        }
        human.push_str(&format!(
            "\n  {}\n    {}: {}\n    Connection: {}",
            device.model,
            device.identity.source().label(),
            device.identity,
            device.active_connection
        ));
    }
    Ok(CommandOutput::new(
        "device.list",
        human,
        DevicesData::from_devices(&devices),
    ))
}

fn run_device_describe(requested_identity: Option<&str>) -> Result<CommandOutput, Error> {
    let device = Device::select(requested_identity)?;
    let physical = device.physical_device();
    Ok(CommandOutput::new(
        "device.describe",
        format_device_description(physical),
        DeviceDescribeData::new(physical, device.capabilities()),
    ))
}

fn format_device_identity(device: &PhysicalDevice) -> String {
    format!(
        "Device: {}\n{}: {}\nConnection: {}",
        device.model,
        device.identity.source().label(),
        device.identity,
        device.active_connection
    )
}

fn format_device_description(device: &PhysicalDevice) -> String {
    let caps = device.model.capabilities();
    let rates = |values: &[PollingRate]| {
        values
            .iter()
            .map(|rate| rate.hz().to_string())
            .collect::<Vec<_>>()
            .join("/")
    };
    let mut lines = vec![
        format_device_identity(device),
        "\nCapabilities:".to_owned(),
        format!(
            "DPI: {}-{}, step {}, max {} stages{}",
            caps.dpi.min,
            caps.dpi.max,
            caps.dpi.step,
            caps.dpi.max_stages,
            if caps.dpi.independent_axes {
                ", independent X/Y"
            } else {
                ""
            }
        ),
    ];
    if caps.lod.supported {
        let values = caps
            .lod
            .values
            .iter()
            .map(|value| value.label)
            .collect::<Vec<_>>()
            .join(" / ");
        let scope = if caps.lod.per_stage {
            ", per stage"
        } else {
            ""
        };
        lines.push(format!("LOD: {values}{scope}"));
    }
    if !caps.polling.wireless.is_empty() {
        lines.push(format!(
            "Wireless Polling: {} Hz",
            rates(caps.polling.wireless)
        ));
    }
    if !caps.polling.wired.is_empty() {
        lines.push(format!("Wired Polling: {} Hz", rates(caps.polling.wired)));
    }
    if caps.battery.supported {
        let charging = if caps.battery.charging_state {
            " (charging state)"
        } else {
            ""
        };
        lines.push(format!("Battery: Supported{charging}"));
    }
    if caps.power.low_power_mode {
        lines.push("Low Power Mode: Supported".to_owned());
    }
    if !caps.power.low_power_polling.is_empty() {
        let rates = caps
            .power
            .low_power_polling
            .iter()
            .map(|rate| rate.hz().to_string())
            .collect::<Vec<_>>()
            .join("/");
        lines.push(format!("Low Power Polling: {rates} Hz"));
    }
    if caps.power.sleep_timer.supported {
        let timer = caps.power.sleep_timer;
        lines.push(format!(
            "Sleep Timer: {}-{} min, step {}",
            timer.min, timer.max, timer.step
        ));
    }
    if caps.power.auto_low_power_supported {
        let threshold = caps.power.auto_low_power_threshold;
        lines.push(format!(
            "Auto Low Power Threshold: {}-{}%, step {}",
            threshold.min, threshold.max, threshold.step
        ));
    }
    if caps.wireless_features.wireless_stability.supported {
        lines.push("Wireless Stability Enhancement: Supported".to_owned());
    }
    if caps.wireless_features.bluetooth_smoothing.supported {
        lines.push("Bluetooth Smoothing: Supported".to_owned());
    }
    if caps.scroll_jump.supported {
        let delay = caps.scroll_jump.delay;
        lines.push(format!(
            "Scroll Jump Protection: {}-{} ms, step {}",
            delay.min, delay.max, delay.step
        ));
    }
    lines.join("\n")
}

fn run_device_snapshot(requested_identity: Option<&str>) -> Result<CommandOutput, Error> {
    let device = Device::select(requested_identity)?;
    let physical = device.physical_device().clone();
    let capabilities = device.capabilities();
    let driver = device.open()?;
    let battery = capabilities
        .battery
        .supported
        .then(|| driver.get_battery_status())
        .transpose()?;
    let dpi = driver.get_dpi_config()?;
    let polling = driver.get_polling_config()?;
    let power = capabilities
        .power
        .supported
        .then(|| driver.get_power_config())
        .transpose()?;
    let wireless_features = (capabilities.wireless_features.wireless_stability.supported
        || capabilities.wireless_features.bluetooth_smoothing.supported)
        .then(|| driver.get_wireless_features())
        .transpose()?;
    let scroll_jump = capabilities
        .scroll_jump
        .supported
        .then(|| driver.get_scroll_jump_config())
        .transpose()?;
    let data = DeviceSnapshotData::new(
        &physical,
        battery,
        &dpi,
        polling,
        power.as_ref(),
        wireless_features.as_ref(),
        scroll_jump.as_ref(),
    );
    let human = format_device_snapshot(&data);
    Ok(CommandOutput::new("device.snapshot", human, data))
}

fn format_device_snapshot(snapshot: &DeviceSnapshotData) -> String {
    let id_label = if snapshot.device.id_source == IdentitySource::Custom.key() {
        IdentitySource::Custom.label()
    } else {
        IdentitySource::Hardware.label()
    };
    let connection = match snapshot.device.connection {
        "wired" => "Wired",
        "2.4_ghz" => "2.4 GHz",
        value => value,
    };
    let mut sections = vec![format!(
        "Device Snapshot:\nDevice: {}\n{id_label}: {}\nConnection: {connection}",
        snapshot.device.name, snapshot.device.id
    )];
    if let Some(battery) = &snapshot.battery {
        sections.push(match (battery.percentage, battery.charging) {
            (Some(percentage), Some(charging)) => format!(
                "Battery: {percentage}%\nCharging: {}",
                if charging { "Yes" } else { "No" }
            ),
            _ => "Battery: unavailable".to_owned(),
        });
    }
    let marker = |stage| {
        if stage == snapshot.dpi.active_stage {
            '>'
        } else {
            ' '
        }
    };
    let mut dpi = vec!["DPI Stages:".to_owned()];
    for stage in &snapshot.dpi.stages {
        let value = if stage.x == stage.y {
            format!("{} DPI", stage.x)
        } else {
            format!("{}x{} DPI", stage.x, stage.y)
        };
        dpi.push(format!("{} {}: {value}", marker(stage.stage), stage.stage));
    }
    sections.push(dpi.join("\n"));
    if let Some(lod) = &snapshot.lod {
        let mut lines = vec!["Lift-off Distance:".to_owned()];
        for stage in &lod.stages {
            let value = match stage.lod {
                "low" => "Low (1 mm)",
                "high" => "High (2 mm)",
                value => value,
            };
            lines.push(format!("{} {}: {value}", marker(stage.stage), stage.stage));
        }
        sections.push(lines.join("\n"));
    }
    sections.push(format_polling(&snapshot.polling));
    let on_off = |enabled| if enabled { "On" } else { "Off" };
    if let Some(power) = &snapshot.power {
        let timer = power.sleep_timer_minutes.map_or_else(
            || format!("{} ms", power.sleep_timer_ms),
            |minutes| format!("{minutes} min"),
        );
        sections.push(format!(
            "Power Management:\nLow Power Mode: {}\nLow Power Polling: {} Hz\nSleep Timer: {timer}\nAuto Low Power: {}\nAuto Low Power Threshold: {}%",
            on_off(power.low_power_enabled), power.low_power_polling_hz,
            on_off(power.auto_low_power_enabled), power.auto_low_power_threshold_percent));
    }
    let mut features = Vec::new();
    if let Some(state) = &snapshot.wireless_stability {
        features.push(format_wireless_stability(state.enabled));
    }
    if let Some(state) = &snapshot.bluetooth_smoothing {
        features.push(format_bluetooth_smoothing(state.enabled));
    }
    if let Some(state) = &snapshot.scroll_jump {
        features.push(format_scroll_jump(state.enabled, state.delay_ms));
    }
    if !features.is_empty() {
        sections.push(features.join("\n"));
    }
    sections.join("\n\n")
}

fn open_device(
    requested_identity: Option<&str>,
    feature: Option<DeviceFeature>,
) -> Result<Box<dyn DeviceDriver>, Error> {
    let device = Device::select(requested_identity)?;
    if let Some(feature) = feature {
        feature.require(device.physical_device().model)?;
    }
    device.open()
}

fn run_dpi(command: DpiCommand, requested_identity: Option<&str>) -> Result<CommandOutput, Error> {
    match command {
        DpiCommand::Get => {
            let device = open_device(requested_identity, None)?;
            let config = device.get_dpi_config()?;
            Ok(CommandOutput::new(
                "dpi.get",
                format_dpi_config(&config),
                DpiData::from_config(&config),
            ))
        }
        DpiCommand::Set { dpis } => {
            let values = dpis.iter().map(|dpi| (dpi.x, dpi.y)).collect::<Vec<_>>();
            if requested_identity.is_none() {
                validate_dpi_request(&values)?;
            }
            let selected = Device::select(requested_identity)?;
            validate_dpi_values(selected.physical_device().model, &values)?;
            let device = selected.open()?;
            let updated = device.replace_dpi_stages(&values)?;
            device.commit()?;
            Ok(CommandOutput::new(
                "dpi.set",
                dpi_stages_updated_message(&dpis),
                DpiData::from_config(&updated),
            ))
        }
        DpiCommand::Use { stage_id } => {
            let device = open_device(requested_identity, None)?;
            device.set_active_dpi_stage(stage_id)?;
            device.commit()?;
            Ok(CommandOutput::new(
                "dpi.use",
                format!("Active DPI stage changed to {stage_id}."),
                ActiveStageData {
                    active_stage: stage_id,
                },
            ))
        }
    }
}

fn format_dpi_config(config: &DpiConfig) -> String {
    let mut lines = vec!["DPI Stages:".to_owned()];
    for (index, stage) in config.stages.iter().enumerate() {
        let marker = if index == config.active { '>' } else { ' ' };
        lines.push(format!("{marker} {}: {}", index + 1, format_dpi(*stage)));
    }
    lines.join("\n")
}

fn format_dpi(stage: DpiStage) -> String {
    if stage.x == stage.y {
        format!("{} DPI", stage.x)
    } else {
        format!("{}x{} DPI", stage.x, stage.y)
    }
}

fn dpi_stages_updated_message(dpis: &[DpiInput]) -> String {
    let values = dpis
        .iter()
        .map(DpiInput::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!("DPI stages updated: {values} DPI.")
}

fn run_lod(command: LodCommand, requested_identity: Option<&str>) -> Result<CommandOutput, Error> {
    let device = open_device(requested_identity, Some(DeviceFeature::LiftOffDistance))?;
    match command {
        LodCommand::Get => {
            let config = device.get_dpi_config()?;
            Ok(CommandOutput::new(
                "lod.get",
                format_lod_config(&config),
                LodData::from_config(&config),
            ))
        }
        LodCommand::Set { distance, stage_id } => {
            let lod = LiftOffDistance::from(distance);
            device.set_lift_off_distance(stage_id, lod)?;
            device.commit()?;
            Ok(CommandOutput::new(
                "lod.set",
                format!("Stage {stage_id} lift-off distance set to {lod}."),
                LodSetData {
                    stage: stage_id,
                    lod: lod_name(lod),
                },
            ))
        }
    }
}

fn format_lod_config(config: &DpiConfig) -> String {
    let mut lines = vec!["Lift-off Distance:".to_owned()];
    for (index, stage) in config.stages.iter().enumerate() {
        let marker = if index == config.active { '>' } else { ' ' };
        lines.push(format!("{marker} {}: {}", index + 1, stage.lod));
    }
    lines.join("\n")
}

fn run_polling(
    command: PollingCommand,
    requested_identity: Option<&str>,
) -> Result<CommandOutput, Error> {
    match command {
        PollingCommand::Get => {
            let selected = Device::select(requested_identity)?;
            require_polling(
                selected.physical_device().model,
                selected.capabilities().polling,
            )?;
            let device = selected.open()?;
            let data = PollingData::from(device.get_polling_config()?);
            Ok(CommandOutput::new(
                "polling.get",
                format_polling(&data),
                data,
            ))
        }
        PollingCommand::Set { mode_or_rate, rate } => {
            let (mode, hz) = match (mode_or_rate, rate) {
                (PollingInput::Mode(mode), Some(hz)) => (Some(mode), hz),
                (PollingInput::Rate(hz), None) => (None, hz),
                _ => return Err(Error::InvalidPollingArguments),
            };
            let selected = Device::select(requested_identity)?;
            let (mode, rate) = resolve_polling_rate(
                selected.physical_device().model,
                selected.capabilities().polling,
                mode,
                hz,
            )?;
            let device = selected.open()?;
            match mode {
                PollingMode::Wireless => device.set_wireless_polling(rate)?,
                PollingMode::Wired => device.set_wired_polling(rate)?,
            }
            device.commit()?;
            let (connection, human) = match mode {
                PollingMode::Wireless => (
                    "wireless",
                    format!("Wireless polling rate changed to {rate}."),
                ),
                PollingMode::Wired => ("wired", format!("Wired polling rate changed to {rate}.")),
            };
            Ok(CommandOutput::new(
                "polling.set",
                human,
                PollingSetData {
                    connection,
                    hz: rate.hz(),
                },
            ))
        }
    }
}

fn require_polling(model: DeviceModel, caps: PollingCapabilities) -> Result<(), Error> {
    if caps.supported && (!caps.wired.is_empty() || !caps.wireless.is_empty()) {
        Ok(())
    } else {
        Err(Error::UnsupportedFeature {
            model,
            feature: "polling rate",
        })
    }
}

fn resolve_polling_rate(
    model: DeviceModel,
    caps: PollingCapabilities,
    mode: Option<PollingMode>,
    hz: u16,
) -> Result<(PollingMode, PollingRate), Error> {
    require_polling(model, caps)?;
    let mode = match mode {
        Some(mode) => mode,
        None => match (caps.wired.first(), caps.wireless.first()) {
            (Some(wired), Some(wireless)) => {
                return Err(Error::PollingModeRequired {
                    wired_hz: wired.hz(),
                    wireless_hz: wireless.hz(),
                });
            }
            (Some(_), None) => PollingMode::Wired,
            _ => PollingMode::Wireless,
        },
    };
    let (connection, feature, rates) = match mode {
        PollingMode::Wired => ("wired", "wired polling", caps.wired),
        PollingMode::Wireless => ("wireless", "wireless polling", caps.wireless),
    };
    if rates.is_empty() {
        return Err(Error::UnsupportedFeature { model, feature });
    }
    let rate =
        rates
            .iter()
            .copied()
            .find(|rate| rate.hz() == hz)
            .ok_or(Error::InvalidPollingRate {
                connection,
                hz,
                allowed: rates,
            })?;
    Ok((mode, rate))
}

fn format_polling(polling: &PollingData) -> String {
    match polling.wireless_hz {
        Some(wireless) => format!(
            "Polling Rate:\n  Wireless: {wireless} Hz\n  Wired:    {} Hz",
            polling.wired_hz
        ),
        None => format!("Polling Rate:\n  Wired: {} Hz", polling.wired_hz),
    }
}

fn run_battery(
    command: BatteryCommand,
    requested_identity: Option<&str>,
) -> Result<CommandOutput, Error> {
    match command {
        BatteryCommand::Get => {
            let device = open_device(requested_identity, Some(DeviceFeature::Battery))?;
            let status = device.get_battery_status()?;
            Ok(CommandOutput::new(
                "battery.get",
                format_battery_status(status),
                BatteryData::from(status),
            ))
        }
    }
}

fn format_battery_status(status: BatteryStatus) -> String {
    match status {
        BatteryStatus::Available { percent, charging } => format!(
            "Battery: {percent}%\nCharging: {}",
            if charging { "Yes" } else { "No" }
        ),
        BatteryStatus::Unavailable => "Battery: unavailable".to_owned(),
    }
}

fn run_power(
    command: PowerCommand,
    requested_identity: Option<&str>,
) -> Result<CommandOutput, Error> {
    match command {
        PowerCommand::Get => {
            let device = open_device(requested_identity, Some(DeviceFeature::Power))?;
            let config = device.get_power_config()?;
            Ok(CommandOutput::new(
                "power.get",
                format_power_config(&config),
                PowerData::from(&config),
            ))
        }
        PowerCommand::LowPower { command } => match command {
            LowPowerCommand::Set { state } => {
                let device = open_device(requested_identity, Some(DeviceFeature::Power))?;
                device.set_low_power_enabled(state.enabled())?;
                device.commit()?;
                Ok(CommandOutput::new(
                    "power.low-power.set",
                    format!("Low Power Mode set to {state}."),
                    LowPowerEnabledData {
                        low_power_enabled: state.enabled(),
                    },
                ))
            }
            LowPowerCommand::Polling { rate } => {
                let rate = LowPowerPollingRate::try_from(rate)?;
                let device = open_device(requested_identity, Some(DeviceFeature::Power))?;
                device.set_low_power_polling(rate)?;
                device.commit()?;
                Ok(CommandOutput::new(
                    "power.low-power.polling",
                    format!("Low Power polling set to {rate}."),
                    LowPowerPollingData {
                        low_power_polling_hz: rate.hz(),
                    },
                ))
            }
        },
        PowerCommand::AutoLowPower { command } => match command {
            AutoLowPowerCommand::Set { state } => {
                let device = open_device(requested_identity, Some(DeviceFeature::Power))?;
                device.set_auto_low_power_enabled(state.enabled())?;
                device.commit()?;
                Ok(CommandOutput::new(
                    "power.auto-low-power.set",
                    format!("Auto Low Power set to {state}."),
                    AutoLowPowerEnabledData {
                        auto_low_power_enabled: state.enabled(),
                    },
                ))
            }
            AutoLowPowerCommand::Threshold { percent } => {
                let threshold = auto_low_power_threshold(percent)?;
                let device = open_device(requested_identity, Some(DeviceFeature::Power))?;
                device.set_auto_low_power_threshold(threshold)?;
                device.commit()?;
                Ok(CommandOutput::new(
                    "power.auto-low-power.threshold",
                    format!("Auto Low Power threshold set to {percent}%."),
                    AutoLowPowerThresholdData {
                        auto_low_power_threshold_percent: percent,
                    },
                ))
            }
        },
        PowerCommand::Sleep { command } => match command {
            SleepCommand::Set { minutes } => {
                let timer = sleep_timer_from_minutes(minutes)?;
                let device = open_device(requested_identity, Some(DeviceFeature::Power))?;
                device.set_sleep_timer(timer)?;
                device.commit()?;
                Ok(CommandOutput::new(
                    "power.sleep.set",
                    format!("Sleep timer set to {minutes} min."),
                    SleepTimerData {
                        sleep_timer_minutes: minutes,
                    },
                ))
            }
        },
    }
}

fn format_power_config(config: &PowerConfig) -> String {
    format!(
        "Power Management:\nLow Power Mode: {}\nLow Power Polling: {}\nSleep Timer: {}\nAuto Low Power: {}\nAuto Low Power Threshold: {}%",
        if config.low_power_enabled {
            "On"
        } else {
            "Off"
        },
        config.low_power_polling,
        config.sleep_timer,
        if config.auto_low_power_enabled {
            "On"
        } else {
            "Off"
        },
        config.auto_low_power_threshold.percent()
    )
}

fn run_wireless_stability(
    command: FeatureToggleCommand,
    requested_identity: Option<&str>,
) -> Result<CommandOutput, Error> {
    let device = open_device(requested_identity, Some(DeviceFeature::WirelessStability))?;
    match command {
        FeatureToggleCommand::Get => {
            let features = device.get_wireless_features()?;
            Ok(CommandOutput::new(
                "wireless-stability.get",
                format_wireless_stability(features.wireless_stability_enabled),
                EnabledData {
                    enabled: features.wireless_stability_enabled,
                },
            ))
        }
        FeatureToggleCommand::Set { state } => {
            apply_wireless_feature_change(
                || device.set_wireless_stability(state.enabled()),
                || device.commit(),
            )?;
            Ok(CommandOutput::new(
                "wireless-stability.set",
                format!(
                    "Wireless Stability Enhancement {}.",
                    if state.enabled() {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
                EnabledData {
                    enabled: state.enabled(),
                },
            ))
        }
    }
}

fn run_bluetooth_smoothing(
    command: FeatureToggleCommand,
    requested_identity: Option<&str>,
) -> Result<CommandOutput, Error> {
    let device = open_device(requested_identity, Some(DeviceFeature::BluetoothSmoothing))?;
    match command {
        FeatureToggleCommand::Get => {
            let features = device.get_wireless_features()?;
            Ok(CommandOutput::new(
                "bluetooth-smoothing.get",
                format_bluetooth_smoothing(features.bluetooth_smoothing_enabled),
                EnabledData {
                    enabled: features.bluetooth_smoothing_enabled,
                },
            ))
        }
        FeatureToggleCommand::Set { state } => {
            apply_wireless_feature_change(
                || device.set_bluetooth_smoothing(state.enabled()),
                || device.commit(),
            )?;
            Ok(CommandOutput::new(
                "bluetooth-smoothing.set",
                format!(
                    "Bluetooth Smoothing {}.",
                    if state.enabled() {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
                EnabledData {
                    enabled: state.enabled(),
                },
            ))
        }
    }
}

fn apply_wireless_feature_change<S, C>(set: S, commit: C) -> Result<(), Error>
where
    S: FnOnce() -> Result<(), Error>,
    C: FnOnce() -> Result<(), Error>,
{
    set()?;
    commit()
}

fn format_wireless_stability(enabled: bool) -> String {
    format!(
        "Wireless Stability Enhancement: {}",
        if enabled { "On" } else { "Off" }
    )
}

fn format_bluetooth_smoothing(enabled: bool) -> String {
    format!(
        "Bluetooth Smoothing: {}",
        if enabled { "On" } else { "Off" }
    )
}

fn run_scroll_jump(
    command: ScrollJumpCommand,
    requested_identity: Option<&str>,
) -> Result<CommandOutput, Error> {
    match command {
        ScrollJumpCommand::Get => {
            let device = open_device(requested_identity, Some(DeviceFeature::ScrollJump))?;
            let config = device.get_scroll_jump_config()?;
            Ok(CommandOutput::new(
                "scroll-jump.get",
                format_scroll_jump(config.enabled, config.delay_ms),
                ScrollJumpData {
                    enabled: config.enabled,
                    delay_ms: config.delay_ms,
                },
            ))
        }
        ScrollJumpCommand::Set { state } => {
            let device = open_device(requested_identity, Some(DeviceFeature::ScrollJump))?;
            apply_scroll_jump_change(
                || device.set_scroll_jump_enabled(state.enabled()),
                || device.commit(),
            )?;
            Ok(CommandOutput::new(
                "scroll-jump.set",
                format!(
                    "Scroll Jump Protection {}.",
                    if state.enabled() {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
                EnabledData {
                    enabled: state.enabled(),
                },
            ))
        }
        ScrollJumpCommand::Delay { milliseconds } => {
            validate_scroll_jump_delay(milliseconds)?;
            let device = open_device(requested_identity, Some(DeviceFeature::ScrollJump))?;
            apply_scroll_jump_change(
                || device.set_scroll_jump_delay(milliseconds),
                || device.commit(),
            )?;
            Ok(CommandOutput::new(
                "scroll-jump.delay",
                format!("Scroll Jump Protection delay set to {milliseconds} ms."),
                DelayData {
                    delay_ms: milliseconds,
                },
            ))
        }
    }
}

fn format_scroll_jump(enabled: bool, delay_ms: u16) -> String {
    format!(
        "Scroll Jump Protection: {}\nDelay: {} ms",
        if enabled { "On" } else { "Off" },
        delay_ms
    )
}

fn apply_scroll_jump_change<S, C>(set: S, commit: C) -> Result<(), Error>
where
    S: FnOnce() -> Result<(), Error>,
    C: FnOnce() -> Result<(), Error>,
{
    set()?;
    commit()
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{
        AutoLowPowerCommand, BatteryCommand, Cli, Command, DeviceCommand, DpiCommand, DpiInput,
        FeatureToggleCommand, LodCommand, LodValue, LowPowerCommand, PollingCommand, PollingMode,
        PowerCommand, PowerState, ScrollJumpCommand, SleepCommand, apply_scroll_jump_change,
        apply_wireless_feature_change, command_identifier, dpi_stages_updated_message,
        format_bluetooth_smoothing, format_device_description, format_device_snapshot,
        format_dpi_config, format_lod_config, format_scroll_jump, format_wireless_stability,
    };
    use steelseries_core::{
        ConnectionType, DeviceIdentity, DeviceModel, DpiConfig, DpiStage, LiftOffDistance,
        PhysicalDevice,
    };

    use crate::output::{
        BatteryData, DeviceData, DeviceSnapshotData, DpiData, DpiStageData, EnabledData, LodData,
        LodStageData, PollingData, PowerData, ScrollJumpData,
    };

    fn stage_config() -> DpiConfig {
        DpiConfig {
            stages: vec![
                DpiStage::scalar(1450),
                DpiStage {
                    x: 1500,
                    y: 1500,
                    lod: LiftOffDistance::High,
                },
                DpiStage::scalar(1550),
            ],
            active: 1,
        }
    }

    fn physical_device() -> PhysicalDevice {
        PhysicalDevice {
            model: DeviceModel::Aerox3WirelessGen2,
            identity: DeviceIdentity::new("6271700431492500250"),
            active_connection: ConnectionType::Wireless2_4Ghz,
            wired_endpoint_available: false,
            receiver_endpoint_available: true,
        }
    }

    #[test]
    fn formats_single_dpi_stage_success_message() {
        assert_eq!(
            dpi_stages_updated_message(&[DpiInput { x: 800, y: 800 }]),
            "DPI stages updated: 800 DPI."
        );
    }

    #[test]
    fn formats_multiple_dpi_stages_success_message() {
        assert_eq!(
            dpi_stages_updated_message(&[
                DpiInput { x: 400, y: 400 },
                DpiInput { x: 800, y: 1600 },
                DpiInput { x: 3200, y: 3200 },
            ]),
            "DPI stages updated: 400, 800x1600, 3200 DPI."
        );
    }

    #[test]
    fn parses_scalar_and_axis_specific_dpi_inputs() {
        assert_eq!(
            "1600".parse::<DpiInput>().unwrap(),
            DpiInput { x: 1600, y: 1600 }
        );
        assert_eq!(
            "800x1600".parse::<DpiInput>().unwrap(),
            DpiInput { x: 800, y: 1600 }
        );
        assert_eq!(
            "800X1600".parse::<DpiInput>().unwrap(),
            DpiInput { x: 800, y: 1600 }
        );
    }

    #[test]
    fn rejects_malformed_and_overflowing_dpi_inputs() {
        for value in [
            "x1600",
            "800x",
            "800x1600x3200",
            "abc",
            "800xabc",
            "70000",
            "800x70000",
        ] {
            assert!(value.parse::<DpiInput>().is_err(), "accepted {value}");
        }
    }

    #[test]
    fn parses_mixed_dpi_stage_list() {
        let cli = Cli::try_parse_from(["steelseriesctl", "dpi", "set", "400", "800x1600", "3200"])
            .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Dpi {
                command: Some(DpiCommand::Set { dpis })
            }) if dpis == vec![
                DpiInput { x: 400, y: 400 },
                DpiInput { x: 800, y: 1600 },
                DpiInput { x: 3200, y: 3200 },
            ]
        ));
    }

    #[test]
    fn dpi_set_still_requires_one_to_five_stages() {
        assert!(Cli::try_parse_from(["steelseriesctl", "dpi", "set"]).is_err());
        let too_many = Cli::try_parse_from([
            "steelseriesctl",
            "dpi",
            "set",
            "100",
            "200",
            "300",
            "400",
            "500",
            "600",
        ])
        .unwrap();
        assert!(matches!(
            super::run(too_many),
            Err(steelseries_core::Error::InvalidDpiStageCount(6))
        ));
    }

    #[test]
    fn formats_dpi_get_without_separate_active_line() {
        let mut config = stage_config();
        config.stages[1].y = 1600;
        let output = format_dpi_config(&config);
        assert_eq!(
            output,
            "DPI Stages:\n  1: 1450 DPI\n> 2: 1500x1600 DPI\n  3: 1550 DPI"
        );
        assert!(!output.contains("Active:"));
    }

    #[test]
    fn formats_lod_get_with_active_stage_marker() {
        assert_eq!(
            format_lod_config(&stage_config()),
            "Lift-off Distance:\n  1: Low (1 mm)\n> 2: High (2 mm)\n  3: Low (1 mm)"
        );
    }

    #[test]
    fn parses_dpi_get_without_device_selector() {
        let cli = Cli::try_parse_from(["steelseriesctl", "dpi", "get"]).unwrap();

        assert_eq!(cli.device, None);
        assert!(matches!(
            cli.command,
            Some(Command::Dpi {
                command: Some(DpiCommand::Get)
            })
        ));
    }

    #[test]
    fn parses_global_json_before_or_after_subcommands() {
        let before = Cli::try_parse_from(["steelseriesctl", "--json", "dpi", "get"]).unwrap();
        assert!(before.json);
        assert_eq!(command_identifier(&before), "dpi.get");

        let after = Cli::try_parse_from(["steelseriesctl", "dpi", "get", "--json"]).unwrap();
        assert!(after.json);
        assert_eq!(command_identifier(&after), "dpi.get");

        let selected = Cli::try_parse_from([
            "steelseriesctl",
            "--device",
            "6271700431492500250",
            "--json",
            "battery",
            "get",
        ])
        .unwrap();
        assert!(selected.json);
        assert_eq!(selected.device.as_deref(), Some("6271700431492500250"));
        assert_eq!(command_identifier(&selected), "battery.get");

        let device = Cli::try_parse_from([
            "steelseriesctl",
            "device",
            "describe",
            "--json",
            "--device",
            "6271700431492500250",
        ])
        .unwrap();
        assert!(device.json);
        assert_eq!(device.device.as_deref(), Some("6271700431492500250"));
        assert_eq!(command_identifier(&device), "device.describe");
    }

    #[test]
    fn uses_stable_json_command_identifiers() {
        for (arguments, expected) in [
            (&["device", "list"][..], "device.list"),
            (&["device", "describe"][..], "device.describe"),
            (&["device", "snapshot"][..], "device.snapshot"),
            (&["dpi", "get"][..], "dpi.get"),
            (&["lod", "get"][..], "lod.get"),
            (&["polling", "get"][..], "polling.get"),
            (&["battery", "get"][..], "battery.get"),
            (&["power", "get"][..], "power.get"),
            (
                &["wireless-stability", "set", "on"][..],
                "wireless-stability.set",
            ),
            (
                &["bluetooth-smoothing", "get"][..],
                "bluetooth-smoothing.get",
            ),
            (&["scroll-jump", "delay", "500"][..], "scroll-jump.delay"),
        ] {
            let cli = Cli::try_parse_from(
                std::iter::once("steelseriesctl").chain(arguments.iter().copied()),
            )
            .unwrap();
            assert_eq!(command_identifier(&cli), expected);
        }
    }

    #[test]
    fn device_command_group_replaces_devices_and_help_needs_no_hardware() {
        assert!(Cli::try_parse_from(["steelseriesctl", "devices"]).is_err());

        let cli = Cli::try_parse_from(["steelseriesctl", "device"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Device { command: None })
        ));
        let output = super::run(cli).unwrap();
        assert!(output.human().contains("steelseriesctl device list"));

        for (name, expected) in [
            ("list", DeviceCommand::List),
            ("describe", DeviceCommand::Describe),
            ("snapshot", DeviceCommand::Snapshot),
        ] {
            let cli = Cli::try_parse_from(["steelseriesctl", "device", name]).unwrap();
            assert!(matches!(
                (cli.command, expected),
                (
                    Some(Command::Device {
                        command: Some(DeviceCommand::List)
                    }),
                    DeviceCommand::List
                ) | (
                    Some(Command::Device {
                        command: Some(DeviceCommand::Describe)
                    }),
                    DeviceCommand::Describe
                ) | (
                    Some(Command::Device {
                        command: Some(DeviceCommand::Snapshot)
                    }),
                    DeviceCommand::Snapshot
                )
            ));
        }
    }

    #[test]
    fn formats_device_describe_human_output() {
        let output = format_device_description(&physical_device());
        assert_eq!(
            output,
            "Device: Aerox 3 Wireless Gen 2\nID: 6271700431492500250\nConnection: 2.4 GHz\n\nCapabilities:\nDPI: 50-26000, step 50, max 5 stages, independent X/Y\nLOD: Low / High, per stage\nWireless Polling: 125/250/500/1000/2000/4000 Hz\nWired Polling: 125/250/500/1000 Hz\nBattery: Supported (charging state)\nLow Power Mode: Supported\nLow Power Polling: 125/250/500 Hz\nSleep Timer: 1-71582 min, step 1\nAuto Low Power Threshold: 5-25%, step 1\nWireless Stability Enhancement: Supported\nBluetooth Smoothing: Supported\nScroll Jump Protection: 100-1500 ms, step 100"
        );
    }

    #[test]
    fn formats_device_snapshot_human_output() {
        let snapshot = DeviceSnapshotData {
            device: DeviceData {
                battery_supported: true,
                id: "6271700431492500250".to_owned(),
                id_source: "hardware",
                wired_endpoint_available: true,
                receiver_endpoint_available: false,
                name: "Aerox 3 Wireless Gen 2",
                model_key: "aerox_3_wireless_gen2",
                connection: "2.4_ghz",
            },
            battery: Some(BatteryData {
                percentage: Some(87),
                charging: Some(false),
            }),
            dpi: DpiData {
                active_stage: 2,
                stages: vec![
                    DpiStageData {
                        stage: 1,
                        x: 400,
                        y: 400,
                    },
                    DpiStageData {
                        stage: 2,
                        x: 800,
                        y: 1600,
                    },
                ],
            },
            lod: Some(LodData {
                stages: vec![
                    LodStageData {
                        stage: 1,
                        lod: "low",
                    },
                    LodStageData {
                        stage: 2,
                        lod: "high",
                    },
                ],
            }),
            polling: PollingData {
                wireless_hz: Some(4_000),
                wired_hz: 1_000,
            },
            power: Some(PowerData {
                low_power_enabled: false,
                low_power_polling_hz: 500,
                sleep_timer_minutes: Some(5),
                sleep_timer_ms: 300_000,
                auto_low_power_enabled: true,
                auto_low_power_threshold_percent: 10,
            }),
            wireless_stability: Some(EnabledData { enabled: true }),
            bluetooth_smoothing: Some(EnabledData { enabled: false }),
            scroll_jump: Some(ScrollJumpData {
                enabled: true,
                delay_ms: 500,
            }),
        };
        let output = format_device_snapshot(&snapshot);
        assert_eq!(
            output,
            "Device Snapshot:\nDevice: Aerox 3 Wireless Gen 2\nID: 6271700431492500250\nConnection: 2.4 GHz\n\nBattery: 87%\nCharging: No\n\nDPI Stages:\n  1: 400 DPI\n> 2: 800x1600 DPI\n\nLift-off Distance:\n  1: Low (1 mm)\n> 2: High (2 mm)\n\nPolling Rate:\n  Wireless: 4000 Hz\n  Wired:    1000 Hz\n\nPower Management:\nLow Power Mode: Off\nLow Power Polling: 500 Hz\nSleep Timer: 5 min\nAuto Low Power: On\nAuto Low Power Threshold: 10%\n\nWireless Stability Enhancement: On\nBluetooth Smoothing: Off\nScroll Jump Protection: On\nDelay: 500 ms"
        );
    }

    #[test]
    fn parses_long_device_selector_for_dpi_get() {
        let cli = Cli::try_parse_from([
            "steelseriesctl",
            "--device",
            "6271700431492500250",
            "dpi",
            "get",
        ])
        .unwrap();

        assert_eq!(cli.device.as_deref(), Some("6271700431492500250"));
        assert!(matches!(
            cli.command,
            Some(Command::Dpi {
                command: Some(DpiCommand::Get)
            })
        ));
    }

    #[test]
    fn parses_dpi_use_as_stage_id() {
        let cli = Cli::try_parse_from(["steelseriesctl", "dpi", "use", "5"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Dpi {
                command: Some(DpiCommand::Use { stage_id: 5 })
            })
        ));
    }

    #[test]
    fn dpi_use_no_longer_accepts_a_dpi_value() {
        assert!(Cli::try_parse_from(["steelseriesctl", "dpi", "use", "800"]).is_err());
    }

    #[test]
    fn parses_lod_get_and_set() {
        let get = Cli::try_parse_from(["steelseriesctl", "lod", "get"]).unwrap();
        assert!(matches!(
            get.command,
            Some(Command::Lod {
                command: Some(LodCommand::Get)
            })
        ));

        let set = Cli::try_parse_from(["steelseriesctl", "lod", "set", "high", "4"]).unwrap();
        assert!(matches!(
            set.command,
            Some(Command::Lod {
                command: Some(LodCommand::Set {
                    distance: LodValue::High,
                    stage_id: 4,
                })
            })
        ));
    }

    #[test]
    fn parses_short_device_selector_for_battery_get() {
        let cli = Cli::try_parse_from([
            "steelseriesctl",
            "-d",
            "6271700431492500250",
            "battery",
            "get",
        ])
        .unwrap();

        assert_eq!(cli.device.as_deref(), Some("6271700431492500250"));
        assert!(matches!(
            cli.command,
            Some(Command::Battery {
                command: Some(BatteryCommand::Get)
            })
        ));
    }

    #[test]
    fn parses_device_selector_for_polling_set() {
        let cli = Cli::try_parse_from([
            "steelseriesctl",
            "--device",
            "6271700431492500250",
            "polling",
            "set",
            "wireless",
            "1000",
        ])
        .unwrap();

        assert_eq!(cli.device.as_deref(), Some("6271700431492500250"));
        assert!(matches!(
            cli.command,
            Some(Command::Polling {
                command: Some(PollingCommand::Set {
                    mode_or_rate: super::PollingInput::Mode(PollingMode::Wireless),
                    rate: Some(1000),
                })
            })
        ));
    }

    #[test]
    fn parses_short_and_explicit_polling_set_forms_with_global_options() {
        let cli = Cli::try_parse_from([
            "steelseriesctl",
            "-d",
            "mouse",
            "polling",
            "set",
            "500",
            "--json",
        ])
        .unwrap();
        assert_eq!(cli.device.as_deref(), Some("mouse"));
        assert!(cli.json);
        assert!(matches!(
            cli.command,
            Some(Command::Polling {
                command: Some(PollingCommand::Set {
                    mode_or_rate: super::PollingInput::Rate(500),
                    rate: None
                })
            })
        ));

        for (input, expected) in [
            ("wired", PollingMode::Wired),
            ("wireless", PollingMode::Wireless),
        ] {
            let cli = Cli::try_parse_from([
                "steelseriesctl",
                "polling",
                "set",
                input,
                "--json",
                "1000",
                "--device",
                "mouse",
            ])
            .unwrap();
            assert!(matches!(cli.command, Some(Command::Polling {
                command: Some(PollingCommand::Set { mode_or_rate: super::PollingInput::Mode(mode), rate: Some(1000) })
            }) if mode == expected));
        }
        assert!(Cli::try_parse_from(["steelseriesctl", "polling", "set", "wired"]).is_err());
        assert!(Cli::try_parse_from(["steelseriesctl", "polling", "set", "invalid"]).is_err());
    }

    #[test]
    fn infers_wired_and_wireless_from_single_mode_capabilities() {
        use super::{PollingCapabilities, PollingRate, resolve_polling_rate};
        for (caps, expected) in [
            (
                PollingCapabilities {
                    supported: true,
                    wired: &[PollingRate::Hz500],
                    wireless: &[],
                },
                PollingMode::Wired,
            ),
            (
                PollingCapabilities {
                    supported: true,
                    wired: &[],
                    wireless: &[PollingRate::Hz500],
                },
                PollingMode::Wireless,
            ),
        ] {
            let result =
                resolve_polling_rate(DeviceModel::Aerox3WirelessGen2, caps, None, 500).unwrap();
            assert_eq!(result, (expected, PollingRate::Hz500));
        }
    }

    #[test]
    fn dual_mode_polling_requires_explicit_mode_and_suggests_valid_examples() {
        use super::{Error, PollingCapabilities, PollingRate, resolve_polling_rate};
        let caps = PollingCapabilities {
            supported: true,
            wired: &[PollingRate::Hz500],
            wireless: &[PollingRate::Hz4000],
        };
        let error = resolve_polling_rate(DeviceModel::Aerox3Wired, caps, None, 500).unwrap_err();
        assert!(matches!(
            error,
            Error::PollingModeRequired {
                wired_hz: 500,
                wireless_hz: 4000
            }
        ));
        let document: serde_json::Value =
            serde_json::from_str(&super::error_document("polling.set", &error)).unwrap();
        assert_eq!(document["error"]["code"], "invalid_argument");
        assert_eq!(document["error"]["field"], "mode");
        assert!(
            error
                .to_string()
                .contains("steelseriesctl polling set wired 500")
        );
        assert!(
            error
                .to_string()
                .contains("steelseriesctl polling set wireless 4000")
        );
        assert_eq!(
            resolve_polling_rate(
                DeviceModel::Aerox3Wired,
                caps,
                Some(PollingMode::Wireless),
                4000
            )
            .unwrap(),
            (PollingMode::Wireless, PollingRate::Hz4000)
        );
    }

    #[test]
    fn unsupported_polling_and_invalid_rates_use_selected_capabilities() {
        use super::{Error, PollingCapabilities, PollingRate, resolve_polling_rate};
        let model = DeviceModel::Aerox3Wired;
        let caps = PollingCapabilities {
            supported: true,
            wired: &[PollingRate::Hz500],
            wireless: &[],
        };
        assert!(matches!(
            resolve_polling_rate(model, caps, Some(PollingMode::Wireless), 500),
            Err(Error::UnsupportedFeature {
                feature: "wireless polling",
                ..
            })
        ));
        assert!(matches!(
            resolve_polling_rate(
                model,
                PollingCapabilities {
                    supported: false,
                    ..caps
                },
                None,
                500
            ),
            Err(Error::UnsupportedFeature {
                feature: "polling rate",
                ..
            })
        ));
        for hz in [1000, 333] {
            let error = resolve_polling_rate(model, caps, None, hz).unwrap_err();
            let document: serde_json::Value =
                serde_json::from_str(&super::error_document("polling.set", &error)).unwrap();
            assert_eq!(document["error"]["code"], "invalid_argument");
            assert_eq!(document["error"]["field"], "wired_hz");
            assert_eq!(document["error"]["details"]["value"], hz);
            assert_eq!(
                document["error"]["details"]["allowed"],
                serde_json::json!([500])
            );
        }
    }

    #[test]
    fn parses_power_command_hierarchy() {
        assert!(matches!(
            Cli::try_parse_from(["steelseriesctl", "power", "get"])
                .unwrap()
                .command,
            Some(Command::Power {
                command: Some(PowerCommand::Get)
            })
        ));
        assert!(matches!(
            Cli::try_parse_from(["steelseriesctl", "power", "low-power", "set", "on"])
                .unwrap()
                .command,
            Some(Command::Power {
                command: Some(PowerCommand::LowPower {
                    command: LowPowerCommand::Set {
                        state: PowerState::On
                    }
                })
            })
        ));
        assert!(matches!(
            Cli::try_parse_from([
                "steelseriesctl",
                "power",
                "auto-low-power",
                "threshold",
                "25"
            ])
            .unwrap()
            .command,
            Some(Command::Power {
                command: Some(PowerCommand::AutoLowPower {
                    command: AutoLowPowerCommand::Threshold { percent: 25 }
                })
            })
        ));
        assert!(matches!(
            Cli::try_parse_from(["steelseriesctl", "power", "sleep", "set", "1440"])
                .unwrap()
                .command,
            Some(Command::Power {
                command: Some(PowerCommand::Sleep {
                    command: SleepCommand::Set { minutes: 1440 }
                })
            })
        ));
    }

    #[test]
    fn rejects_invalid_dpi_and_power_values_before_device_discovery() {
        let dpi = Cli::try_parse_from(["steelseriesctl", "dpi", "set", "49"]).unwrap();
        assert!(matches!(
            super::run(dpi),
            Err(steelseries_core::Error::InvalidDpiValue { value: 49, .. })
        ));

        let threshold = Cli::try_parse_from([
            "steelseriesctl",
            "power",
            "auto-low-power",
            "threshold",
            "4",
        ])
        .unwrap();
        assert!(matches!(
            super::run(threshold),
            Err(steelseries_core::Error::InvalidAutoLowPowerThreshold(4))
        ));

        let sleep =
            Cli::try_parse_from(["steelseriesctl", "power", "sleep", "set", "71583"]).unwrap();
        assert!(matches!(
            super::run(sleep),
            Err(steelseries_core::Error::SleepTimerTooLarge)
        ));

        let polling =
            Cli::try_parse_from(["steelseriesctl", "power", "low-power", "polling", "1000"])
                .unwrap();
        assert!(matches!(
            super::run(polling),
            Err(steelseries_core::Error::UnsupportedLowPowerPollingRate(
                1000
            ))
        ));
    }

    #[test]
    fn wireless_feature_commands_without_subcommands_show_help_without_a_device() {
        let stability = Cli::try_parse_from(["steelseriesctl", "wireless-stability"]).unwrap();
        assert!(matches!(
            stability.command,
            Some(Command::WirelessStability { command: None })
        ));
        super::run(stability).unwrap();

        let smoothing = Cli::try_parse_from(["steelseriesctl", "bluetooth-smoothing"]).unwrap();
        assert!(matches!(
            smoothing.command,
            Some(Command::BluetoothSmoothing { command: None })
        ));
        super::run(smoothing).unwrap();
    }

    #[test]
    fn parses_wireless_feature_get_and_on_off_set_commands() {
        let stability =
            Cli::try_parse_from(["steelseriesctl", "wireless-stability", "get"]).unwrap();
        assert!(matches!(
            stability.command,
            Some(Command::WirelessStability {
                command: Some(FeatureToggleCommand::Get)
            })
        ));

        for command in ["wireless-stability", "bluetooth-smoothing"] {
            for (value, expected) in [("on", PowerState::On), ("off", PowerState::Off)] {
                let cli = Cli::try_parse_from(["steelseriesctl", command, "set", value]).unwrap();
                assert!(matches!(
                    cli.command,
                    Some(Command::WirelessStability {
                        command: Some(FeatureToggleCommand::Set { state })
                    }) | Some(Command::BluetoothSmoothing {
                        command: Some(FeatureToggleCommand::Set { state })
                    }) if state == expected
                ));
            }
            assert!(Cli::try_parse_from(["steelseriesctl", command, "set", "enabled"]).is_err());
        }
    }

    #[test]
    fn formats_wireless_feature_get_states() {
        assert_eq!(
            format_wireless_stability(true),
            "Wireless Stability Enhancement: On"
        );
        assert_eq!(
            format_wireless_stability(false),
            "Wireless Stability Enhancement: Off"
        );
        assert_eq!(format_bluetooth_smoothing(true), "Bluetooth Smoothing: On");
        assert_eq!(
            format_bluetooth_smoothing(false),
            "Bluetooth Smoothing: Off"
        );
    }

    #[test]
    fn wireless_feature_set_commits_once_and_only_after_set() {
        use std::{cell::RefCell, rc::Rc};

        let calls = Rc::new(RefCell::new(Vec::new()));
        let set_calls = Rc::clone(&calls);
        let commit_calls = Rc::clone(&calls);
        apply_wireless_feature_change(
            || {
                set_calls.borrow_mut().push("set");
                Ok(())
            },
            || {
                commit_calls.borrow_mut().push("commit");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*calls.borrow(), ["set", "commit"]);

        let commit_count = std::cell::Cell::new(0);
        let result = apply_wireless_feature_change(
            || Err(steelseries_core::Error::InvalidStageId(0)),
            || {
                commit_count.set(commit_count.get() + 1);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(commit_count.get(), 0);
    }

    #[test]
    fn scroll_jump_without_subcommand_shows_help_without_a_device() {
        let cli = Cli::try_parse_from(["steelseriesctl", "scroll-jump"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::ScrollJump { command: None })
        ));
        super::run(cli).unwrap();
    }

    #[test]
    fn parses_scroll_jump_commands() {
        let get = Cli::try_parse_from(["steelseriesctl", "scroll-jump", "get"]).unwrap();
        assert!(matches!(
            get.command,
            Some(Command::ScrollJump {
                command: Some(ScrollJumpCommand::Get)
            })
        ));

        for (value, expected) in [("on", PowerState::On), ("off", PowerState::Off)] {
            let set = Cli::try_parse_from(["steelseriesctl", "scroll-jump", "set", value]).unwrap();
            assert!(matches!(
                set.command,
                Some(Command::ScrollJump {
                    command: Some(ScrollJumpCommand::Set { state })
                }) if state == expected
            ));
        }
        assert!(Cli::try_parse_from(["steelseriesctl", "scroll-jump", "set", "enabled"]).is_err());

        let delay = Cli::try_parse_from(["steelseriesctl", "scroll-jump", "delay", "500"]).unwrap();
        assert!(matches!(
            delay.command,
            Some(Command::ScrollJump {
                command: Some(ScrollJumpCommand::Delay { milliseconds: 500 })
            })
        ));
    }

    #[test]
    fn formats_scroll_jump_get_state() {
        assert_eq!(
            format_scroll_jump(true, 500),
            "Scroll Jump Protection: On\nDelay: 500 ms"
        );
        assert_eq!(
            format_scroll_jump(false, 1_500),
            "Scroll Jump Protection: Off\nDelay: 1500 ms"
        );
    }

    #[test]
    fn rejects_invalid_scroll_jump_delays_before_device_discovery() {
        for milliseconds in [99, 101, 250, 1_501] {
            let cli = Cli::try_parse_from([
                "steelseriesctl",
                "scroll-jump",
                "delay",
                &milliseconds.to_string(),
            ])
            .unwrap();
            assert!(matches!(
                super::run(cli),
                Err(steelseries_core::Error::InvalidScrollJumpDelay(value))
                    if value == milliseconds
            ));
        }
    }

    #[test]
    fn scroll_jump_set_commits_once_and_only_after_set() {
        use std::{cell::RefCell, rc::Rc};

        let calls = Rc::new(RefCell::new(Vec::new()));
        let set_calls = Rc::clone(&calls);
        let commit_calls = Rc::clone(&calls);
        apply_scroll_jump_change(
            || {
                set_calls.borrow_mut().push("set");
                Ok(())
            },
            || {
                commit_calls.borrow_mut().push("commit");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*calls.borrow(), ["set", "commit"]);

        let commit_count = std::cell::Cell::new(0);
        let result = apply_scroll_jump_change(
            || Err(steelseries_core::Error::InvalidScrollJumpDelay(99)),
            || {
                commit_count.set(commit_count.get() + 1);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(commit_count.get(), 0);
    }
}
