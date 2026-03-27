use std::sync::Arc;
use std::f32::consts::FRAC_PI_2;
use crossbeam_channel::Receiver;

use crate::params::{DeckParams, SharedParams};
use crate::voice::Voice;
use crate::lfo::Lfo;
use crate::midi::MidiEvent;

const MAX_VOICES: usize = 8;

// One-pole smoothing coefficient: ~5ms time constant at 44100Hz
// smooth = 1 - e^(-1 / (0.005 * 44100)) ≈ 0.0045
const SMOOTH: f32 = 0.0045;

struct Deck {
    voices: [Voice; MAX_VOICES],
    lfo: Lfo,
    params: Arc<DeckParams>,
    // Smoothed parameter values to eliminate zipper noise
    smooth_cutoff: f32,
    smooth_resonance: f32,
}

impl Deck {
    fn new(params: Arc<DeckParams>, sample_rate: f32) -> Self {
        use crate::params::DeckParams as P;
        let cutoff    = P::load_f32(&params.filter_cutoff);
        let resonance = P::load_f32(&params.filter_resonance);
        Self {
            voices: std::array::from_fn(|_| Voice::new(sample_rate)),
            lfo: Lfo::new(sample_rate),
            smooth_cutoff: cutoff,
            smooth_resonance: resonance,
            params,
        }
    }

    fn note_on(&mut self, note: u8) {
        let slot = self.find_free_voice().unwrap_or(0);
        self.voices[slot].note_on(note);
    }

    fn note_off(&mut self, note: u8) {
        for v in self.voices.iter_mut() {
            if v.active && v.note == note {
                v.note_off();
            }
        }
    }

    fn find_free_voice(&self) -> Option<usize> {
        self.voices.iter().position(|v| !v.active || v.is_done())
    }

    /// Process one sample and return a mono value.
    fn process_sample(&mut self) -> f32 {
        use crate::params::DeckParams as P;

        let waveform   = P::load_u8(&self.params.osc_waveform);

        // Smooth cutoff and resonance toward target to eliminate zipper noise
        let target_cutoff    = P::load_f32(&self.params.filter_cutoff);
        let target_resonance = P::load_f32(&self.params.filter_resonance);
        self.smooth_cutoff    += SMOOTH * (target_cutoff    - self.smooth_cutoff);
        self.smooth_resonance += SMOOTH * (target_resonance - self.smooth_resonance);
        let cutoff    = self.smooth_cutoff;
        let resonance = self.smooth_resonance;
        let attack     = P::load_f32(&self.params.env_attack);
        let decay      = P::load_f32(&self.params.env_decay);
        let sustain    = P::load_f32(&self.params.env_sustain);
        let release    = P::load_f32(&self.params.env_release);
        let lfo_rate   = P::load_f32(&self.params.lfo_rate);
        let lfo_depth  = P::load_f32(&self.params.lfo_depth);
        let lfo_target = P::load_u8(&self.params.lfo_target);
        let volume     = P::load_f32(&self.params.volume);
        let jog_offset = P::load_f32(&self.params.jog_phase_offset);
        let jog_vel    = P::load_f32(&self.params.jog_velocity);

        let lfo_val = self.lfo.next(lfo_rate);

        let effective_cutoff = if lfo_target == 1 {
            (cutoff + lfo_val * lfo_depth * cutoff).clamp(20.0, 20000.0)
        } else {
            cutoff
        };

        let mut out = 0.0f32;

        for v in self.voices.iter_mut() {
            if !v.active { continue; }

            let freq = if lfo_target == 2 {
                v.freq * (1.0 + lfo_val * lfo_depth * 0.05)
            } else {
                v.freq
            };

            let osc_out  = v.osc.next_sample(freq, waveform, jog_offset, jog_vel);
            let filt_out = v.filter.process(osc_out, effective_cutoff, resonance);
            let env_amp  = v.env.next(attack, decay, sustain, release);

            let amp = if lfo_target == 3 {
                env_amp * (1.0 + lfo_val * lfo_depth * 0.5).max(0.0)
            } else {
                env_amp
            };

            out += filt_out * amp;

            if v.is_done() {
                v.active = false;
            }
        }

        out * volume
    }
}

pub struct Engine {
    deck_a: Deck,
    deck_b: Deck,
    shared: Arc<SharedParams>,
    events: Receiver<MidiEvent>,
}

impl Engine {
    pub fn new(
        params_a: Arc<DeckParams>,
        params_b: Arc<DeckParams>,
        shared: Arc<SharedParams>,
        events: Receiver<MidiEvent>,
        sample_rate: f32,
    ) -> Self {
        Self {
            deck_a: Deck::new(params_a, sample_rate),
            deck_b: Deck::new(params_b, sample_rate),
            shared,
            events,
        }
    }

    /// Fill interleaved stereo f32 output buffer. Real-time safe.
    pub fn process(&mut self, output: &mut [f32]) {
        // Drain MIDI events
        while let Ok(event) = self.events.try_recv() {
            match event {
                MidiEvent::NoteOn  { deck: 1, note, .. } => self.deck_b.note_on(note),
                MidiEvent::NoteOn  { note, .. }           => self.deck_a.note_on(note),
                MidiEvent::NoteOff { deck: 1, note }      => self.deck_b.note_off(note),
                MidiEvent::NoteOff { note, .. }           => self.deck_a.note_off(note),
            }
        }

        let xf    = self.shared.crossfader.load(std::sync::atomic::Ordering::Relaxed);
        let angle = xf * FRAC_PI_2;
        let gain_a = angle.cos(); // 1→0 as xf goes 0→1
        let gain_b = angle.sin(); // 0→1
        let master = self.shared.master_volume.load(std::sync::atomic::Ordering::Relaxed);

        let frames = output.len() / 2;
        for i in 0..frames {
            let sa = self.deck_a.process_sample();
            let sb = self.deck_b.process_sample();
            let mixed = (sa * gain_a + sb * gain_b) * master;
            let clipped = if mixed.is_finite() { mixed.clamp(-1.0, 1.0) } else { 0.0 };
            output[i * 2]     = clipped;
            output[i * 2 + 1] = clipped;
        }
    }
}
