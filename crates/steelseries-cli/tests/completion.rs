use std::process::Command;

fn candidates(shell: &str, words: &[&str]) -> Vec<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_steelseriesctl"))
        .env("STEELSERIESCTL_COMPLETE", shell)
        .env("_CLAP_COMPLETE_INDEX", (words.len() + 1).to_string())
        .env("_CLAP_IFS", "\n")
        .args(["--", "steelseriesctl"])
        .args(words)
        .arg("")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{shell} {words:?}: {stderr}");
    assert!(stderr.is_empty(), "{shell} {words:?}: {stderr}");
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            line.split_once(if shell == "zsh" { ':' } else { '\t' })
                .map_or(line, |(value, _)| value)
                .trim_end()
                .to_owned()
        })
        .collect()
}

#[test]
fn completes_clap_commands_options_and_argument_values() {
    for shell in ["bash", "zsh", "fish"] {
        for (words, value) in [
            (&[][..], "polling"),
            (&["polling"], "set"),
            (&["wireless-stability", "set"], "off"),
            (&["lod", "set"], "low"),
            (&["power", "low-power", "polling"], "250"),
            (&["device", "list"], "--json"),
        ] {
            let found = candidates(shell, words);
            assert!(found.iter().any(|item| item == value), "{shell}: {found:?}");
        }
    }
}

#[test]
fn completes_optional_polling_mode_and_filters_explicit_rates() {
    for shell in ["bash", "zsh", "fish"] {
        let inputs = candidates(shell, &["polling", "set"]);
        for expected in ["wired", "wireless", "500"] {
            assert!(
                inputs.iter().any(|value| value == expected),
                "{shell}: {inputs:?}"
            );
        }
        let wired = candidates(shell, &["polling", "set", "wired"]);
        assert!(wired.iter().any(|value| value == "1000"));
        assert!(!wired.iter().any(|value| value == "4000"));
        let wireless = candidates(shell, &["polling", "set", "wireless"]);
        assert!(wireless.iter().any(|value| value == "4000"));
    }
}
