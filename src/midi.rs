use std::sync::Arc;
use std::sync::atomic::Ordering;

use crate::config::MidiMapping;
use crate::oscillator::JOG_SCALE;
use crate::params::{DeckParams, SharedParams};

pub enum MidiEvent {
    NoteOn  { deck: u8, note: u8, #[allow(dead_code)] velocity: u8 },
    NoteOff { deck: u8, note: u8 },
}

/// Process one relative jog CC message into the deck's jog state.
pub fn update_jog(params: &Arc<DeckParams>, raw: u8) {
    // DDJ-FLX4 jog sends values centered at 64: 65..127=forward, 63..0=backward
    let delta = raw as i8 - 64;
    let vel   = delta as f32 / 64.0; // -1.0..1.0

    // Accumulate phase offset (wraps into 0..1)
    let old   = params.jog_phase_offset.load(Ordering::Relaxed);
    let new   = (old + vel * JOG_SCALE).rem_euclid(1.0);
    params.jog_phase_offset.store(new, Ordering::Relaxed);

    // Store instantaneous velocity for FM depth
    params.jog_velocity.store(vel, Ordering::Relaxed);
}

/// Apply a single CC message to the appropriate params.
pub fn apply_cc(
    deck_a: &Arc<DeckParams>,
    deck_b: &Arc<DeckParams>,
    shared: &Arc<SharedParams>,
    mappings: &[MidiMapping],
    device_name: &str,
    channel: u8,
    cc: u8,
    value: u8,
) {
    for m in mappings {
        if !device_name.contains(&m.device) { continue; }
        if m.channel != 0 && m.channel != channel + 1 { continue; }
        if m.kind != "cc" { continue; }
        if m.cc != Some(cc) { continue; }

        // Relative CC → jog scrub
        if m.relative == Some(true) {
            let target = if m.deck == Some(1) { deck_b } else { deck_a };
            update_jog(target, value);
            continue;
        }

        // Shared param
        if m.param == "crossfader" || m.param == "master_volume" {
            let norm   = value as f32 / 127.0;
            let mapped = m.min + norm * (m.max - m.min);
            apply_shared_param(shared, &m.param, mapped);
            continue;
        }

        // Per-deck param — deck field wins; fall back to MIDI channel
        let target = match m.deck {
            Some(1) => deck_b,
            Some(0) => deck_a,
            _ => if channel == 1 { deck_b } else { deck_a },
        };

        let norm = value as f32 / 127.0;
        let mapped = if m.exp == Some(true) && m.min > 0.0 {
            // Exponential curve: equal perceived steps (good for frequency)
            m.min * (m.max / m.min).powf(norm)
        } else {
            m.min + norm * (m.max - m.min)
        };
        apply_deck_param(target, &m.param, mapped);
    }
}

fn apply_shared_param(shared: &Arc<SharedParams>, param: &str, value: f32) {
    match param {
        "crossfader"    => shared.crossfader.store(value, Ordering::Relaxed),
        "master_volume" => shared.master_volume.store(value, Ordering::Relaxed),
        _ => eprintln!("Unknown shared param: {param}"),
    }
}

fn apply_deck_param(params: &Arc<DeckParams>, param: &str, value: f32) {
    match param {
        "filter_cutoff"    => params.filter_cutoff.store(value, Ordering::Relaxed),
        "filter_resonance" => params.filter_resonance.store(value, Ordering::Relaxed),
        "volume"           => params.volume.store(value, Ordering::Relaxed),
        "osc_morph"        => params.osc_morph.store(value, Ordering::Relaxed),
        "env_attack"       => params.env_attack.store(value, Ordering::Relaxed),
        "env_decay"        => params.env_decay.store(value, Ordering::Relaxed),
        "env_sustain"      => params.env_sustain.store(value, Ordering::Relaxed),
        "env_release"      => params.env_release.store(value, Ordering::Relaxed),
        "lfo_rate"         => params.lfo_rate.store(value, Ordering::Relaxed),
        "lfo_depth"        => params.lfo_depth.store(value, Ordering::Relaxed),
        _ => eprintln!("Unknown deck param: {param}"),
    }
}
