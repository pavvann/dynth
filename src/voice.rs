use crate::oscillator::WavetableOscillator;
use crate::filter::LadderFilter;
use crate::envelope::Adsr;

pub struct Voice {
    pub osc: WavetableOscillator,
    pub filter: LadderFilter,
    pub env: Adsr,
    pub note: u8,
    pub freq: f32,
    pub active: bool,
}

impl Voice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            osc: WavetableOscillator::new(sample_rate),
            filter: LadderFilter::new(sample_rate),
            env: Adsr::new(sample_rate),
            note: 0,
            freq: 440.0,
            active: false,
        }
    }

    pub fn note_on(&mut self, note: u8) {
        self.note = note;
        self.freq = midi_note_to_freq(note);
        self.active = true;
        self.osc.reset_phase();
        self.env.note_on();
    }

    pub fn note_off(&mut self) {
        self.env.note_off();
    }

    pub fn is_done(&self) -> bool {
        self.env.is_idle()
    }
}

pub fn midi_note_to_freq(note: u8) -> f32 {
    440.0 * 2f32.powf((note as f32 - 69.0) / 12.0)
}
