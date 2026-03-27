#[derive(Clone, Copy, PartialEq)]
enum Stage { Idle, Attack, Decay, Sustain, Release }

pub struct Adsr {
    stage: Stage,
    level: f32,
    sample_rate: f32,
}

impl Adsr {
    pub fn new(sample_rate: f32) -> Self {
        Self { stage: Stage::Idle, level: 0.0, sample_rate }
    }

    pub fn note_on(&mut self) {
        self.stage = Stage::Attack;
    }

    pub fn note_off(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    pub fn is_idle(&self) -> bool {
        self.stage == Stage::Idle
    }

    /// Returns envelope amplitude 0–1. Call once per sample.
    pub fn next(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) -> f32 {
        // Convert times to per-sample increments (avoid divide-by-zero)
        let attack_inc  = 1.0 / (attack.max(0.001)  * self.sample_rate);
        let decay_inc   = 1.0 / (decay.max(0.001)   * self.sample_rate);
        let release_inc = 1.0 / (release.max(0.001) * self.sample_rate);

        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                self.level += attack_inc;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level -= decay_inc;
                if self.level <= sustain {
                    self.level = sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {
                self.level = sustain;
            }
            Stage::Release => {
                self.level -= release_inc;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }

        self.level
    }
}
