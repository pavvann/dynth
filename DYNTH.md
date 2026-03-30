# dynth

A real-time subtractive synthesizer in pure Rust, built specifically around the Pioneer DDJ-FLX4 as a performance instrument — not a DJ controller.

---

## What it is

Two independent synthesizer voices (Deck A, Deck B) running simultaneously, blended by the DDJ's crossfader. Each deck is a full signal chain: wavetable oscillator → ladder filter → ADSR envelope, with an LFO that can target any parameter. The jog wheels aren't mapped to tempo or pitch — they physically scrub a wavetable and drive FM self-modulation in real time.

No DAW. No VST. No plugin host. Runs as a terminal app.

---

## Running

```bash
cargo run --release
```

Optionally pass a custom config:

```bash
cargo run --release -- /path/to/config.toml
```

`--release` is required. The debug build is too slow for glitch-free audio at 44100 Hz.

---

## Signal chain (per deck)

```
MIDI note-on
    │
    ▼
WavetableOscillator
  • 2048-sample table, pre-computed at startup
  • 4 waveforms: sine, saw, square, triangle
  • Jog phase offset shifts read position (wavetable scrub)
  • Jog velocity drives FM self-modulation:
      fm_hz = freq × |vel| × 0.5 × last_sample
    │
    ▼
LadderFilter  (Moog-style 4-pole)
  • Cutoff: 20 Hz – Nyquist
  • Resonance: 0–3.8 Q (capped below self-oscillation)
  • State clamped ±2.0 per stage — NaN auto-resets
  • Cutoff and resonance smoothed with 5ms one-pole IIR
    │
    ▼
ADSR Envelope
  • Linear ramps, per-sample computation
  • Attack / Decay / Sustain level / Release
    │
    ▼
LFO (sine, audio-rate)
  • Targets: none / cutoff / pitch (±5%) / amplitude
    │
    ▼
Pre-fader deck volume
```

**Engine output:**

```
Deck A ──┐
          ├── constant-power crossfader ──► master volume ──► audio out
Deck B ──┘
```

Crossfader uses `cos`/`sin` on `xf × π/2` so the combined power stays flat across the blend.

---

## Architecture

### Threads

| Thread | Role |
|---|---|
| Main | Runs the ratatui TUI event loop (20 fps) |
| CPAL audio callback | `Engine::process` — fills stereo f32 buffer, lock-free |
| midir callbacks (one per MIDI port) | Parse MIDI, write atomics or send events |

### Parameter sharing

All synthesis parameters are `AtomicF32` / `AtomicU8` — no mutexes in the audio path.

- **`DeckParams`** — per-deck: waveform, cutoff, resonance, ADSR, LFO, jog state, volume
- **`SharedParams`** — global: crossfader position, master volume

MIDI note events (note-on/off) cross thread boundaries via a `crossbeam` unbounded channel; the audio callback drains it non-blocking each buffer.

### Voice allocation

8 voices per deck (16 total). On note-on, the engine picks the first free or finished voice. If all 8 are active, it steals voice 0.

---

## Source files

| File | What it does |
|---|---|
| `main.rs` | CPAL setup, midir setup, wires everything, launches UI |
| `engine.rs` | Audio callback: two `Deck` structs, crossfader mix, NaN guard |
| `params.rs` | `DeckParams` + `SharedParams` — all atomics |
| `oscillator.rs` | `WavetableOscillator` — 4 tables, linear interpolation, FM |
| `filter.rs` | 4-pole ladder filter with state clamping and NaN recovery |
| `envelope.rs` | ADSR state machine |
| `lfo.rs` | Sine LFO |
| `voice.rs` | Bundles oscillator + filter + envelope for one voice |
| `midi.rs` | CC → param routing, jog accumulation, exponential mapping |
| `config.rs` | TOML deserializer for `MidiMapping` |
| `ui.rs` | ratatui TUI — two-deck layout, controls reference, live gauges |
| `tests.rs` | 42 unit tests |

---

## DDJ-FLX4 mappings (confirmed)

| Hardware | MIDI ch | CC | Maps to | Notes |
|---|---|---|---|---|
| Left jog wheel | 1 | 33 | Deck A wavetable scrub + FM | relative, centered at 64 |
| Right jog wheel | 2 | 47 | Deck B wavetable scrub + FM | relative, centered at 64 |
| EQ HIGH (left) | 1 | 7 | Deck A filter cutoff | exponential 80–8000 Hz |
| EQ MID (left) | 1 | 11 | Deck A filter resonance | linear 0–0.85 |
| Channel fader (left) | 1 | 19 | Deck A volume | |
| EQ HIGH (right) | 2 | 7 | Deck B filter cutoff | exponential 80–8000 Hz |
| EQ MID (right) | 2 | 11 | Deck B filter resonance | |
| Channel fader (right) | 2 | 19 | Deck B volume | |
| Crossfader | 7 | 31 | A↔B blend | |

The DDJ sends 14-bit pairs for most controls (coarse + fine CC). dynth uses only the coarse value (lower CC number).

### Jog wheel mechanics

The jog sends relative CC values centered at 64:
- `65–127` = forward (positive delta)
- `63–0` = backward (negative delta)

Each message:
1. Computes `vel = (raw - 64) / 64.0` → `-1.0..1.0`
2. Accumulates `phase_offset += vel × 0.003`, wrapped `rem_euclid(1.0)`
3. Stores `vel` as `jog_velocity` for FM depth

The audio thread reads both atomics once per sample:
- `phase_offset` shifts the wavetable read position → timbre changes from wave shape displacement
- `jog_velocity` drives FM self-modulation → scratching = frequency deviation proportional to current output amplitude

---

## config.toml

Each mapping entry:

```toml
[[mapping]]
device   = "DDJ-FLX4"   # substring match against port name
type     = "cc"
channel  = 1             # 0=any, 1=MIDI-ch1, 2=MIDI-ch2, etc.
cc       = 7
param    = "filter_cutoff"
deck     = 0             # 0=Deck A, 1=Deck B (omit to route by MIDI channel)
min      = 80.0
max      = 8000.0
exp      = true          # exponential curve (use for frequency params)
relative = false         # true = jog mode (delta from 64, accumulates)
```

**Params:** `filter_cutoff`, `filter_resonance`, `volume`, `env_attack`, `env_decay`, `env_sustain`, `env_release`, `lfo_rate`, `lfo_depth`, `osc_morph`, `crossfader`, `master_volume`, `jog`

---

## Keyboard controls

All keyboard controls apply to the focused deck (shown in the title bar).

| Key | Action |
|---|---|
| `a` / `b` / `Tab` | Switch focused deck |
| `1` `2` `3` `4` | Waveform: sine / saw / square / triangle |
| `e` / `E` | Attack +0.01s / −0.01s |
| `d` / `D` | Decay +0.01s / −0.01s |
| `s` / `S` | Sustain +5% / −5% |
| `r` / `R` | Release +0.05s / −0.05s |
| `]` / `[` | LFO rate +0.1 Hz / −0.1 Hz |
| `=` / `-` | LFO depth +5% / −5% |
| `t` | Cycle LFO target: None → Cutoff → Pitch → Amp |
| `q` / `Ctrl-C` | Quit |

---

## Crates

| Crate | Version | Purpose |
|---|---|---|
| `cpal` | 0.15 | Cross-platform audio output |
| `midir` | 0.10 | MIDI input, all ports simultaneously |
| `ratatui` | 0.30 | Terminal UI |
| `crossterm` | 0.29 | Terminal backend for ratatui |
| `atomic_float` | 1.1 | `AtomicF32` for lock-free param sharing |
| `crossbeam-channel` | 0.5 | MIDI event queue (MIDI → audio thread) |
| `serde` + `toml` | 1 / 0.8 | Config file deserialization |
| `anyhow` | 1 | Error handling |

---

## Tests

```bash
cargo test
```

42 tests covering: oscillator waveform math, wavetable phase offset, FM modulation, jog accumulation and wrapping, ADSR stage transitions, filter stability at extremes (high resonance, NaN recovery), LFO range, MIDI note→frequency, CC routing (linear/exp/relative/deck targeting), and atomic param defaults.
