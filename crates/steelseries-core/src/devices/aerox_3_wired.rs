use std::cell::Cell;

use crate::{
    BatteryCapabilities, DeviceCapabilities, DeviceDriver, DeviceModel, DpiCapabilities, DpiConfig,
    DpiStage, Error, FeatureCapabilities, LiftOffDistance, LiftOffDistanceCapabilities,
    PollingCapabilities, PollingRate, PollingState, PowerCapabilities, RangeCapabilities,
    ScrollJumpCapabilities, SleepTimerCapabilities, WirelessFeatureCapabilities,
    usb::HidSession,
    validation::{
        MAX_DPI_STAGES, MIN_DPI_STAGES, WIRED_POLLING_RATES, stage_id_to_index, validate_dpi_value,
    },
};

pub(crate) const VENDOR_ID: u16 = 0x1038;
pub(crate) const PRODUCT_ID: u16 = 0x1836;
pub(crate) const CONFIG_INTERFACE: i32 = 3;

const MODEL: DeviceModel = DeviceModel::Aerox3Wired;
const SET_DPI: u8 = 0x2d;
const GET_DPI: u8 = 0xad;
const SET_POLLING: u8 = 0x2b;
const GET_POLLING: u8 = 0xab;
const COMMIT: u8 = 0x11;
const REPORT_SIZE: usize = 64;
const WRITE_SIZE: usize = REPORT_SIZE + 1;
const RESPONSE_TIMEOUT_MS: i32 = 1_500;

const MIN_DPI: u16 = 200;
const MAX_DPI: u16 = 8_500;
const DPI_STEP: u16 = 100;

// TrueMove Core mapping from Rivalcfg (WTFPL v2).
// https://github.com/flozz/rivalcfg/blob/master/rivalcfg/devices/dpi/truemove_core.py
const DPI_CODES: [u8; ((MAX_DPI - MIN_DPI) / DPI_STEP + 1) as usize] = [
    0x04, 0x06, 0x08, 0x0b, 0x0d, 0x0f, 0x12, 0x14, 0x16, 0x19, 0x1b, 0x1d, 0x20, 0x22, 0x24, 0x27,
    0x29, 0x2b, 0x2e, 0x30, 0x32, 0x34, 0x37, 0x39, 0x3b, 0x3e, 0x40, 0x42, 0x45, 0x47, 0x49, 0x4c,
    0x4e, 0x50, 0x53, 0x55, 0x57, 0x5a, 0x5c, 0x5e, 0x61, 0x63, 0x65, 0x68, 0x6a, 0x6c, 0x6f, 0x71,
    0x73, 0x76, 0x78, 0x7a, 0x7d, 0x7f, 0x81, 0x84, 0x86, 0x88, 0x8b, 0x8d, 0x8f, 0x92, 0x94, 0x96,
    0x99, 0x9b, 0x9d, 0xa0, 0xa2, 0xa4, 0xa7, 0xa9, 0xab, 0xad, 0xb0, 0xb2, 0xb4, 0xb7, 0xb9, 0xbc,
    0xbe, 0xc0, 0xc3, 0xc5,
];

pub(crate) struct Aerox3Wired {
    session: HidSession,
    awaiting_response: Cell<bool>,
}

impl Aerox3Wired {
    pub(crate) const CAPABILITIES: DeviceCapabilities = DeviceCapabilities {
        dpi: DpiCapabilities {
            supported: true,
            min: MIN_DPI,
            max: MAX_DPI,
            step: DPI_STEP,
            min_stages: MIN_DPI_STAGES,
            max_stages: MAX_DPI_STAGES,
            active_stage_selection: true,
            independent_axes: false,
        },
        lod: LiftOffDistanceCapabilities {
            supported: false,
            per_stage: false,
            values: &[],
        },
        polling: PollingCapabilities {
            supported: true,
            wireless: &[],
            wired: WIRED_POLLING_RATES,
        },
        battery: BatteryCapabilities {
            supported: false,
            charging_state: false,
        },
        power: PowerCapabilities {
            supported: false,
            low_power_mode: false,
            low_power_polling: &[],
            sleep_timer: SleepTimerCapabilities {
                supported: false,
                min: 0,
                max: 0,
                step: 0,
                recommended_range: None,
            },
            auto_low_power_supported: false,
            auto_low_power_threshold: RangeCapabilities {
                min: 0,
                max: 0,
                step: 0,
            },
        },
        wireless_features: WirelessFeatureCapabilities {
            wireless_stability: FeatureCapabilities { supported: false },
            bluetooth_smoothing: FeatureCapabilities { supported: false },
        },
        scroll_jump: ScrollJumpCapabilities {
            supported: false,
            delay: RangeCapabilities {
                min: 0,
                max: 0,
                step: 0,
            },
        },
        commit: true,
    };

    pub(crate) fn new(session: HidSession) -> Self {
        Self {
            session,
            awaiting_response: Cell::new(false),
        }
    }

    fn send(&self, report: &[u8; WRITE_SIZE]) -> Result<[u8; REPORT_SIZE], Error> {
        // A late response after a failed exchange could be mistaken for a
        // later request with the same opcode. Reopen the session before retrying.
        if self.awaiting_response.replace(true) {
            return Err(Error::IncompleteHidExchange);
        }
        let response = exchange(
            report,
            |report| self.session.device.write(report),
            |response| {
                self.session
                    .device
                    .read_timeout(response, RESPONSE_TIMEOUT_MS)
            },
        )?;
        // A matching response is the synchronization point between commands.
        self.awaiting_response.set(false);
        Ok(response)
    }

    fn send_and_ack(&self, report: &[u8; WRITE_SIZE]) -> Result<(), Error> {
        parse_ack(&self.send(report)?, report[1])
    }
}

impl DeviceDriver for Aerox3Wired {
    fn model(&self) -> DeviceModel {
        MODEL
    }

    fn get_dpi_config(&self) -> Result<DpiConfig, Error> {
        parse_dpi(&self.send(&command(GET_DPI))?)
    }

    fn set_dpi_config(&self, config: &DpiConfig) -> Result<(), Error> {
        self.send_and_ack(&encode_dpi_config(config)?)
    }

    fn replace_dpi_stages(&self, values: &[(u16, u16)]) -> Result<DpiConfig, Error> {
        crate::validation::validate_dpi_values(MODEL, values)?;
        let current = self.get_dpi_config()?;
        let updated = replace_stages(&current, values);
        self.set_dpi_config(&updated)?;
        Ok(updated)
    }

    fn set_active_dpi_stage(&self, stage_id: usize) -> Result<(), Error> {
        let index = stage_id_to_index(stage_id)?;
        let mut config = self.get_dpi_config()?;
        if index >= config.stages.len() {
            return Err(Error::StageNotConfigured {
                stage_id,
                stage_count: config.stages.len(),
            });
        }
        config.active = index;
        self.set_dpi_config(&config)
    }

    fn get_polling_config(&self) -> Result<PollingState, Error> {
        Ok(PollingState {
            wired: parse_polling(&self.send(&command(GET_POLLING))?)?,
            wireless: None,
        })
    }

    fn set_wired_polling(&self, rate: PollingRate) -> Result<(), Error> {
        self.send_and_ack(&encode_polling(rate)?)
    }

    fn commit(&self) -> Result<(), Error> {
        self.send_and_ack(&command(COMMIT))
    }
}

fn command(opcode: u8) -> [u8; WRITE_SIZE] {
    let mut report = [0; WRITE_SIZE];
    report[1] = opcode;
    report
}

fn exchange(
    report: &[u8; WRITE_SIZE],
    write: impl FnOnce(&[u8]) -> hidapi::HidResult<usize>,
    read: impl FnOnce(&mut [u8]) -> hidapi::HidResult<usize>,
) -> Result<[u8; REPORT_SIZE], Error> {
    let command = report[1];
    let written = write(report).map_err(|source| Error::Write { command, source })?;
    if written != WRITE_SIZE {
        return Err(Error::ShortWrite {
            command,
            expected: WRITE_SIZE,
            actual: written,
        });
    }
    // Reserve a byte to detect reports longer than the protocol requires.
    let mut response = [0; REPORT_SIZE + 1];
    let received = read(&mut response).map_err(|source| Error::Read { command, source })?;
    if received == 0 {
        return Err(Error::ReadTimeout {
            expected: command,
            observed: String::new(),
        });
    }
    if received != REPORT_SIZE {
        return Err(Error::InvalidReportLength {
            command,
            expected: REPORT_SIZE,
            actual: received,
        });
    }
    if response[0] != command {
        return Err(Error::UnexpectedCommand {
            expected: command,
            actual: response[0],
        });
    }
    Ok(response[..REPORT_SIZE]
        .try_into()
        .expect("64-byte response"))
}

fn parse_ack(response: &[u8; REPORT_SIZE], command: u8) -> Result<(), Error> {
    let [status, detail] = [response[1], response[2]];
    if [status, detail] != [0, 0] {
        return Err(Error::InvalidAcknowledgement {
            command,
            status,
            detail,
        });
    }
    Ok(())
}

fn encode_dpi(value: u16, stage: usize) -> Result<u8, Error> {
    validate_dpi_value(Aerox3Wired::CAPABILITIES.dpi, "X", value, stage)?;
    Ok(DPI_CODES[usize::from((value - MIN_DPI) / DPI_STEP)])
}

fn decode_dpi(code: u8) -> Result<u16, Error> {
    let index = DPI_CODES
        .binary_search(&code)
        .map_err(|_| Error::UnknownDpiCode(code))?;
    Ok(MIN_DPI + index as u16 * DPI_STEP)
}

fn encode_dpi_config(config: &DpiConfig) -> Result<[u8; WRITE_SIZE], Error> {
    let count = config.stages.len();
    if !(MIN_DPI_STAGES..=MAX_DPI_STAGES).contains(&count) {
        return Err(Error::InvalidDpiStageCount(count));
    }
    if config.active >= count {
        return Err(Error::InvalidDpiActiveIndex {
            active: config.active,
            stage_count: count,
        });
    }
    let mut report = command(SET_DPI);
    report[2] = count as u8;
    report[3] = config.active as u8;
    for (index, stage) in config.stages.iter().enumerate() {
        if stage.x != stage.y {
            return Err(Error::UnsupportedFeature {
                model: MODEL,
                feature: "independent X/Y DPI",
            });
        }
        if stage.lod != LiftOffDistance::Low {
            return Err(Error::UnsupportedFeature {
                model: MODEL,
                feature: "lift-off distance",
            });
        }
        report[4 + index] = encode_dpi(stage.x, index + 1)?;
    }
    Ok(report)
}

fn parse_dpi(response: &[u8; REPORT_SIZE]) -> Result<DpiConfig, Error> {
    let count = usize::from(response[1]);
    let active = usize::from(response[2]);
    if !(MIN_DPI_STAGES..=MAX_DPI_STAGES).contains(&count) || active >= count {
        return Err(Error::MalformedDpiState {
            stage_count: count,
            active,
        });
    }
    let stages = response[3..3 + count]
        .iter()
        .map(|&code| decode_dpi(code).map(DpiStage::scalar))
        .collect::<Result<_, _>>()?;
    Ok(DpiConfig { stages, active })
}

fn replace_stages(current: &DpiConfig, values: &[(u16, u16)]) -> DpiConfig {
    let current_dpi = current.stages[current.active].x;
    let single_edit = values.len() == current.stages.len()
        && values
            .iter()
            .zip(&current.stages)
            .filter(|(value, stage)| value.0 != stage.x)
            .take(2)
            .count()
            <= 1;
    let active = if single_edit
        || values
            .get(current.active)
            .is_some_and(|value| value.0 == current_dpi)
    {
        current.active
    } else {
        values
            .iter()
            .position(|value| value.0 == current_dpi)
            .unwrap_or_else(|| current.active.min(values.len() - 1))
    };
    let stages = values
        .iter()
        .map(|&(dpi, _)| DpiStage::scalar(dpi))
        .collect();
    DpiConfig { stages, active }
}

fn parse_polling(response: &[u8; REPORT_SIZE]) -> Result<PollingRate, Error> {
    match response[1] {
        0x01 => Ok(PollingRate::Hz1000),
        0x02 => Ok(PollingRate::Hz500),
        0x03 => Ok(PollingRate::Hz250),
        0x04 => Ok(PollingRate::Hz125),
        code => Err(Error::UnknownPollingCode(code)),
    }
}

fn encode_polling(rate: PollingRate) -> Result<[u8; WRITE_SIZE], Error> {
    let mut report = command(SET_POLLING);
    report[2] = match rate {
        PollingRate::Hz1000 => 0x01,
        PollingRate::Hz500 => 0x02,
        PollingRate::Hz250 => 0x03,
        PollingRate::Hz125 => 0x04,
        _ => return Err(Error::WiredPollingTooHigh { value: rate.hz() }),
    };
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(opcode: u8, payload: &[u8]) -> [u8; REPORT_SIZE] {
        let mut report = [0; REPORT_SIZE];
        report[0] = opcode;
        report[1..1 + payload.len()].copy_from_slice(payload);
        report
    }

    #[test]
    fn all_84_dpi_codes_round_trip_uniquely() {
        assert_eq!(DPI_CODES.len(), 84);
        for dpi in (200..=8500).step_by(100) {
            let code = encode_dpi(dpi, 1).unwrap();
            assert_eq!(decode_dpi(code).unwrap(), dpi);
        }
        for code in u8::MIN..=u8::MAX {
            if let Ok(dpi) = decode_dpi(code) {
                assert_eq!(encode_dpi(dpi, 1).unwrap(), code);
            } else {
                assert!(!DPI_CODES.contains(&code));
            }
        }
    }

    #[test]
    fn known_dpi_codes_and_boundaries_match_the_protocol() {
        for (dpi, code) in [
            (200, 0x04),
            (400, 0x08),
            (600, 0x0d),
            (700, 0x0f),
            (800, 0x12),
            (900, 0x14),
            (1200, 0x1b),
            (1600, 0x24),
            (2300, 0x34),
            (2400, 0x37),
            (3200, 0x49),
            (8100, 0xbc),
            (8500, 0xc5),
        ] {
            assert_eq!(encode_dpi(dpi, 1).unwrap(), code);
        }
        for dpi in [0, 100, 199, 201, 250, 8499, 8501, 8600] {
            assert!(matches!(
                encode_dpi(dpi, 2),
                Err(Error::InvalidDpiValue { value, stage: Some(2), min: 200, max: 8500, step: 100, .. }) if value == dpi
            ));
        }
        assert!(matches!(decode_dpi(0x00), Err(Error::UnknownDpiCode(0x00))));
    }

    #[test]
    fn parses_dpi_and_encodes_zero_based_active_stage_in_full_write() {
        let mut config = parse_dpi(&response(0xad, &[3, 1, 0x12, 0x24, 0x49])).unwrap();
        assert_eq!(config.active, 1);
        assert_eq!(config.stages, [800, 1600, 3200].map(DpiStage::scalar));
        config.active = 0;
        let report = encode_dpi_config(&config).unwrap();
        assert_eq!(&report[..7], &[0, 0x2d, 3, 0, 0x12, 0x24, 0x49]);
        assert_eq!(report.len(), 65);
        assert!(report[7..].iter().all(|&byte| byte == 0));
        assert_eq!(&command(GET_DPI)[..2], &[0, 0xad]);
        assert!(command(GET_DPI)[2..].iter().all(|&byte| byte == 0));
    }

    #[test]
    fn rejects_invalid_dpi_counts_indexes_axes_and_sensor_codes() {
        for count in [0, 6, 255] {
            assert!(matches!(parse_dpi(&response(GET_DPI, &[count, 0])),
                Err(Error::MalformedDpiState { stage_count, .. }) if stage_count == usize::from(count)));
        }
        assert!(matches!(
            parse_dpi(&response(GET_DPI, &[1, 1, 0x12])),
            Err(Error::MalformedDpiState {
                stage_count: 1,
                active: 1
            })
        ));
        assert!(matches!(
            parse_dpi(&response(GET_DPI, &[1, 0, 0x13])),
            Err(Error::UnknownDpiCode(0x13))
        ));
        for count in [0, 6] {
            let config = DpiConfig {
                stages: vec![DpiStage::scalar(800); count],
                active: 0,
            };
            assert!(
                matches!(encode_dpi_config(&config), Err(Error::InvalidDpiStageCount(size)) if size == count)
            );
        }
        let mut config = DpiConfig {
            stages: vec![DpiStage::scalar(800)],
            active: 1,
        };
        assert!(matches!(
            encode_dpi_config(&config),
            Err(Error::InvalidDpiActiveIndex {
                stage_count: 1,
                active: 1
            })
        ));
        config.active = 0;
        config.stages[0].y = 900;
        assert!(matches!(
            encode_dpi_config(&config),
            Err(Error::UnsupportedFeature {
                feature: "independent X/Y DPI",
                ..
            })
        ));
        config.stages[0].y = 800;
        config.stages[0].lod = LiftOffDistance::High;
        assert!(matches!(
            encode_dpi_config(&config),
            Err(Error::UnsupportedFeature {
                feature: "lift-off distance",
                ..
            })
        ));
    }

    #[test]
    fn preset_edits_preserve_the_active_stage_when_possible() {
        let current = DpiConfig {
            stages: vec![
                DpiStage::scalar(800),
                DpiStage::scalar(800),
                DpiStage::scalar(3200),
            ],
            active: 1,
        };
        let edited = replace_stages(&current, &[(800, 800), (1600, 1600), (3200, 3200)]);
        assert_eq!(edited.active, 1);
        assert_eq!(edited.stages, [800, 1600, 3200].map(DpiStage::scalar));
        let added = replace_stages(
            &edited,
            &[(400, 400), (800, 800), (1600, 1600), (3200, 3200)],
        );
        assert_eq!(added.active, 2);
        let removed = replace_stages(&added, &[(400, 400), (3200, 3200)]);
        assert_eq!(removed.active, 1);
        assert_eq!(removed.stages, [400, 3200].map(DpiStage::scalar));
    }

    #[test]
    fn encodes_and_decodes_polling_independently_of_dpi() {
        assert_eq!(&command(GET_POLLING)[..2], &[0, 0xab]);
        for (rate, code) in [
            (PollingRate::Hz125, 4),
            (PollingRate::Hz250, 3),
            (PollingRate::Hz500, 2),
            (PollingRate::Hz1000, 1),
        ] {
            let report = encode_polling(rate).unwrap();
            assert_eq!(&report[..3], &[0, 0x2b, code]);
            assert!(report[3..].iter().all(|&byte| byte == 0));
            assert_eq!(parse_polling(&response(0xab, &[code])).unwrap(), rate);
        }
        assert!(matches!(
            encode_polling(PollingRate::Hz2000),
            Err(Error::WiredPollingTooHigh { value: 2000 })
        ));
        for code in [0, 5, 255] {
            assert!(
                matches!(parse_polling(&response(GET_POLLING, &[code])), Err(Error::UnknownPollingCode(value)) if value == code)
            );
        }
    }

    #[test]
    fn sends_commit_with_zero_padding_and_checks_acknowledgements() {
        let report = command(COMMIT);
        assert_eq!(&report[..2], &[0, 0x11]);
        assert!(report[2..].iter().all(|&byte| byte == 0));
        for opcode in [SET_DPI, SET_POLLING, COMMIT] {
            parse_ack(&response(opcode, &[0, 0]), opcode).unwrap();
            for (status, detail) in [(1, 0), (0, 1)] {
                assert!(
                    matches!(parse_ack(&response(opcode, &[status, detail]), opcode),
                    Err(Error::InvalidAcknowledgement { command, status: s, detail: d })
                        if (command, s, d) == (opcode, status, detail))
                );
            }
        }
    }

    #[test]
    fn rejects_short_writes_timeouts_and_invalid_replies() {
        let report = command(GET_DPI);
        let valid_response = response(GET_DPI, &[1, 0, 0x12]);
        let response = exchange(
            &report,
            |sent| {
                assert_eq!(sent, report);
                Ok(sent.len())
            },
            |buffer| {
                buffer[..REPORT_SIZE].copy_from_slice(&valid_response);
                Ok(REPORT_SIZE)
            },
        )
        .unwrap();
        assert_eq!(response, valid_response);
        assert!(matches!(
            exchange(&report, |_| Ok(64), |_| panic!("read after short write")),
            Err(Error::ShortWrite {
                actual: 64,
                expected: 65,
                ..
            })
        ));
        assert!(matches!(
            exchange(&report, |_| Ok(65), |_| Ok(0)),
            Err(Error::ReadTimeout {
                expected: GET_DPI,
                ..
            })
        ));
        for length in [2, 63, 65] {
            assert!(matches!(exchange(&report, |_| Ok(65), |_| Ok(length)),
                Err(Error::InvalidReportLength { expected: 64, actual, .. }) if actual == length));
        }
        assert!(matches!(
            exchange(
                &report,
                |_| Ok(65),
                |buffer| {
                    buffer[0] = COMMIT;
                    Ok(REPORT_SIZE)
                }
            ),
            Err(Error::UnexpectedCommand {
                expected: GET_DPI,
                actual: COMMIT
            })
        ));
    }
}
