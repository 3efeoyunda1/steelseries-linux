use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use steelseries_core::devices::aerox_3_wireless_gen2::{
    config_from_scalar_dpis, validate_wired_polling_rate,
};
use steelseries_core::{
    Aerox3WirelessGen2, BatteryStatus, DeviceModel, DpiStage, Error, PollingRate,
};

const DPI_HELP: &str = "DPI commands:\n  steelseriesctl dpi get\n  steelseriesctl dpi set <dpi1> [dpi2] [dpi3] [dpi4] [dpi5]\n  steelseriesctl dpi use <dpi>";
const POLLING_HELP: &str = "Polling commands:\n  steelseriesctl polling get\n  steelseriesctl polling set wireless <125|250|500|1000|2000|4000>\n  steelseriesctl polling set wired <125|250|500|1000>";
const BATTERY_HELP: &str = "Battery commands:\n  steelseriesctl battery get";

#[derive(Debug, Parser)]
#[command(name = "steelseriesctl", about = "SteelSeries Linux CLI", version)]
struct Cli {
    /// Select a physical device by device ID.
    #[arg(short = 'd', long, global = true, value_name = "ID")]
    device: Option<String>,
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
}

#[derive(Debug, Subcommand)]
enum DpiCommand {
    /// Show all DPI stages and the active stage.
    Get,
    /// Replace the DPI stage list with one to five scalar values.
    Set {
        #[arg(required = true, num_args = 1..=5, value_name = "DPI")]
        dpis: Vec<u16>,
    },
    /// Select an existing scalar DPI stage.
    Use { dpi: u16 },
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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PollingMode {
    Wireless,
    Wired,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), Error> {
    let Cli { device, command } = cli;
    let requested_identity = device.as_deref();

    match command {
        None => print_root_help(),
        Some(Command::Devices) => print_devices()?,
        Some(Command::Dpi { command: None }) => println!("{DPI_HELP}"),
        Some(Command::Dpi {
            command: Some(command),
        }) => run_dpi(command, requested_identity)?,
        Some(Command::Polling { command: None }) => println!("{POLLING_HELP}"),
        Some(Command::Polling {
            command: Some(command),
        }) => run_polling(command, requested_identity)?,
        Some(Command::Battery { command: None }) => println!("{BATTERY_HELP}"),
        Some(Command::Battery {
            command: Some(command),
        }) => run_battery(command, requested_identity)?,
    }
    Ok(())
}

fn print_root_help() {
    println!(
        "SteelSeries Linux CLI\n\nCommands:\n  devices\n  dpi\n  polling\n  battery\n\nOptions:\n  -d, --device <ID>  Select a physical device by device ID"
    );
}

fn print_devices() -> Result<(), Error> {
    let devices = Aerox3WirelessGen2::discover()?;
    println!("SteelSeries devices:");
    if devices.is_empty() {
        println!("  No usable Aerox 3 Wireless Gen 2 device found.");
    }
    for (index, device) in devices.into_iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("  {}", DeviceModel::Aerox3WirelessGen2);
        println!("    ID: {}", device.identity);
        println!("    Connection: {}", device.active_connection);
    }
    Ok(())
}

fn run_dpi(command: DpiCommand, requested_identity: Option<&str>) -> Result<(), Error> {
    let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
    match command {
        DpiCommand::Get => print_dpi(&device.get_dpi_config()?),
        DpiCommand::Set { dpis } => {
            let current = device.get_dpi_config()?;
            let updated = config_from_scalar_dpis(&current, &dpis)?;
            device.set_dpi_config(&updated)?;
            device.commit()?;
            println!("{}", dpi_stages_updated_message(&dpis));
        }
        DpiCommand::Use { dpi } => {
            device.set_active_dpi(dpi)?;
            device.commit()?;
            println!("Active DPI changed to {dpi} DPI.");
        }
    }
    Ok(())
}

fn print_dpi(config: &steelseries_core::DpiConfig) {
    println!("DPI Stages:");
    for (index, stage) in config.stages.iter().enumerate() {
        let marker = if index == config.active { '>' } else { ' ' };
        println!("{marker} {}: {}", index + 1, format_dpi(*stage));
    }
    println!();
    println!("Active: {}", format_dpi(config.stages[config.active]));
}

fn format_dpi(stage: DpiStage) -> String {
    if stage.x == stage.y {
        format!("{} DPI", stage.x)
    } else {
        format!("{}x{} DPI", stage.x, stage.y)
    }
}

fn dpi_stages_updated_message(dpis: &[u16]) -> String {
    let values = dpis
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!("DPI stages updated: {values} DPI.")
}

fn run_polling(command: PollingCommand, requested_identity: Option<&str>) -> Result<(), Error> {
    match command {
        PollingCommand::Get => {
            let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
            let config = device.get_polling_config()?;
            println!("Polling Rate:");
            println!("  Wireless: {}", config.wireless);
            println!("  Wired:    {}", config.wired);
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
            match mode {
                PollingMode::Wireless => {
                    println!("Wireless polling rate changed to {rate}.");
                }
                PollingMode::Wired => {
                    println!("Wired polling rate changed to {rate}.");
                }
            }
        }
    }
    Ok(())
}

fn run_battery(command: BatteryCommand, requested_identity: Option<&str>) -> Result<(), Error> {
    match command {
        BatteryCommand::Get => {
            let device = Aerox3WirelessGen2::open_selected(requested_identity)?;
            match device.get_battery_status()? {
                BatteryStatus::Available { percent, charging } => {
                    println!("Battery: {percent}%");
                    println!("Charging: {}", if charging { "Yes" } else { "No" });
                }
                BatteryStatus::Unavailable => println!("Battery: unavailable"),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::{
        BatteryCommand, Cli, Command, DpiCommand, PollingCommand, PollingMode,
        dpi_stages_updated_message,
    };

    #[test]
    fn formats_single_dpi_stage_success_message() {
        assert_eq!(
            dpi_stages_updated_message(&[800]),
            "DPI stages updated: 800 DPI."
        );
    }

    #[test]
    fn formats_multiple_dpi_stages_success_message() {
        assert_eq!(
            dpi_stages_updated_message(&[400, 800, 1600]),
            "DPI stages updated: 400, 800, 1600 DPI."
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
}
