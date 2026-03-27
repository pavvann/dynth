/// Moog-style 4-pole ladder low-pass filter (Huovilainen model, simplified).
pub struct LadderFilter {
    s: [f32; 4],
    sample_rate: f32,
}

impl LadderFilter {
    pub fn new(sample_rate: f32) -> Self {
        Self { s: [0.0; 4], sample_rate }
    }

    pub fn process(&mut self, input: f32, cutoff: f32, resonance: f32) -> f32 {
        // Recover from NaN/inf state (can happen with extreme cutoff + resonance)
        if self.s.iter().any(|v| !v.is_finite()) {
            self.s = [0.0; 4];
        }

        let cutoff = cutoff.clamp(20.0, self.sample_rate * 0.49);
        let q = resonance.clamp(0.0, 1.0) * 3.8; // cap slightly below self-oscillation

        let f = (std::f32::consts::PI * cutoff / self.sample_rate).tan();
        let g = f / (1.0 + f);

        let fb = q * self.s[3];
        // Soft-clip the feedback to prevent runaway
        let x = (input - fb).clamp(-4.0, 4.0);

        let v0 = (x - self.s[0]) * g;
        let y0 = v0 + self.s[0];
        self.s[0] = (y0 + v0).clamp(-2.0, 2.0);

        let v1 = (y0 - self.s[1]) * g;
        let y1 = v1 + self.s[1];
        self.s[1] = (y1 + v1).clamp(-2.0, 2.0);

        let v2 = (y1 - self.s[2]) * g;
        let y2 = v2 + self.s[2];
        self.s[2] = (y2 + v2).clamp(-2.0, 2.0);

        let v3 = (y2 - self.s[3]) * g;
        let y3 = v3 + self.s[3];
        self.s[3] = (y3 + v3).clamp(-2.0, 2.0);

        y3
    }

    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.s = [0.0; 4];
    }
}
