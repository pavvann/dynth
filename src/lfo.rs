use std::f32::consts::PI;

pub struct Lfo {
    phase: f32,
    sample_rate: f32,
}

impl Lfo {
    pub fn new(sample_rate: f32) -> Self {
        Self { phase: 0.0, sample_rate }
    }

    /// Returns value in -1..1. Call once per sample.
    pub fn next(&mut self, rate: f32) -> f32 {
        let sample = (self.phase * 2.0 * PI).sin();
        self.phase = (self.phase + rate / self.sample_rate).fract();
        sample
    }
}
