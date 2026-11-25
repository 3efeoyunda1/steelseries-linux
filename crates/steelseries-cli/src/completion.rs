use std::{env, ffi::OsString, fs, io, path::Path, process::ExitCode};

use clap::{Command, CommandFactory, ValueEnum};
use clap_complete::{
    CompleteEnv, CompletionCandidate,
    env::{Bash, EnvCompleter, Fish, Shells, Zsh},
};
use steelseries_core::{
    Device,
    validation::{LOW_POWER_POLLING_RATES, POLLING_RATES, WIRED_POLLING_RATES},
};

use crate::{Cli, PollingInput, PollingMode};

const COMPLETE_ENV: &str = "STEELSERIESCTL_COMPLETE";
const GENERATE_ENV: &str = "STEELSERIESCTL_GENERATE_COMPLETIONS";
const PROGRAM: &str = "steelseriesctl";

pub fn handle() -> Option<ExitCode> {
    if let Some(directory) = env::var_os(GENERATE_ENV) {
        return Some(match generate(Path::new(&directory)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("Error generating shell completions: {error}");
                ExitCode::FAILURE
            }
        });
    }
    if env::var_os(COMPLETE_ENV).is_some() {
        CompleteEnv::with_factory(completion_command)
            .var(COMPLETE_ENV)
            .completer(PROGRAM)
            .shells(Shells(&[&Bash, &Zsh, &Fish]))
            .complete();
    }
    None
}

fn generate(directory: &Path) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    for (shell, filename) in [
        (&Bash as &dyn EnvCompleter, PROGRAM),
        (&Zsh, "_steelseriesctl"),
        (&Fish, "steelseriesctl.fish"),
    ] {
        let mut script = Vec::new();
        shell.write_registration(COMPLETE_ENV, PROGRAM, PROGRAM, PROGRAM, &mut script)?;
        if filename == "_steelseriesctl" {
            // compinit autoloads this file as the function for the first Tab press.
            // Execute the generated function immediately in that context.
            let registration = b"compdef _clap_dynamic_completer_steelseriesctl steelseriesctl\n";
            if !script.ends_with(registration) {
                return Err(io::Error::other(
                    "unexpected clap_complete Zsh registration",
                ));
            }
            script.truncate(script.len() - registration.len());
            script.extend_from_slice(b"_clap_dynamic_completer_steelseriesctl \"$@\"\n");
        }
        fs::write(directory.join(filename), script)?;
    }
    Ok(())
}

fn completion_command() -> Command {
    let args: Vec<_> = env::args_os()
        .skip_while(|arg| arg != "--")
        .skip(1)
        .collect();
    // The adapters pass the current word as an index (Bash/Zsh) or
    // as the last word (Fish). Only parse the words before that word.
    let index = env::var("_CLAP_COMPLETE_INDEX")
        .ok()
        .and_then(|index| index.parse::<usize>().ok())
        .unwrap_or_else(|| args.len().saturating_sub(1));
    let before_cursor = &args[..index.min(args.len())];
    let mode = polling_mode(before_cursor);

    Cli::command().mut_subcommand("polling", |polling| {
        polling.mut_subcommand("set", |set| {
            set.mut_arg("rate", |rate| {
                rate.add(clap_complete::engine::ArgValueCandidates::new(move || {
                    let rates = match mode {
                        Some(PollingMode::Wired) => WIRED_POLLING_RATES,
                        Some(PollingMode::Wireless) => POLLING_RATES,
                        None => &[],
                    };
                    rates
                        .iter()
                        .map(|rate| CompletionCandidate::new(rate.hz().to_string()))
                        .collect()
                }))
            })
        })
    })
}

fn polling_mode(args: &[OsString]) -> Option<PollingMode> {
    let matches = Cli::command()
        .ignore_errors(true)
        .try_get_matches_from(args)
        .ok()?;
    let set = matches
        .subcommand_matches("polling")?
        .subcommand_matches("set")?;
    match set.get_one::<PollingInput>("mode_or_rate")? {
        PollingInput::Mode(mode) => Some(*mode),
        PollingInput::Rate(_) => None,
    }
}

pub fn devices() -> Vec<CompletionCandidate> {
    Device::discover()
        .unwrap_or_default()
        .into_iter()
        .map(|device| {
            CompletionCandidate::new(device.identity.to_string()).help(Some(
                format!("{} ({})", device.model.name(), device.active_connection).into(),
            ))
        })
        .collect()
}

pub fn polling_inputs() -> Vec<CompletionCandidate> {
    PollingMode::value_variants()
        .iter()
        .filter_map(ValueEnum::to_possible_value)
        .map(|mode| CompletionCandidate::new(mode.get_name()))
        .chain(
            POLLING_RATES
                .iter()
                .map(|rate| CompletionCandidate::new(rate.hz().to_string())),
        )
        .collect()
}

pub fn low_power_rates() -> Vec<CompletionCandidate> {
    LOW_POWER_POLLING_RATES
        .iter()
        .map(|rate| CompletionCandidate::new(rate.hz().to_string()))
        .collect()
}
