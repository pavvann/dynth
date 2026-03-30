# dynth

A real-time subtractive synthesizer that turns a Pioneer DDJ-FLX4 into a proper synth controller. No DAW, no plugins — just raw audio computed sample-by-sample in Rust.

The jog wheels don't control tempo. They scrub a wavetable and drive FM self-modulation. The crossfader blends two independent synth voices. The EQ knobs sweep a Moog-style ladder filter.

![Rust](https://img.shields.io/badge/rust-1.75+-orange?style=flat-square)

---

## What it sounds like

Two independent synthesizer voices (Deck A and Deck B), each with:
- Wavetable oscillator — sine, saw, square, triangle
- 4-pole ladder filter with resonance
- ADSR envelope
- Sine LFO that can target cutoff, pitch, or amplitude

Blend them with the crossfader. Scratch the jog wheel to scrub the wavetable — the faster you scratch, the more FM distortion gets added to the tone.

---

## Requirements

- Rust 1.75+
- Pioneer DDJ-FLX4 connected via USB
- macOS (CoreAudio / CoreMIDI) — Linux should work with ALSA, untested

---

## Run

```bash
git clone <this repo>
cd dynth
cargo run --release
```

`--release` is not optional — the debug build can't keep up with the audio callback at 44100 Hz.

Custom config:

```bash
cargo run --release -- /path/to/my-config.toml
```

---

## DDJ-FLX4 controls

| Hardware | Deck | What it does |
|---|---|---|
| Left jog wheel | A | Wavetable scrub + FM depth |
| Right jog wheel | B | Wavetable scrub + FM depth |
| EQ HIGH | A / B | Filter cutoff (80–8000 Hz, exponential) |
| EQ MID | A / B | Filter resonance |
| Channel fader | A / B | Deck volume |
| Crossfader | — | Blend Deck A ↔ Deck B |

MIDI channel 1 → Deck A, channel 2 → Deck B for note input. Connect a keyboard to play notes; jog wheel touch pads also trigger voices.

---

## Keyboard controls (in the TUI)

Switch focused deck with `a`, `b`, or `Tab`. All controls below apply to the focused deck.

| Key | Action |
|---|---|
| `1` `2` `3` `4` | Waveform: sine / saw / square / triangle |
| `e` / `E` | Attack +/− |
| `d` / `D` | Decay +/− |
| `s` / `S` | Sustain +/− |
| `r` / `R` | Release +/− |
| `]` / `[` | LFO rate up/down |
| `=` / `-` | LFO depth up/down |
| `t` | Cycle LFO target: None → Cutoff → Pitch → Amp |
| `q` | Quit |

---

## Config

CC mappings live in `config.toml`. Each entry looks like:

```toml
[[mapping]]
device   = "DDJ-FLX4"   # substring match on port name
type     = "cc"
channel  = 1             # 1 = Deck A, 2 = Deck B, 0 = any
cc       = 7
param    = "filter_cutoff"
deck     = 0
min      = 80.0
max      = 8000.0
exp      = true          # exponential scaling (recommended for frequency)
```

To find your device's CC numbers, temporarily add `eprintln!` in the MIDI callback and wiggle controls — see [DYNTH.md](DYNTH.md) for details.

---

## Tests

```bash
cargo test
```

---

## Technical overview

See [DYNTH.md](DYNTH.md) for the full architecture writeup — signal chain, threading model, jog wheel FM mechanics, filter stability details.
