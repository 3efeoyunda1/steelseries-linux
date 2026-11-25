use crate::{
    AutoLowPowerThreshold, DeviceModel, DpiCapabilities, Error, LowPowerPollingRate, PollingRate,
    RangeCapabilities, RecommendedRange, SleepTimer, SleepTimerCapabilities,
};

pub const MIN_DPI_STAGES: usize = 1;
pub const MAX_DPI_STAGES: usize = 5;
pub const WIRED_POLLING_RATES: &[PollingRate] = &[
    PollingRate::Hz125,
    PollingRate::Hz250,
    PollingRate::Hz500,
    PollingRate::Hz1000,
];
pub const POLLING_RATES: &[PollingRate] = &[
    PollingRate::Hz125,
    PollingRate::Hz250,
    PollingRate::Hz500,
    PollingRate::Hz1000,
    PollingRate::Hz2000,
    PollingRate::Hz4000,
];
pub const LOW_POWER_POLLING_RATES: &[LowPowerPollingRate] = &[
    LowPowerPollingRate::Hz125,
    LowPowerPollingRate::Hz250,
    LowPowerPollingRate::Hz500,
];
pub const SCROLL_JUMP_DELAY: RangeCapabilities<u16> = RangeCapabilities {
    min: 100,
    max: 1_500,
    step: 100,
};
pub const AUTO_LOW_POWER_THRESHOLD: RangeCapabilities<u8> = RangeCapabilities {
    min: 5,
    max: 25,
    step: 1,
};
pub const SLEEP_TIMER: SleepTimerCapabilities = SleepTimerCapabilities {
    supported: true,
    min: 1,
    max: 71_582,
    step: 1,
    recommended_range: Some(RecommendedRange { min: 5, max: 20 }),
};

/// Rejects values that no registered model can use, before discovering hardware.
pub fn validate_dpi_request(values: &[(u16, u16)]) -> Result<(), Error> {
    let mut first_error = None;
    for model in crate::manager::registered_models() {
        match validate_dpi_values(model, values) {
            Ok(()) => return Ok(()),
            Err(error) => {
                first_error.get_or_insert(error);
            }
        }
    }
    Err(first_error.expect("at least one model is registered"))
}

pub fn validate_dpi_values(model: DeviceModel, values: &[(u16, u16)]) -> Result<(), Error> {
    let caps = model.capabilities().dpi;
    if !(caps.min_stages..=caps.max_stages).contains(&values.len()) {
        return Err(Error::InvalidDpiStageCount(values.len()));
    }
    for (index, &(x, y)) in values.iter().enumerate() {
        if !caps.independent_axes && x != y {
            return Err(Error::UnsupportedFeature {
                model,
                feature: "independent X/Y DPI",
            });
        }
        validate_dpi_value(caps, "X", x, index + 1)?;
        if caps.independent_axes {
            validate_dpi_value(caps, "Y", y, index + 1)?;
        }
    }
    Ok(())
}

pub(crate) fn validate_dpi_value(
    caps: DpiCapabilities,
    axis: &'static str,
    value: u16,
    stage: usize,
) -> Result<(), Error> {
    if !(caps.min..=caps.max).contains(&value) || !(value - caps.min).is_multiple_of(caps.step) {
        return Err(Error::InvalidDpiValue {
            axis,
            value,
            stage: Some(stage),
            min: caps.min,
            max: caps.max,
            step: caps.step,
        });
    }
    Ok(())
}

impl TryFrom<u16> for PollingRate {
    type Error = Error;

    fn try_from(hz: u16) -> Result<Self, Self::Error> {
        match hz {
            125 => Ok(Self::Hz125),
            250 => Ok(Self::Hz250),
            500 => Ok(Self::Hz500),
            1000 => Ok(Self::Hz1000),
            2000 => Ok(Self::Hz2000),
            4000 => Ok(Self::Hz4000),
            _ => Err(Error::UnsupportedPollingRate(hz)),
        }
    }
}

impl TryFrom<u16> for LowPowerPollingRate {
    type Error = Error;

    fn try_from(hz: u16) -> Result<Self, Self::Error> {
        match hz {
            125 => Ok(Self::Hz125),
            250 => Ok(Self::Hz250),
            500 => Ok(Self::Hz500),
            _ => Err(Error::UnsupportedLowPowerPollingRate(hz)),
        }
    }
}

pub fn validate_stage_id(stage_id: usize) -> Result<(), Error> {
    stage_id_to_index(stage_id).map(|_| ())
}

pub(crate) fn stage_id_to_index(stage_id: usize) -> Result<usize, Error> {
    if !(1..=MAX_DPI_STAGES).contains(&stage_id) {
        return Err(Error::InvalidStageId(stage_id));
    }
    Ok(stage_id - 1)
}

pub fn validate_scroll_jump_delay(delay_ms: u16) -> Result<(), Error> {
    if !(SCROLL_JUMP_DELAY.min..=SCROLL_JUMP_DELAY.max).contains(&delay_ms)
        || !delay_ms.is_multiple_of(SCROLL_JUMP_DELAY.step)
    {
        return Err(Error::InvalidScrollJumpDelay(delay_ms));
    }
    Ok(())
}

pub fn auto_low_power_threshold(percent: u8) -> Result<AutoLowPowerThreshold, Error> {
    validate_auto_low_power_threshold(percent)?;
    Ok(AutoLowPowerThreshold::new(percent))
}

pub(crate) fn validate_auto_low_power_threshold(percent: u8) -> Result<(), Error> {
    if !(AUTO_LOW_POWER_THRESHOLD.min..=AUTO_LOW_POWER_THRESHOLD.max).contains(&percent) {
        return Err(Error::InvalidAutoLowPowerThreshold(percent));
    }
    Ok(())
}

pub fn sleep_timer_from_minutes(minutes: u64) -> Result<SleepTimer, Error> {
    if minutes < SLEEP_TIMER.min {
        return Err(Error::SleepTimerZero);
    }
    if minutes > SLEEP_TIMER.max {
        return Err(Error::SleepTimerTooLarge);
    }
    let milliseconds = minutes
        .checked_mul(60_000)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(Error::SleepTimerTooLarge)?;
    Ok(SleepTimer::from_milliseconds(milliseconds))
}

/// Validates the shared USB polling rates.
pub fn validate_wired_polling_rate(rate: PollingRate) -> Result<(), Error> {
    if WIRED_POLLING_RATES.contains(&rate) {
        Ok(())
    } else {
        Err(Error::WiredPollingTooHigh { value: rate.hz() })
    }
}
