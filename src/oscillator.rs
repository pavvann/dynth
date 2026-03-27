use std::f32::consts::PI;

pub const TABLE_SIZE: usize = 2048;

/// How much the jog velocity deflects the carrier frequency (FM self-modulation index).
/// 0.5 = at jog_velocity 1.0, max deviation is ±0.5 * carrier frequency.
const FM_SCALE: f32 = 0.5;

/// How fast the jog phase accumulates per CC tick (tunes scrub speed).
pub const JOG_SCALE: f32 = 0.003;

pub struct WavetableOscillator {
    /// Four pre-computed single-cycle tables: [sine, saw, square, triangle]
    tables: Box<[[f32; TABLE_SIZE]; 4]>,
    phase: f32,       // carrier read position 0..1
    sample_rate: f32,
    last_sample: f32, // for FM feedback
}

impl WavetableOscillator {
    pub fn new(sample_rate: f32) -> Self {
        let mut tables = Box::new([[0.0f32; TABLE_SIZE]; 4]);

        for i in 0..TABLE_SIZE {
            let t = i as f32 / TABLE_SIZE as f32; // 0..1

            // Sine
            tables[0][i] = (t * 2.0 * PI).sin();

            // Saw: 1 → -1
            tables[1][i] = 1.0 - 2.0 * t;

            // Square
            tables[2][i] = if t < 0.5 { 1.0 } else { -1.0 };

            // Triangle
            tables[3][i] = if t < 0.5 {
                4.0 * t - 1.0
            } else {
                3.0 - 4.0 * t
            };
        }

        Self { tables, phase: 0.0, sample_rate, last_sample: 0.0 }
    }

    /// Read one sample from a table at fractional position `pos` (0..1) with linear interpolation.
    fn read(&self, table: usize, pos: f32) -> f32 {
        let idx_f = pos * TABLE_SIZE as f32;
        let idx0  = idx_f as usize % TABLE_SIZE;
        let idx1  = (idx0 + 1) % TABLE_SIZE;
        let frac  = idx_f.fract();
        self.tables[table][idx0] * (1.0 - frac) + self.tables[table][idx1] * frac
    }

    /// Advance and return the next sample.
    ///
    /// - `waveform`:     0=sine 1=saw 2=square 3=triangle
    /// - `phase_offset`: jog-wheel accumulated phase shift 0..1
    /// - `fm_depth`:     jog velocity -1..1, drives FM self-modulation
    pub fn next_sample(&mut self, freq: f32, waveform: u8, phase_offset: f32, fm_depth: f32) -> f32 {
        let table = (waveform as usize).min(3);

        // FM self-modulation: deviate carrier frequency by fm_depth * last output
        let fm_hz = freq * fm_depth.abs() * FM_SCALE * self.last_sample;
        let effective_freq = freq + fm_hz;

        // Read wavetable at current phase + jog offset
        let read_pos = (self.phase + phase_offset).fract();
        let sample = self.read(table, read_pos);

        // Advance carrier phase
        self.phase = (self.phase + effective_freq / self.sample_rate).fract();
        self.last_sample = sample;

        sample
    }

    pub fn reset_phase(&mut self) {
        self.phase = 0.0;
        self.last_sample = 0.0;
    }
}
