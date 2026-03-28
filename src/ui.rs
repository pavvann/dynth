use std::sync::Arc;
use std::time::{Duration, Instant};
use std::sync::atomic::Ordering;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Chart, Dataset, Gauge, GraphType, List, ListItem, Paragraph,
    },
    Frame, Terminal,
};

use crate::params::{DeckParams, SharedParams};

const WAVEFORM_NAMES: [&str; 4] = ["Sine", "Saw", "Square", "Triangle"];

fn nudge_f32(a: &atomic_float::AtomicF32, delta: f32, min: f32, max: f32) {
    let v = (a.load(Ordering::Relaxed) + delta).clamp(min, max);
    a.store(v, Ordering::Relaxed);
}
const LFO_TARGET_NAMES: [&str; 4] = ["None", "Cutoff", "Pitch", "Amp"];

pub struct Ui {
    params_a: Arc<DeckParams>,
    params_b: Arc<DeckParams>,
    shared: Arc<SharedParams>,
    midi_devices: Vec<String>,
    /// 0 = Deck A focused, 1 = Deck B focused
    active_deck: u8,
}

impl Ui {
    pub fn new(
        params_a: Arc<DeckParams>,
        params_b: Arc<DeckParams>,
        shared: Arc<SharedParams>,
        midi_devices: Vec<String>,
    ) -> Self {
        Self { params_a, params_b, shared, midi_devices, active_deck: 0 }
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let tick = Duration::from_millis(50);
        let mut last_tick = Instant::now();

        loop {
            terminal.draw(|f| self.render(f))?;

            let timeout = tick.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    let quit = matches!(key.code, KeyCode::Char('q'))
                        || (key.code == KeyCode::Char('c')
                            && key.modifiers.contains(KeyModifiers::CONTROL));
                    if quit { break; }

                    let params = if self.active_deck == 0 { &self.params_a } else { &self.params_b };

                    match key.code {
                        // Deck select
                        KeyCode::Tab       => self.active_deck ^= 1,
                        KeyCode::Char('a') => self.active_deck = 0,
                        KeyCode::Char('b') => self.active_deck = 1,

                        // Waveform
                        KeyCode::Char('1') => params.osc_waveform.store(0, Ordering::Relaxed),
                        KeyCode::Char('2') => params.osc_waveform.store(1, Ordering::Relaxed),
                        KeyCode::Char('3') => params.osc_waveform.store(2, Ordering::Relaxed),
                        KeyCode::Char('4') => params.osc_waveform.store(3, Ordering::Relaxed),

                        // Envelope: A/D/S/R — Up/Down arrows while pressing key
                        KeyCode::Char('e') => nudge_f32(&params.env_attack,  0.01, 0.001, 4.0),
                        KeyCode::Char('E') => nudge_f32(&params.env_attack,  -0.01, 0.001, 4.0),
                        KeyCode::Char('d') => nudge_f32(&params.env_decay,   0.01, 0.001, 4.0),
                        KeyCode::Char('D') => nudge_f32(&params.env_decay,   -0.01, 0.001, 4.0),
                        KeyCode::Char('s') => nudge_f32(&params.env_sustain, 0.05, 0.0, 1.0),
                        KeyCode::Char('S') => nudge_f32(&params.env_sustain, -0.05, 0.0, 1.0),
                        KeyCode::Char('r') => nudge_f32(&params.env_release, 0.05, 0.001, 8.0),
                        KeyCode::Char('R') => nudge_f32(&params.env_release, -0.05, 0.001, 8.0),

                        // LFO rate/depth
                        KeyCode::Char(']') => nudge_f32(&params.lfo_rate,  0.1, 0.0, 20.0),
                        KeyCode::Char('[') => nudge_f32(&params.lfo_rate,  -0.1, 0.0, 20.0),
                        KeyCode::Char('=') => nudge_f32(&params.lfo_depth, 0.05, 0.0, 1.0),
                        KeyCode::Char('-') => nudge_f32(&params.lfo_depth, -0.05, 0.0, 1.0),

                        // LFO target cycle: none → cutoff → pitch → amp → none
                        KeyCode::Char('t') => {
                            let t = params.lfo_target.load(Ordering::Relaxed);
                            params.lfo_target.store((t + 1) % 4, Ordering::Relaxed);
                        }

                        _ => {}
                    }
                }
            }

            if last_tick.elapsed() >= tick { last_tick = Instant::now(); }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }

    fn render(&self, f: &mut Frame) {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // app name bar
                Constraint::Length(5), // controls reference
                Constraint::Min(0),    // decks
                Constraint::Length(4), // crossfader + status
            ])
            .split(f.area());

        self.render_title(f, root[0]);
        self.render_controls(f, root[1]);

        let decks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(root[2]);

        self.render_deck(f, decks[0], &self.params_a, "A", 0);
        self.render_deck(f, decks[1], &self.params_b, "B", 1);

        self.render_crossfader(f, root[3]);
    }

    fn render_title(&self, f: &mut Frame, area: Rect) {
        let deck_label = if self.active_deck == 0 { "DECK A" } else { "DECK B" };
        let color      = if self.active_deck == 0 { Color::Cyan } else { Color::Magenta };
        let title = Paragraph::new(Line::from(vec![
            Span::styled(" dynth ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled("▸ two-deck synthesizer   ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("editing: {deck_label}"), Style::default().fg(color).add_modifier(Modifier::BOLD)),
        ]))
        .style(Style::default().bg(Color::Black));
        f.render_widget(title, area);
    }

    fn render_controls(&self, f: &mut Frame, area: Rect) {
        let key = |k: &'static str| Span::styled(k, Style::default().fg(Color::White).add_modifier(Modifier::BOLD));
        let lbl = |l: &'static str| Span::styled(l, Style::default().fg(Color::DarkGray));
        let sep = || Span::styled("  ", Style::default());

        let rows = vec![
            Line::from(vec![
                key("[a]"), lbl(" Deck A  "), key("[b]"), lbl(" Deck B  "), key("[Tab]"), lbl(" toggle deck"),
                sep(),
                key("[1]"), lbl(" Sine  "), key("[2]"), lbl(" Saw  "), key("[3]"), lbl(" Square  "), key("[4]"), lbl(" Triangle"),
            ]),
            Line::from(vec![
                key("[e/E]"), lbl(" Attack ±  "),
                key("[d/D]"), lbl(" Decay ±  "),
                key("[s/S]"), lbl(" Sustain ±  "),
                key("[r/R]"), lbl(" Release ±"),
            ]),
            Line::from(vec![
                key("[]/[[]"), lbl(" LFO rate ±  "),
                key("[=/-]"), lbl(" LFO depth ±  "),
                key("[t]"), lbl(" LFO target: None → Cutoff → Pitch → Amp"),
            ]),
            Line::from(vec![
                key("[q]"), lbl(" quit"),
            ]),
        ];

        use ratatui::widgets::Paragraph as P;
        let widget = P::new(rows)
            .block(Block::default().title(" Controls ").borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)));
        f.render_widget(widget, area);
    }

    fn render_deck(&self, f: &mut Frame, area: Rect, params: &Arc<DeckParams>, label: &str, id: u8) {
        let focused = self.active_deck == id;
        let border_color = if focused {
            if id == 0 { Color::Cyan } else { Color::Magenta }
        } else {
            Color::DarkGray
        };

        let title_str = if focused {
            format!(" ▶ DECK {label} (MIDI ch {}) ", id + 1)
        } else {
            format!("   DECK {label} (MIDI ch {}) ", id + 1)
        };

        let outer = Block::default()
            .title(title_str)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));
        let inner = outer.inner(area);
        f.render_widget(outer, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),  // osc + filter side by side
                Constraint::Min(8),     // envelope chart
                Constraint::Length(6),  // LFO
                Constraint::Length(3),  // jog
            ])
            .split(inner);

        let top = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[0]);

        self.render_osc(f, top[0], params, border_color);
        self.render_filter(f, top[1], params, border_color);
        self.render_envelope(f, rows[1], params, border_color);
        self.render_lfo(f, rows[2], params, border_color);
        self.render_jog(f, rows[3], params, border_color);
    }

    fn render_osc(&self, f: &mut Frame, area: Rect, params: &Arc<DeckParams>, color: Color) {
        let waveform = params.osc_waveform.load(Ordering::Relaxed) as usize;

        let items: Vec<ListItem> = WAVEFORM_NAMES.iter().enumerate().map(|(i, name)| {
            let selected = i == waveform;
            let style = if selected {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            ListItem::new(format!("{} {name}", if selected { "▶" } else { " " })).style(style)
        }).collect();

        let list = List::new(items).block(
            Block::default().title(" Osc ").borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
        );
        f.render_widget(list, area);
    }

    fn render_filter(&self, f: &mut Frame, area: Rect, params: &Arc<DeckParams>, _color: Color) {
        let cutoff    = DeckParams::load_f32(&params.filter_cutoff);
        let resonance = DeckParams::load_f32(&params.filter_resonance);

        let block = Block::default().title(" Filter ").borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), Constraint::Length(1),
                Constraint::Length(1), Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(inner);

        f.render_widget(
            Paragraph::new(format!("Cut {:>6.0}Hz", cutoff))
                .style(Style::default().fg(Color::White)),
            rows[0],
        );
        let cutoff_r = ((cutoff - 20.0) / (20000.0 - 20.0)).clamp(0.0, 1.0) as f64;
        f.render_widget(
            Gauge::default().gauge_style(Style::default().fg(Color::Magenta).bg(Color::DarkGray))
                .ratio(cutoff_r).label(""),
            rows[1],
        );
        f.render_widget(
            Paragraph::new(format!("Res {:>8.2}", resonance))
                .style(Style::default().fg(Color::White)),
            rows[2],
        );
        f.render_widget(
            Gauge::default().gauge_style(Style::default().fg(Color::Magenta).bg(Color::DarkGray))
                .ratio(resonance as f64).label(""),
            rows[3],
        );
    }

    fn render_envelope(&self, f: &mut Frame, area: Rect, params: &Arc<DeckParams>, _color: Color) {
        let attack  = DeckParams::load_f32(&params.env_attack);
        let decay   = DeckParams::load_f32(&params.env_decay);
        let sustain = DeckParams::load_f32(&params.env_sustain);
        let release = DeckParams::load_f32(&params.env_release);

        let total = (attack + decay + 0.3 + release).max(0.1);
        let a_end = attack / total;
        let d_end = a_end + decay / total;
        let s_end = d_end + 0.3 / total;

        let data: Vec<(f64, f64)> = vec![
            (0.0,           0.0),
            (a_end as f64,  1.0),
            (d_end as f64,  sustain as f64),
            (s_end as f64,  sustain as f64),
            (1.0,           0.0),
        ];

        let dataset = Dataset::default()
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Yellow))
            .data(&data);

        let chart = Chart::new(vec![dataset])
            .block(
                Block::default()
                    .title(format!(" Env  A:{:.2} D:{:.2} S:{:.0}% R:{:.2} ",
                        attack, decay, sustain * 100.0, release))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .x_axis(Axis::default().bounds([0.0, 1.0])
                .labels(vec![
                    Span::styled("A", Style::default().fg(Color::DarkGray)),
                    Span::styled("D", Style::default().fg(Color::DarkGray)),
                    Span::styled("S", Style::default().fg(Color::DarkGray)),
                    Span::styled("R", Style::default().fg(Color::DarkGray)),
                ]))
            .y_axis(Axis::default().bounds([0.0, 1.0])
                .labels(vec![
                    Span::styled("0", Style::default().fg(Color::DarkGray)),
                    Span::styled("1", Style::default().fg(Color::DarkGray)),
                ]));

        f.render_widget(chart, area);
    }

    fn render_lfo(&self, f: &mut Frame, area: Rect, params: &Arc<DeckParams>, _color: Color) {
        let rate   = DeckParams::load_f32(&params.lfo_rate);
        let depth  = DeckParams::load_f32(&params.lfo_depth);
        let target = params.lfo_target.load(Ordering::Relaxed) as usize;

        let block = Block::default().title(" LFO ").borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), Constraint::Length(1),
                Constraint::Length(1), Constraint::Length(1),
            ])
            .split(inner);

        f.render_widget(
            Paragraph::new(format!("Rate  {:.2}Hz  → {}", rate,
                LFO_TARGET_NAMES.get(target).unwrap_or(&"?")))
                .style(Style::default().fg(Color::White)),
            rows[0],
        );
        f.render_widget(
            Gauge::default().gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
                .ratio((rate / 20.0).clamp(0.0, 1.0) as f64).label(""),
            rows[1],
        );
        f.render_widget(
            Paragraph::new(format!("Depth {:.0}%", depth * 100.0))
                .style(Style::default().fg(Color::White)),
            rows[2],
        );
        f.render_widget(
            Gauge::default().gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
                .ratio(depth as f64).label(""),
            rows[3],
        );
    }

    fn render_jog(&self, f: &mut Frame, area: Rect, params: &Arc<DeckParams>, color: Color) {
        let offset = DeckParams::load_f32(&params.jog_phase_offset);
        let vel    = DeckParams::load_f32(&params.jog_velocity);
        let fm_pct = (vel.abs() * 100.0) as u32;

        // Velocity indicator: arrow direction based on sign
        let vel_str = if vel > 0.01 {
            format!("▶ +{:.2}", vel)
        } else if vel < -0.01 {
            format!("◀ {:.2}", vel)
        } else {
            "  ──".to_string()
        };

        let vel_color = if vel.abs() > 0.01 { color } else { Color::DarkGray };

        let block = Block::default()
            .title(format!(" Jog  FM {}% ", fm_pct))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(10)])
            .split(inner);

        f.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(color).bg(Color::DarkGray))
                .ratio(offset as f64)
                .label("scrub"),
            cols[0],
        );
        f.render_widget(
            Paragraph::new(vel_str).style(Style::default().fg(vel_color)),
            cols[1],
        );
    }

    fn render_crossfader(&self, f: &mut Frame, area: Rect) {
        let xf    = self.shared.crossfader.load(Ordering::Relaxed);
        let vol   = self.shared.master_volume.load(Ordering::Relaxed);
        let vol_a = DeckParams::load_f32(&self.params_a.volume);
        let vol_b = DeckParams::load_f32(&self.params_b.volume);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Length(2)])
            .split(area);

        // Crossfader with A/B labels
        let xf_label = format!("A ◄ {:>3.0}% ► B", xf * 100.0);
        f.render_widget(
            Gauge::default()
                .block(Block::default().title(" Crossfader ").borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)))
                .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
                .ratio(xf as f64)
                .label(xf_label),
            rows[0],
        );

        // Volume row: Deck A vol | Master | Deck B vol
        let vol_row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(34),
                Constraint::Percentage(33),
            ])
            .split(rows[1]);

        f.render_widget(
            Gauge::default()
                .block(Block::default().title(" Vol A ").borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)))
                .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
                .ratio(vol_a as f64)
                .label(format!("{:.0}%", vol_a * 100.0)),
            vol_row[0],
        );
        f.render_widget(
            Gauge::default()
                .block(Block::default().title(" Master ").borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::White)))
                .gauge_style(Style::default().fg(Color::White).bg(Color::DarkGray))
                .ratio(vol as f64)
                .label(format!("{:.0}%", vol * 100.0)),
            vol_row[1],
        );
        f.render_widget(
            Gauge::default()
                .block(Block::default().title(" Vol B ").borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)))
                .gauge_style(Style::default().fg(Color::Magenta).bg(Color::DarkGray))
                .ratio(vol_b as f64)
                .label(format!("{:.0}%", vol_b * 100.0)),
            vol_row[2],
        );

        // MIDI device list in a small overlay bottom-right (if space allows)
        if !self.midi_devices.is_empty() {
            let devices_area = Rect {
                x: area.width.saturating_sub(30).min(area.x + area.width - 2),
                y: rows[0].y,
                width: 28,
                height: 2,
            };
            let names: Vec<ListItem> = self.midi_devices.iter().map(|d| {
                ListItem::new(format!("⬛ {d}")).style(Style::default().fg(Color::DarkGray))
            }).collect();
            f.render_widget(List::new(names)
                .block(Block::default().borders(Borders::NONE)), devices_area);
        }
    }
}
