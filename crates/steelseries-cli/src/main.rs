use std::{fmt, process::ExitCode, str::FromStr};

mod output;

use clap::{Parser, Subcommand, ValueEnum};
use steelseries_core::devices::aerox_3_wireless_gen2::{
    auto_low_power_threshold, config_from_dpi_values, sleep_timer_from_minutes,
    validate_dpi_values, validate_scroll_jump_delay, validate_stage_id,
    validate_wired_polling_rate,
};
use steelseries_core::{
    Aerox3WirelessGen2, BatteryStatus, DeviceModel, DpiConfig, DpiStage, Error, LiftOffDistance,
    LowPowerPollingRate, PollingRate, PowerConfig,
};

use output::{
    ActiveStageData, AutoLowPowerEnabledData, AutoLowPowerThresholdData, BatteryData,
    CommandOutput, DelayData, DevicesData, DpiData, EnabledData, HelpData, LodData, LodSetData,
    LowPowerEnabledData, LowPowerPollingData, PollingData, PollingSetData, PowerData,
    ScrollJumpData, SleepTimerData, error_document, invalid_argument_document, lod_name,
};

const DPI_HELP: &str = "DPI commands:\n  steelseriesctl dpi get\n  steelseriesctl dpi set <dpi1|X1xY1> [dpi2|X2xY2] ... [dpi5|X5xY5]\n  steelseriesctl dpi use <stage-id>\n\nA single value applies to both axes; use XxY for independent axis values.";
const LOD_HELP: &str = "Lift-off distance commands:\n  steelseriesctl lod get\n  steelseriesctl lod set low <stage-id>\n  steelseriesctl lod set high <stage-id>";
const POLLING_HELP: &str = "Polling commands:\n  steelseriesctl polling get\n  steelseriesctl polling set wireless <125|250|500|1000|2000|4000>\n  steelseriesctl polling set wired <125|250|500|1000>";
const BATTERY_HELP: &str = "Battery commands:\n  steelseriesctl battery get";
const POWER_HELP: &str = "Power Management commands:\n  steelseriesctl power get\n  steelseriesctl power low-power set <on|off>\n  steelseriesctl power low-power polling <125|250|500>\n  steelseriesctl power auto-low-power set <on|off>\n  steelseriesctl power auto-low-power threshold <5..25>\n  steelseriesctl power sleep set <minutes>";
const WIRELESS_STABILITY_HELP: &str = "Wireless Stability Enhancement commands:\n  steelseriesctl wireless-stability get\n  steelseriesctl wireless-stability set <on|off>";
const BLUETOOTH_SMOOTHING_HELP: &str = "Bluetooth Smoothing commands:\n  steelseriesctl bluetooth-smoothing get\n  steelseriesctl bluetooth-smoothing set <on|off>";
const SCROLL_JUMP_HELP: &str = "Scroll Jump Protection commands:\n  steelseriesctl scroll-jump get\n  steelseriesctl scroll-jump set <on|off>\n  steelseriesctl scroll-jump delay <milliseconds>";
const ROOT_HELP: &str = "SteelSeries Linux CLI\n\nCommands:\n  devices\n  dpi\n  lod\n  polling\n  battery\n  power\n  wireless-stability\n  bluetooth-smoothing\n  scroll-jump\n\nOptions:\n  -d, --device <ID>  Select a physical device by device ID\n      --json         Emit machine-readable JSON";

#[derive(Debug, Parser)]
#[command(name = "steelseriesctl", about = "SteelSeries Linux CLI", version)]
struct Cli {
    /// Select a physical device by device ID.
    #[arg(short = 'd', long, global = true, value_name = "ID")]
    device: Option<String>,
    /// Emit a stable machine-readable JSON document.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Detect supported SteelSeries devices.
    Devices,
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
    /// Show wireless and wired polling rates.
    Get,
    /// Change one connection mode while preserving the other.
    Set { mode: PollingMode, rate: u16 },
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
    Polling { rate: u16 },
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PollingMode {
    Wireless,
    Wired,
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
        Some(Command::Devices) => run_devices(),
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
        Some(Command::Devices) => "devices",
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

fn run_devices() -> Result<CommandOutput, Error> {
    let devices = Aerox3WirelessGen2::discover()?;
    let mut human = "SteelSeries devices:".to_owned();
    if devices.is_empty() {
        human.push_str("\n  No usable Aerox 3 Wireless Gen 2 device found.");
    }
    for (index, device) in devices.iter().enumerate() {
        if index > 0 {
            human.push('\n');
        }
        human.push_str(&format!(
            "\n  {}\n    ID: {}\n    Connection: {}",
            DeviceModel::Aerox3WirelessGen2,
            device.identity,
            device.active_connection
        ));
    }
    Ok(CommandOutput::new(
        "devices",
        human,
        DevicesData::from_devices(&devices),
    ))
}

fn run_dpi(command: DpiCommand, requested_identity: Option<&str>) -> Result<CommandOutput, Error> {
    match command {
        DpiCommand::Get => {
            let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
            let config = device.get_dpi_config()?;
            Ok(CommandOutput::new(
                "dpi.get",
                format_dpi_config(&config),
                DpiData::from_config(&config),
            ))
        }
        DpiCommand::Set { dpis } => {
            let values = dpis.iter().map(|dpi| (dpi.x, dpi.y)).collect::<Vec<_>>();
            validate_dpi_values(&values)?;
            let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
            let current = device.get_dpi_config()?;
            let updated = config_from_dpi_values(&current, &values)?;
            device.set_dpi_config(&updated)?;
            device.commit()?;
            Ok(CommandOutput::new(
                "dpi.set",
                dpi_stages_updated_message(&dpis),
                DpiData::from_config(&updated),
            ))
        }
        DpiCommand::Use { stage_id } => {
            let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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
    let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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
            let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
            let config = device.get_polling_config()?;
            Ok(CommandOutput::new(
                "polling.get",
                format!(
                    "Polling Rate:\n  Wireless: {}\n  Wired:    {}",
                    config.wireless, config.wired
                ),
                PollingData {
                    wireless_hz: config.wireless.hz(),
                    wired_hz: config.wired.hz(),
                },
            ))
        }
        PollingCommand::Set { mode, rate } => {
            let rate = PollingRate::try_from(rate)?;
            if matches!(mode, PollingMode::Wired) {
                validate_wired_polling_rate(rate)?;
            }
            let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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

fn run_battery(
    command: BatteryCommand,
    requested_identity: Option<&str>,
) -> Result<CommandOutput, Error> {
    match command {
        BatteryCommand::Get => {
            let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
            let status = device.get_battery_status()?;
            let human = match status {
                BatteryStatus::Available { percent, charging } => format!(
                    "Battery: {percent}%\nCharging: {}",
                    if charging { "Yes" } else { "No" }
                ),
                BatteryStatus::Unavailable => "Battery: unavailable".to_owned(),
            };
            Ok(CommandOutput::new(
                "battery.get",
                human,
                BatteryData::from(status),
            ))
        }
    }
}

fn run_power(
    command: PowerCommand,
    requested_identity: Option<&str>,
) -> Result<CommandOutput, Error> {
    match command {
        PowerCommand::Get => {
            let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
            let config = device.get_power_config()?;
            Ok(CommandOutput::new(
                "power.get",
                format_power_config(&config),
                PowerData::from(&config),
            ))
        }
        PowerCommand::LowPower { command } => match command {
            LowPowerCommand::Set { state } => {
                let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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
                let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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
                let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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
                let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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
                let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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
    let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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
    let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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
            let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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
            let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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
            let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
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
        AutoLowPowerCommand, BatteryCommand, Cli, Command, DpiCommand, DpiInput,
        FeatureToggleCommand, LodCommand, LodValue, LowPowerCommand, PollingCommand, PollingMode,
        PowerCommand, PowerState, ScrollJumpCommand, SleepCommand, apply_scroll_jump_change,
        apply_wireless_feature_change, command_identifier, dpi_stages_updated_message,
        format_bluetooth_smoothing, format_dpi_config, format_lod_config, format_scroll_jump,
        format_wireless_stability,
    };
    use steelseries_core::{DpiConfig, DpiStage, LiftOffDistance};

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
    }

    #[test]
    fn uses_stable_json_command_identifiers() {
        for (arguments, expected) in [
            (&["devices"][..], "devices"),
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
                    mode: PollingMode::Wireless,
                    rate: 1000,
                })
            })
        ));
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
