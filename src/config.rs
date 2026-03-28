use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct MidiMapping {
    pub device: String,
    #[serde(rename = "type")]
    pub kind: String,   // "cc" | "note"
    pub channel: u8,    // 0 = any
    pub cc: Option<u8>,
    pub param: String,
    pub min: f32,
    pub max: f32,
    /// 0 = Deck A (default), 1 = Deck B
    pub deck: Option<u8>,
    /// true = relative CC (jog wheel): value is centered at 64, not absolute
    pub relative: Option<bool>,
    /// true = exponential curve (use for frequency/cutoff params)
    pub exp: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub mapping: Vec<MidiMapping>,
}

impl Config {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
}
