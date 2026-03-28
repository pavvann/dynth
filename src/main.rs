mod config;
mod engine;
mod envelope;
mod filter;
mod lfo;
mod midi;
mod oscillator;
mod params;
mod ui;
mod voice;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::unbounded;
use midir::MidiInput;

use config::Config;
use engine::Engine;
use midi::{apply_cc, MidiEvent};
use params::{DeckParams, SharedParams};
use ui::Ui;

fn main() -> anyhow::Result<()> {
    let config_path = std::env::args().nth(1).unwrap_or_else(|| "config.toml".into());
    let config = Config::load(&config_path)?;

    let params_a = DeckParams::new();
    let params_b = DeckParams::new();
    let shared   = SharedParams::new();

    let (event_tx, event_rx) = unbounded::<MidiEvent>();

    // ── Audio ────────────────────────────────────────────────────────────────
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow::anyhow!("No output device"))?;

    let mut supported = device.supported_output_configs()?;
    let stream_config: cpal::StreamConfig = supported
        .find(|c| c.channels() == 2 && c.sample_format() == cpal::SampleFormat::F32)
        .or_else(|| device.supported_output_configs().ok()?.next())
        .ok_or_else(|| anyhow::anyhow!("No supported output config"))?
        .with_sample_rate(cpal::SampleRate(44100))
        .into();

    let sample_rate = stream_config.sample_rate.0 as f32;

    let mut engine = Engine::new(
        Arc::clone(&params_a),
        Arc::clone(&params_b),
        Arc::clone(&shared),
        event_rx,
        sample_rate,
    );

    let stream = device.build_output_stream(
        &stream_config,
        move |data: &mut [f32], _| engine.process(data),
        |err| eprintln!("Audio error: {err}"),
        None,
    )?;
    stream.play()?;

    // ── MIDI ─────────────────────────────────────────────────────────────────
    let midi_in = MidiInput::new("dynth")?;
    let ports = midi_in.ports();
    let mut midi_device_names: Vec<String> = Vec::new();
    let mut _connections: Vec<midir::MidiInputConnection<()>> = Vec::new();

    for port in &ports {
        let port_name = midi_in.port_name(port)?;
        midi_device_names.push(port_name.clone());

        let mappings  = config.mapping.clone();
        let pa        = Arc::clone(&params_a);
        let pb        = Arc::clone(&params_b);
        let sh        = Arc::clone(&shared);
        let tx        = event_tx.clone();
        let pname     = port_name.clone();

        let conn_in = MidiInput::new(&format!("dynth-{port_name}"))?;
        let conn = conn_in.connect(
            port,
            &port_name,
            move |_ts, msg, _| {
                if msg.len() < 2 { return; }
                let status  = msg[0];
                let kind    = status & 0xF0;
                let channel = status & 0x0F;

                // MIDI channel 0 → Deck A, channel 1 → Deck B
                let deck_id: u8 = if channel == 1 { 1 } else { 0 };

                match kind {
                    0x90 if msg.len() >= 3 && msg[2] > 0 => {
                        let _ = tx.send(MidiEvent::NoteOn {
                            deck: deck_id, note: msg[1], velocity: msg[2],
                        });
                    }
                    0x80 | 0x90 => {
                        let _ = tx.send(MidiEvent::NoteOff { deck: deck_id, note: msg[1] });
                    }
                    0xB0 => {
                        apply_cc(&pa, &pb, &sh, &mappings, &pname, channel, msg[1], msg[2]);
                    }
                    _ => {}
                }
            },
            (),
        )?;

        _connections.push(conn);
    }

    // ── UI ───────────────────────────────────────────────────────────────────
    Ui::new(params_a, params_b, shared, midi_device_names).run()?;

    Ok(())
}
