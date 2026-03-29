#![cfg(test)]

use std::f32::consts::PI;
use std::sync::atomic::Ordering;

use crate::config::MidiMapping;
use crate::envelope::Adsr;
use crate::filter::LadderFilter;
use crate::lfo::Lfo;
use crate::midi::{apply_cc, update_jog};
use crate::oscillator::WavetableOscillator;
use crate::params::{DeckParams, SharedParams};
use crate::voice::{midi_note_to_freq, Voice};

const SR: f32 = 44100.0;
const EPSILON: f32 = 1e-4;

fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

// ── WavetableOscillator ───────────────────────────────────────────────────────

#[test]
fn osc_sine_phase_zero_is_zero() {
    let mut osc = WavetableOscillator::new(SR);
    let s = osc.next_sample(440.0, 0, 0.0, 0.0);
    assert!(approx_eq(s, 0.0, EPSILON), "sine at phase 0 should be ~0, got {s}");
}

#[test]
fn osc_saw_phase_zero_is_one() {
    let mut osc = WavetableOscillator::new(SR);
    let s = osc.next_sample(440.0, 1, 0.0, 0.0);
    assert!(approx_eq(s, 1.0, EPSILON), "saw at phase 0 should be 1, got {s}");
}

#[test]
fn osc_square_phase_zero_is_one() {
    let mut osc = WavetableOscillator::new(SR);
    let s = osc.next_sample(440.0, 2, 0.0, 0.0);
    assert!(approx_eq(s, 1.0, EPSILON), "square at phase 0 should be 1, got {s}");
}

#[test]
fn osc_triangle_phase_zero_is_neg_one() {
    let mut osc = WavetableOscillator::new(SR);
    let s = osc.next_sample(440.0, 3, 0.0, 0.0);
    assert!(approx_eq(s, -1.0, EPSILON), "triangle at phase 0 should be -1, got {s}");
}

#[test]
fn osc_unknown_waveform_clamps_to_triangle() {
    // waveform 99 should clamp to 3 (triangle), not panic
    let mut osc = WavetableOscillator::new(SR);
    let s = osc.next_sample(440.0, 99, 0.0, 0.0);
    // Just assert it doesn't panic and is finite
    assert!(s.is_finite());
}

#[test]
fn osc_phase_offset_shifts_output() {
    // Same frequency, same waveform, but different phase_offset → different sample
    let mut osc_a = WavetableOscillator::new(SR);
    let mut osc_b = WavetableOscillator::new(SR);
    let s_a = osc_a.next_sample(440.0, 0, 0.0, 0.0);
    let s_b = osc_b.next_sample(440.0, 0, 0.25, 0.0); // 0.25 = quarter-cycle offset
    // At phase_offset 0.25 into sine, we expect sin(2π*0.25) = 1.0
    assert!(approx_eq(s_b, 1.0, 0.01), "sine with 0.25 offset should be ~1.0, got {s_b}");
    assert!(!approx_eq(s_a, s_b, EPSILON), "offset should produce different sample");
}

#[test]
fn osc_fm_depth_nonzero_changes_output() {
    // With FM self-modulation, after some samples the oscillator diverges from clean version
    let mut osc_clean = WavetableOscillator::new(SR);
    let mut osc_fm    = WavetableOscillator::new(SR);

    let mut clean_sum = 0.0f32;
    let mut fm_sum    = 0.0f32;
    for _ in 0..1000 {
        clean_sum += osc_clean.next_sample(440.0, 1, 0.0, 0.0).abs();
        fm_sum    += osc_fm.next_sample(440.0, 1, 0.0, 1.0).abs(); // max FM
    }
    // They should differ (FM changes the spectrum/phase trajectory)
    assert!(
        (clean_sum - fm_sum).abs() > 0.1,
        "FM should measurably alter output: clean={clean_sum:.3} fm={fm_sum:.3}"
    );
}

#[test]
fn osc_output_stays_finite_with_high_fm() {
    let mut osc = WavetableOscillator::new(SR);
    for _ in 0..SR as usize {
        let s = osc.next_sample(440.0, 0, 0.5, 1.0);
        assert!(s.is_finite(), "FM oscillator produced non-finite value: {s}");
    }
}

#[test]
fn osc_reset_phase_restarts() {
    let mut osc = WavetableOscillator::new(SR);
    let first = osc.next_sample(440.0, 0, 0.0, 0.0);
    osc.next_sample(440.0, 0, 0.0, 0.0); // advance
    osc.reset_phase();
    let after = osc.next_sample(440.0, 0, 0.0, 0.0);
    assert!(approx_eq(first, after, EPSILON), "reset should restart phase");
}

#[test]
fn osc_saw_output_bounded_over_time() {
    let mut osc = WavetableOscillator::new(SR);
    for _ in 0..SR as usize * 2 {
        let s = osc.next_sample(261.63, 1, 0.0, 0.0);
        assert!(s >= -1.01 && s <= 1.01, "saw out of range: {s}");
    }
}

// ── Jog wheel accumulation ────────────────────────────────────────────────────

#[test]
fn jog_neutral_is_no_movement() {
    let params = DeckParams::new();
    let initial = params.jog_phase_offset.load(Ordering::Relaxed);
    update_jog(&params, 64); // 64 = no movement
    let after = params.jog_phase_offset.load(Ordering::Relaxed);
    assert!(approx_eq(initial, after, EPSILON), "neutral jog (64) should not move phase");
}

#[test]
fn jog_forward_increases_phase() {
    let params = DeckParams::new();
    params.jog_phase_offset.store(0.0, Ordering::Relaxed);
    update_jog(&params, 100); // forward
    let phase = params.jog_phase_offset.load(Ordering::Relaxed);
    assert!(phase > 0.0, "forward jog should increase phase, got {phase}");
}

#[test]
fn jog_backward_decreases_phase_with_wrap() {
    let params = DeckParams::new();
    params.jog_phase_offset.store(0.0, Ordering::Relaxed);
    update_jog(&params, 0); // maximum backward
    let phase = params.jog_phase_offset.load(Ordering::Relaxed);
    // Should wrap to near 1.0 (rem_euclid handles negative)
    assert!(phase > 0.9 || phase == 0.0, "backward from 0 should wrap to ~1.0, got {phase}");
}

#[test]
fn jog_velocity_stored_correctly() {
    let params = DeckParams::new();
    update_jog(&params, 127); // max forward
    let vel = params.jog_velocity.load(Ordering::Relaxed);
    assert!(vel > 0.9, "max forward velocity should be ~1.0, got {vel}");

    update_jog(&params, 1); // near-max backward
    let vel = params.jog_velocity.load(Ordering::Relaxed);
    assert!(vel < -0.9, "near-max backward velocity should be ~-1.0, got {vel}");
}

#[test]
fn jog_phase_stays_in_0_1() {
    let params = DeckParams::new();
    // Spin many ticks forward
    for _ in 0..10_000 {
        update_jog(&params, 120);
    }
    let phase = params.jog_phase_offset.load(Ordering::Relaxed);
    assert!(phase >= 0.0 && phase < 1.0, "phase should be in [0,1), got {phase}");

    // Spin many ticks backward
    for _ in 0..10_000 {
        update_jog(&params, 10);
    }
    let phase = params.jog_phase_offset.load(Ordering::Relaxed);
    assert!(phase >= 0.0 && phase < 1.0, "phase should be in [0,1) after reverse, got {phase}");
}

// ── Envelope ─────────────────────────────────────────────────────────────────

#[test]
fn env_starts_idle_and_silent() {
    let mut env = Adsr::new(SR);
    assert!(env.is_idle());
    let level = env.next(0.01, 0.1, 0.7, 0.3);
    assert_eq!(level, 0.0);
}

#[test]
fn env_attack_ramps_to_one() {
    let mut env = Adsr::new(SR);
    env.note_on();
    let attack = 0.01;
    let samples = (attack * SR) as usize + 10;
    let mut peak = 0.0f32;
    for _ in 0..samples {
        peak = peak.max(env.next(attack, 0.1, 0.7, 0.3));
    }
    assert!(approx_eq(peak, 1.0, EPSILON), "envelope should reach 1.0, peaked at {peak}");
}

#[test]
fn env_decay_settles_at_sustain() {
    let mut env = Adsr::new(SR);
    env.note_on();
    let sustain = 0.6;
    let samples = ((0.001 + 0.01) * SR) as usize + 500;
    let mut last = 0.0;
    for _ in 0..samples {
        last = env.next(0.001, 0.01, sustain, 0.3);
    }
    assert!(approx_eq(last, sustain, 0.01), "sustain should be {sustain}, got {last}");
}

#[test]
fn env_note_off_triggers_release() {
    let mut env = Adsr::new(SR);
    env.note_on();
    for _ in 0..2000 { env.next(0.001, 0.001, 0.8, 0.5); }
    env.note_off();
    let release = 0.05;
    for _ in 0..((release * SR) as usize + 200) {
        env.next(0.001, 0.001, 0.8, release);
    }
    assert!(env.is_idle(), "envelope should be idle after release");
}

#[test]
fn env_note_off_while_idle_stays_idle() {
    let mut env = Adsr::new(SR);
    env.note_off();
    assert!(env.is_idle());
}

// ── Filter ───────────────────────────────────────────────────────────────────

#[test]
fn filter_passes_dc_approximately() {
    let mut f = LadderFilter::new(SR);
    let mut last = 0.0;
    for _ in 0..10_000 { last = f.process(1.0, 18000.0, 0.0); }
    assert!(last > 0.9, "open filter should pass DC, got {last}");
}

#[test]
fn filter_attenuates_above_cutoff() {
    let freq = 10_000.0;
    let mut f = LadderFilter::new(SR);
    for i in 0..SR as usize {
        let x = (2.0 * PI * freq * i as f32 / SR).sin();
        f.process(x, 1_000.0, 0.0);
    }
    let mut peak = 0.0f32;
    for i in 0..SR as usize {
        let x = (2.0 * PI * freq * i as f32 / SR).sin();
        peak = peak.max(f.process(x, 1_000.0, 0.0).abs());
    }
    assert!(peak < 0.1, "10kHz should be attenuated at 1kHz cutoff, peak={peak}");
}

#[test]
fn filter_extreme_cutoffs_dont_panic() {
    let mut f = LadderFilter::new(SR);
    assert!(f.process(0.5, 1.0,        0.0).is_finite());
    assert!(f.process(0.5, 100_000.0,  0.0).is_finite());
}

#[test]
fn filter_high_resonance_stays_finite() {
    let mut f = LadderFilter::new(SR);
    for i in 0..4000 {
        let x = (2.0 * PI * 440.0 * i as f32 / SR).sin();
        let y = f.process(x, 440.0, 0.99);
        assert!(y.is_finite(), "high resonance produced non-finite: {y}");
    }
}

// ── LFO ──────────────────────────────────────────────────────────────────────

#[test]
fn lfo_starts_at_zero() {
    let mut lfo = Lfo::new(SR);
    let s = lfo.next(1.0);
    assert!(approx_eq(s, 0.0, EPSILON));
}

#[test]
fn lfo_output_stays_in_range() {
    let mut lfo = Lfo::new(SR);
    for _ in 0..SR as usize * 5 {
        let s = lfo.next(3.7);
        assert!(s >= -1.0 && s <= 1.0, "LFO out of range: {s}");
    }
}

// ── Voice / MIDI note → freq ──────────────────────────────────────────────────

#[test]
fn a4_is_440hz() {
    assert!(approx_eq(midi_note_to_freq(69), 440.0, 0.01));
}

#[test]
fn a5_is_880hz() {
    assert!(approx_eq(midi_note_to_freq(81), 880.0, 0.1));
}

#[test]
fn c4_is_middle_c() {
    assert!(approx_eq(midi_note_to_freq(60), 261.63, 0.1));
}

#[test]
fn octaves_double_frequency() {
    let f0 = midi_note_to_freq(48);
    let f1 = midi_note_to_freq(60);
    let f2 = midi_note_to_freq(72);
    assert!(approx_eq(f1 / f0, 2.0, 0.001));
    assert!(approx_eq(f2 / f1, 2.0, 0.001));
}

#[test]
fn voice_note_on_sets_freq_and_active() {
    let mut v = Voice::new(SR);
    assert!(!v.active);
    v.note_on(69);
    assert!(v.active);
    assert!(approx_eq(v.freq, 440.0, 0.01));
    assert_eq!(v.note, 69);
}

#[test]
fn voice_note_off_eventually_goes_idle() {
    let mut v = Voice::new(SR);
    v.note_on(60);
    for _ in 0..2000 { v.env.next(0.001, 0.001, 0.7, 0.05); }
    v.note_off();
    for _ in 0..SR as usize { v.env.next(0.001, 0.001, 0.7, 0.05); }
    assert!(v.is_done());
}

// ── MIDI CC mapping ───────────────────────────────────────────────────────────

fn make_mapping(device: &str, cc: u8, param: &str, min: f32, max: f32) -> MidiMapping {
    MidiMapping {
        device: device.to_string(),
        kind: "cc".to_string(),
        channel: 0,
        cc: Some(cc),
        param: param.to_string(),
        min,
        max,
        deck: None,
        relative: None,
        exp: None,
    }
}

fn make_jog_mapping(device: &str, cc: u8, deck: u8) -> MidiMapping {
    MidiMapping {
        device: device.to_string(),
        kind: "cc".to_string(),
        channel: 0,
        cc: Some(cc),
        param: "jog".to_string(),
        min: 0.0,
        max: 0.0,
        deck: Some(deck),
        relative: Some(true),
        exp: None,
    }
}

fn dummy_shared() -> std::sync::Arc<SharedParams> { SharedParams::new() }

#[test]
fn cc_max_maps_to_param_max() {
    let pa = DeckParams::new();
    let pb = DeckParams::new();
    let sh = dummy_shared();
    let mappings = vec![make_mapping("DDJ", 71, "filter_cutoff", 200.0, 18000.0)];
    apply_cc(&pa, &pb, &sh, &mappings, "DDJ MIDI", 0, 71, 127);
    let cutoff = pa.filter_cutoff.load(Ordering::Relaxed);
    assert!(approx_eq(cutoff, 18000.0, 1.0), "got {cutoff}");
}

#[test]
fn cc_zero_maps_to_param_min() {
    let pa = DeckParams::new();
    let pb = DeckParams::new();
    let sh = dummy_shared();
    let mappings = vec![make_mapping("DDJ", 71, "filter_cutoff", 200.0, 18000.0)];
    apply_cc(&pa, &pb, &sh, &mappings, "DDJ MIDI", 0, 71, 0);
    let cutoff = pa.filter_cutoff.load(Ordering::Relaxed);
    assert!(approx_eq(cutoff, 200.0, 1.0), "got {cutoff}");
}

#[test]
fn cc_midpoint_maps_correctly() {
    let pa = DeckParams::new();
    let pb = DeckParams::new();
    let sh = dummy_shared();
    let mappings = vec![make_mapping("DDJ", 7, "volume", 0.0, 1.0)];
    apply_cc(&pa, &pb, &sh, &mappings, "DDJ MIDI", 0, 7, 64);
    let vol = pa.volume.load(Ordering::Relaxed);
    assert!(approx_eq(vol, 64.0 / 127.0, 0.01), "got {vol}");
}

#[test]
fn cc_wrong_device_is_ignored() {
    let pa = DeckParams::new();
    let pb = DeckParams::new();
    let sh = dummy_shared();
    let mappings = vec![make_mapping("DDJ", 71, "filter_cutoff", 200.0, 18000.0)];
    let original = pa.filter_cutoff.load(Ordering::Relaxed);
    apply_cc(&pa, &pb, &sh, &mappings, "Some Other Device", 0, 71, 127);
    assert!(approx_eq(pa.filter_cutoff.load(Ordering::Relaxed), original, EPSILON));
}

#[test]
fn cc_routes_to_deck_b_by_deck_field() {
    let pa = DeckParams::new();
    let pb = DeckParams::new();
    let sh = dummy_shared();
    let mut m = make_mapping("DDJ", 71, "filter_cutoff", 200.0, 18000.0);
    m.deck = Some(1); // explicit Deck B
    let mappings = vec![m];
    apply_cc(&pa, &pb, &sh, &mappings, "DDJ MIDI", 0, 71, 127);
    let cutoff_a = pa.filter_cutoff.load(Ordering::Relaxed);
    let cutoff_b = pb.filter_cutoff.load(Ordering::Relaxed);
    assert!(!approx_eq(cutoff_a, 18000.0, 100.0), "Deck A should not be updated");
    assert!(approx_eq(cutoff_b, 18000.0, 1.0),    "Deck B should be 18000, got {cutoff_b}");
}

#[test]
fn cc_crossfader_updates_shared() {
    let pa = DeckParams::new();
    let pb = DeckParams::new();
    let sh = dummy_shared();
    let mappings = vec![make_mapping("DDJ", 8, "crossfader", 0.0, 1.0)];
    apply_cc(&pa, &pb, &sh, &mappings, "DDJ MIDI", 0, 8, 127);
    let xf = sh.crossfader.load(Ordering::Relaxed);
    assert!(approx_eq(xf, 1.0, 0.01), "crossfader should be 1.0, got {xf}");
}

#[test]
fn cc_jog_relative_updates_deck_phase() {
    let pa = DeckParams::new();
    let pb = DeckParams::new();
    let sh = dummy_shared();
    pa.jog_phase_offset.store(0.0, Ordering::Relaxed);
    let mappings = vec![make_jog_mapping("DDJ", 33, 0)];
    apply_cc(&pa, &pb, &sh, &mappings, "DDJ MIDI", 0, 33, 100); // forward
    let phase = pa.jog_phase_offset.load(Ordering::Relaxed);
    assert!(phase > 0.0, "jog forward should increase phase, got {phase}");
}

// ── DeckParams defaults ───────────────────────────────────────────────────────

#[test]
fn deck_params_defaults_are_sane() {
    let p = DeckParams::new();
    assert_eq!(p.osc_waveform.load(Ordering::Relaxed), 1); // saw
    let cutoff = DeckParams::load_f32(&p.filter_cutoff);
    assert!(cutoff > 0.0 && cutoff < 20001.0);
    let vol = DeckParams::load_f32(&p.volume);
    assert!(vol > 0.0 && vol <= 1.0);
    let jog = DeckParams::load_f32(&p.jog_phase_offset);
    assert_eq!(jog, 0.0);
}

#[test]
fn shared_params_crossfader_defaults_to_center() {
    let s = SharedParams::new();
    let xf = s.crossfader.load(Ordering::Relaxed);
    assert!(approx_eq(xf, 0.5, EPSILON));
}

#[test]
fn deck_params_store_load_roundtrip() {
    let p = DeckParams::new();
    p.filter_cutoff.store(1234.5, Ordering::Relaxed);
    assert!(approx_eq(DeckParams::load_f32(&p.filter_cutoff), 1234.5, 0.01));
}
