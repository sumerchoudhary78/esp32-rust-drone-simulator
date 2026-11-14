//! Per-board two-point battery calibration. No guessed ESP32 ADC reference.
#[derive(Clone, Copy, Debug)]
pub struct Calibration {
    pub raw_low: u16,
    pub volts_low: f32,
    pub raw_high: u16,
    pub volts_high: f32,
}
impl Calibration {
    pub fn valid(self) -> bool {
        self.raw_low > 32
            && self.raw_high < 4063
            && self.raw_high > self.raw_low
            && self.raw_high - self.raw_low > 100
            && self.volts_low.is_finite()
            && self.volts_high.is_finite()
            && (8.0..=11.5).contains(&self.volts_low)
            && (11.5..=13.0).contains(&self.volts_high)
            && self.volts_high - self.volts_low >= 1.0
    }
    pub fn voltage(self, raw: u16) -> Option<f32> {
        if !self.valid() || !(32..=4062).contains(&raw) {
            return None;
        }
        let v = self.volts_low
            + (raw as f32 - self.raw_low as f32) * (self.volts_high - self.volts_low)
                / (self.raw_high - self.raw_low) as f32;
        if (7.0..=13.2).contains(&v) {
            Some(v)
        } else {
            None
        }
    }
}
pub struct Battery {
    calibration: Option<Calibration>,
    filtered: Option<f32>,
    good: u8,
}
impl Battery {
    pub fn new(calibration: Option<Calibration>) -> Self {
        Self {
            calibration,
            filtered: None,
            good: 0,
        }
    }
    pub fn update(&mut self, raw: Option<u16>) -> Option<f32> {
        let Some(v) = raw.and_then(|r| self.calibration.and_then(|c| c.voltage(r))) else {
            self.good = 0;
            self.filtered = None;
            return None;
        };
        let smooth = self.filtered.map_or(v, |old| old + (v - old) * 0.05);
        self.filtered = Some(smooth);
        self.good = self.good.saturating_add(1);
        // Falling voltage is acted on immediately; filtering never delays a cut.
        if self.good < 10 {
            None
        } else {
            Some(v.min(smooth))
        }
    }
}
