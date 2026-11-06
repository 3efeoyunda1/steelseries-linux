#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpiCurveKind {
    DynamicLogarithmic,
    Anchored,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DpiReference {
    pub value: u16,
    pub position: f64,
    pub label_position: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DpiCurveError {
    InvalidRange,
    InvalidStep,
}

#[derive(Debug, Clone, Copy)]
pub struct DpiCurve {
    min: u16,
    max: u16,
    step: u16,
    kind: DpiCurveKind,
    dynamic_gamma: f64,
}

impl DpiCurve {
    pub fn new(min: u16, max: u16, step: u16) -> Result<Self, DpiCurveError> {
        if min == 0 || min >= max {
            return Err(DpiCurveError::InvalidRange);
        }
        if step == 0 {
            return Err(DpiCurveError::InvalidStep);
        }
        let kind = curve_kind(max);
        let dynamic_gamma = dynamic_gamma(min, max);
        Ok(Self {
            min,
            max,
            step,
            kind,
            dynamic_gamma,
        })
    }

    pub fn position_to_dpi(self, position: f64) -> u16 {
        self.snap(self.position_to_raw_dpi(position))
    }

    pub fn position_to_raw_dpi(self, position: f64) -> f64 {
        let position = position.clamp(0.0, 1.0);
        if position <= 0.0 {
            return f64::from(self.min);
        }
        if position >= 1.0 {
            return f64::from(self.max);
        }
        match self.kind {
            DpiCurveKind::DynamicLogarithmic => {
                let exponent = position.powf(1.0 / self.dynamic_gamma);
                f64::from(self.min) * (f64::from(self.max) / f64::from(self.min)).powf(exponent)
            }
            DpiCurveKind::Anchored => {
                interpolate_position_to_dpi(position, &anchored_points(self.min, self.max))
            }
        }
    }

    pub fn dpi_to_position(self, dpi: u16) -> f64 {
        self.raw_dpi_to_position(f64::from(dpi))
    }

    fn raw_dpi_to_position(self, dpi: f64) -> f64 {
        let dpi = dpi.clamp(f64::from(self.min), f64::from(self.max));
        if dpi == f64::from(self.min) {
            return 0.0;
        }
        if dpi == f64::from(self.max) {
            return 1.0;
        }
        match self.kind {
            DpiCurveKind::DynamicLogarithmic => {
                let base = (dpi / f64::from(self.min)).ln()
                    / (f64::from(self.max) / f64::from(self.min)).ln();
                base.powf(self.dynamic_gamma).clamp(0.0, 1.0)
            }
            DpiCurveKind::Anchored => {
                interpolate_dpi_to_position(dpi, &anchored_points(self.min, self.max))
            }
        }
    }

    pub fn snap(self, raw: f64) -> u16 {
        let clamped = raw.clamp(f64::from(self.min), f64::from(self.max));
        let offset = clamped - f64::from(self.min);
        let steps = (offset / f64::from(self.step)).round();
        let snapped = f64::from(self.min) + steps * f64::from(self.step);
        snapped.clamp(f64::from(self.min), f64::from(self.max)) as u16
    }

    pub fn finalize_text_value(self, value: i64) -> Result<u16, DpiCurveError> {
        if value < i64::from(self.min) || value > i64::from(self.max) {
            return Err(DpiCurveError::InvalidRange);
        }
        Ok(self.snap(value as f64))
    }

    pub fn references(self) -> Vec<DpiReference> {
        let mut values = vec![self.min];
        for value in [400, 800, 1_600, 3_200] {
            if value > self.min && value < self.max {
                values.push(value);
            }
        }
        if self.max != self.min {
            values.push(self.max);
        }
        let mut references = values
            .into_iter()
            .map(|value| DpiReference {
                value,
                position: self.dpi_to_position(value),
                label_position: self.dpi_to_position(value),
            })
            .collect::<Vec<_>>();
        layout_reference_labels(&mut references, 400.0);
        references
    }
}

const LABEL_GAP: f64 = 6.0;

fn reference_label_width(value: u16) -> f64 {
    value.to_string().len() as f64 * 6.0 + 4.0
}

fn layout_reference_labels(references: &mut [DpiReference], width: f64) {
    if references.is_empty() || width <= 0.0 {
        return;
    }
    let widths = references
        .iter()
        .map(|reference| reference_label_width(reference.value))
        .collect::<Vec<_>>();
    let last = references.len() - 1;
    let mut left = references
        .iter()
        .zip(&widths)
        .map(|(reference, label_width)| {
            (reference.position * width - label_width / 2.0).clamp(0.0, width - label_width)
        })
        .collect::<Vec<_>>();
    left[0] = 0.0;
    left[last] = width - widths[last];

    for index in 1..last {
        left[index] = left[index].max(left[index - 1] + widths[index - 1] + LABEL_GAP);
    }
    for index in (1..last).rev() {
        left[index] = left[index].min(left[index + 1] - widths[index] - LABEL_GAP);
    }
    for index in 1..last {
        left[index] = left[index].max(left[index - 1] + widths[index - 1] + LABEL_GAP);
    }

    for ((reference, left), label_width) in references.iter_mut().zip(left).zip(widths) {
        reference.label_position = ((left + label_width / 2.0) / width).clamp(0.0, 1.0);
    }
}

pub const fn curve_kind(max: u16) -> DpiCurveKind {
    if max <= 8_000 {
        DpiCurveKind::DynamicLogarithmic
    } else {
        DpiCurveKind::Anchored
    }
}

fn dynamic_gamma(min: u16, max: u16) -> f64 {
    let focus = 3_200_u16.min(max / 2);
    if focus <= min || focus >= max {
        return 1.0;
    }
    let base = (f64::from(focus) / f64::from(min)).ln() / (f64::from(max) / f64::from(min)).ln();
    if !base.is_finite() || base <= 0.0 || base >= 1.0 {
        return 1.0;
    }
    let gamma = 0.6_f64.ln() / base.ln();
    if gamma.is_finite() && gamma > 0.0 {
        gamma
    } else {
        1.0
    }
}

fn anchored_points(min: u16, max: u16) -> Vec<(f64, f64)> {
    let mut points = vec![(f64::from(min), 0.0)];
    for (dpi, position) in [(400, 0.1), (800, 0.25), (1_600, 0.4), (3_200, 0.6)] {
        if dpi > min && dpi < max {
            points.push((f64::from(dpi), position));
        }
    }
    points.push((f64::from(max), 1.0));
    points
}

fn interpolate_position_to_dpi(position: f64, points: &[(f64, f64)]) -> f64 {
    for segment in points.windows(2) {
        let (start_dpi, start_position) = segment[0];
        let (end_dpi, end_position) = segment[1];
        if position <= end_position {
            let ratio = (position - start_position) / (end_position - start_position);
            return start_dpi + ratio * (end_dpi - start_dpi);
        }
    }
    points.last().map_or(0.0, |point| point.0)
}

fn interpolate_dpi_to_position(dpi: f64, points: &[(f64, f64)]) -> f64 {
    for segment in points.windows(2) {
        let (start_dpi, start_position) = segment[0];
        let (end_dpi, end_position) = segment[1];
        if dpi <= end_dpi {
            let ratio = (dpi - start_dpi) / (end_dpi - start_dpi);
            return start_position + ratio * (end_position - start_position);
        }
    }
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_uses_dynamic_through_8000_and_anchored_above_it() {
        assert_eq!(curve_kind(7_999), DpiCurveKind::DynamicLogarithmic);
        assert_eq!(curve_kind(8_000), DpiCurveKind::DynamicLogarithmic);
        assert_eq!(curve_kind(8_001), DpiCurveKind::Anchored);
        assert_eq!(curve_kind(26_000), DpiCurveKind::Anchored);
    }

    #[test]
    fn dynamic_curve_places_focus_at_sixty_percent_and_round_trips() {
        for max in [3_200, 6_400, 8_000] {
            let curve = DpiCurve::new(50, max, 50).unwrap();
            let focus = 3_200_u16.min(max / 2);
            assert!((curve.dpi_to_position(focus) - 0.6).abs() < 1e-10);
            for position in [0.0, 0.1, 0.6, 0.9, 1.0] {
                let raw = curve.position_to_raw_dpi(position);
                let restored = curve.raw_dpi_to_position(raw);
                assert!(restored.is_finite());
                assert!((restored - position).abs() < 1e-10);
            }
            assert_eq!(curve.position_to_raw_dpi(0.0), 50.0);
            assert_eq!(curve.position_to_raw_dpi(1.0), f64::from(max));
        }
    }

    #[test]
    fn anchored_curve_interpolates_in_both_directions() {
        let curve = DpiCurve::new(50, 26_000, 50).unwrap();
        assert_eq!(curve_kind(26_000), DpiCurveKind::Anchored);
        assert!((curve.dpi_to_position(400) - 0.1).abs() < 1e-10);
        assert!((curve.dpi_to_position(800) - 0.25).abs() < 1e-10);
        assert!((curve.position_to_raw_dpi(0.4) - 1_600.0).abs() < 1e-10);
        assert!((curve.position_to_raw_dpi(0.6) - 3_200.0).abs() < 1e-10);
        assert_eq!(curve.position_to_raw_dpi(0.0), 50.0);
        assert_eq!(curve.position_to_raw_dpi(1.0), 26_000.0);
    }

    #[test]
    fn snapping_never_exposes_invalid_effective_dpi() {
        let curve = DpiCurve::new(50, 26_000, 50).unwrap();
        assert_eq!(curve.snap(158.0), 150);
        assert_eq!(curve.snap(177.0), 200);
        assert_ne!(curve.snap(158.0), 158);
    }

    #[test]
    fn references_exclude_out_of_range_values_and_endpoint_duplicates() {
        let curve = DpiCurve::new(400, 1_600, 50).unwrap();
        let values = curve
            .references()
            .iter()
            .map(|item| item.value)
            .collect::<Vec<_>>();
        assert_eq!(values, [400, 800, 1_600]);
        assert_eq!(curve.references().last().unwrap().position, 1.0);
        let points = anchored_points(50, 3_200);
        assert_eq!(points.last(), Some(&(3_200.0, 1.0)));
        assert_eq!(points.iter().filter(|point| point.0 == 3_200.0).count(), 1);
    }

    #[test]
    fn reference_labels_are_clamped_and_do_not_overlap() {
        let curve = DpiCurve::new(50, 26_000, 50).unwrap();
        let references = curve.references();
        let width = 400.0;
        let intervals = references
            .iter()
            .map(|reference| {
                let label_width = reference_label_width(reference.value);
                let center = reference.label_position * width;
                (center - label_width / 2.0, center + label_width / 2.0)
            })
            .collect::<Vec<_>>();

        assert!(intervals[0].0.abs() < 1e-10);
        assert!((intervals.last().unwrap().1 - width).abs() < 1e-10);
        for pair in intervals.windows(2) {
            assert!(pair[1].0 - pair[0].1 >= LABEL_GAP - 1e-10);
        }
        assert_eq!(references[0].position, 0.0);
        assert_eq!(references.last().unwrap().position, 1.0);
        assert!((references[1].position - 0.1).abs() < 1e-10);
    }

    #[test]
    fn text_values_are_range_checked_then_snapped() {
        let curve = DpiCurve::new(50, 26_000, 50).unwrap();
        assert_eq!(curve.finalize_text_value(1_573), Ok(1_550));
        assert_eq!(curve.finalize_text_value(1_584), Ok(1_600));
        assert_eq!(
            curve.finalize_text_value(26_001),
            Err(DpiCurveError::InvalidRange)
        );
    }
}
