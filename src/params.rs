use atomic_float::AtomicF32;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// Per-deck synthesis parameters. All fields are atomics for lock-free real-time access.
pub struct DeckParams {
    // Oscillator
    pub osc_waveform: AtomicU8,      // 0=sine 1=saw 2=square 3=triangle
    pub osc_morph: AtomicF32,        // 0.0–1.0 reserved for future morph

    // Jog wheel state — written by MIDI thread, read by audio thread
    pub jog_phase_offset: AtomicF32, // accumulated 0..1, shifts wavetable read position
    pub jog_velocity: AtomicF32,     // -1.0..1.0, drives FM self-modulation depth

    // Filter
    pub filter_cutoff: AtomicF32,
    pub filter_resonance: AtomicF32,

    // ADSR
    pub env_attack: AtomicF32,
    pub env_decay: AtomicF32,
    pub env_sustain: AtomicF32,
    pub env_release: AtomicF32,

    // LFO
    pub lfo_rate: AtomicF32,
    pub lfo_depth: AtomicF32,
    pub lfo_target: AtomicU8,  // 0=none 1=cutoff 2=pitch 3=amp

    // Per-deck pre-fader volume
    pub volume: AtomicF32,
}

impl DeckParams {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            osc_waveform: AtomicU8::new(1), // saw
            osc_morph: AtomicF32::new(0.0),

            jog_phase_offset: AtomicF32::new(0.0),
            jog_velocity: AtomicF32::new(0.0),

            filter_cutoff: AtomicF32::new(4000.0),
            filter_resonance: AtomicF32::new(0.3),

            env_attack: AtomicF32::new(0.01),
            env_decay: AtomicF32::new(0.1),
            env_sustain: AtomicF32::new(0.7),
            env_release: AtomicF32::new(0.3),

            lfo_rate: AtomicF32::new(1.0),
            lfo_depth: AtomicF32::new(0.0),
            lfo_target: AtomicU8::new(0),

            volume: AtomicF32::new(0.8),
        })
    }

    pub fn load_f32(a: &AtomicF32) -> f32 {
        a.load(Ordering::Relaxed)
    }

    pub fn load_u8(a: &AtomicU8) -> u8 {
        a.load(Ordering::Relaxed)
    }
}

/// Global shared parameters (not per-deck).
pub struct SharedParams {
    pub crossfader: AtomicF32,    // 0.0 = full Deck A, 1.0 = full Deck B
    pub master_volume: AtomicF32, // 0.0–1.0
}

impl SharedParams {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            crossfader: AtomicF32::new(0.5),
            master_volume: AtomicF32::new(0.8),
        })
    }
}
