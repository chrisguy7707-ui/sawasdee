//! Thai speech through the computer's own voices, plus small sound effects.

#[allow(unused_imports)]
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

/// Recordings of every Thai word and sentence, made with a Thai voice and bundled in the app,
/// so speech works on every computer and phone (no system voice needed).
static VOICE_PACK: &[u8] = include_bytes!("../assets/audio/thai_voice.pack");

fn find_clip(text: &str) -> Option<&'static [u8]> {
    let id = crate::lesson::speech_id(text);
    let n = u32::from_le_bytes(VOICE_PACK.get(0..4)?.try_into().ok()?) as usize;
    let mut pos = 4usize;
    for _ in 0..n {
        let cid = u64::from_le_bytes(VOICE_PACK.get(pos..pos + 8)?.try_into().ok()?);
        let len = u32::from_le_bytes(VOICE_PACK.get(pos + 8..pos + 12)?.try_into().ok()?) as usize;
        let data = VOICE_PACK.get(pos + 12..pos + 12 + len)?;
        if cid == id {
            return Some(data);
        }
        pos += 12 + len;
    }
    None
}

/// The bundled recording decoded to mono samples, for pitch analysis.
pub fn clip_pcm(text: &str) -> Option<crate::pitch::Pcm> {
    use rodio::Source;
    let dec = rodio::Decoder::new(std::io::Cursor::new(find_clip(text)?)).ok()?;
    let rate: u32 = dec.sample_rate().into();
    let ch: usize = u16::from(dec.channels()) as usize;
    let all: Vec<f32> = dec.collect();
    let samples = if ch <= 1 { all } else { all.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect() };
    Some(crate::pitch::Pcm { samples, rate })
}

/// Whether a bundled recording exists for this text.
pub fn has_clip(text: &str) -> bool {
    find_clip(text).is_some()
}

/// True when the bundled recording of this text can be decoded.
pub fn clip_decodes(text: &str) -> bool {
    find_clip(text).map(|c| rodio::Decoder::new(std::io::Cursor::new(c)).is_ok()).unwrap_or(false)
}

pub struct Audio {
    /// 0 = still checking, 1 = system Thai voice available, 2 = not available.
    state: Arc<AtomicU8>,
    child: Option<Child>,
    /// Sound output for the bundled recordings (None if the device has no audio output).
    out: Option<rodio::MixerDeviceSink>,
    sink: Option<rodio::Player>,
    pub muted: bool,
}

impl Audio {
    pub fn new(muted: bool) -> Self {
        let state = Arc::new(AtomicU8::new(0));
        let s2 = state.clone();
        std::thread::spawn(move || {
            let ok = detect();
            s2.store(if ok { 1 } else { 2 }, Ordering::SeqCst);
        });
        // Don't open the sound device when muted (e.g. while taking test pictures).
        let out = if muted { None } else { rodio::DeviceSinkBuilder::open_default_sink().ok() };
        Audio { state, child: None, out, sink: None, muted }
    }

    /// True when speech is possible: the bundled recordings are always there.
    pub fn available(&self) -> bool {
        true
    }

    /// Read a Thai text aloud: the bundled recording first, the system voice as a backup.
    pub fn speak(&mut self, text: &str, slow: bool) {
        if self.muted {
            return;
        }
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        if let Some(clip) = find_clip(text) {
            if self.out.is_none() {
                self.out = rodio::DeviceSinkBuilder::open_default_sink().ok();
            }
            if let Some(out) = &self.out {
                if let Ok(dec) = rodio::Decoder::new(std::io::Cursor::new(clip)) {
                    let sink = rodio::Player::connect_new(out.mixer());
                    sink.set_speed(if slow { 0.75 } else { 1.0 });
                    sink.append(dec);
                    self.sink = Some(sink);
                    return;
                }
            }
        }
        if self.state.load(Ordering::SeqCst) != 2 {
            self.child = spawn_speech(text, slow);
        }
    }

    pub fn effect(&mut self, good: bool) {
        if self.muted {
            return;
        }
        #[cfg(target_os = "macos")]
        {
            let file = if good { "/System/Library/Sounds/Glass.aiff" } else { "/System/Library/Sounds/Basso.aiff" };
            let _ = Command::new("afplay").args(["-v", "0.5", file]).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = good;
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
        }
    }
}

#[cfg(target_os = "macos")]
fn detect() -> bool {
    Command::new("say")
        .args(["-v", "?"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("th_TH"))
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn spawn_speech(text: &str, slow: bool) -> Option<Child> {
    let rate = if slow { "110" } else { "165" };
    Command::new("say").args(["-v", "Kanya", "-r", rate, text]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()
}

#[cfg(target_os = "linux")]
fn detect() -> bool {
    Command::new("espeak-ng").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn spawn_speech(text: &str, slow: bool) -> Option<Child> {
    let rate = if slow { "100" } else { "150" };
    Command::new("espeak-ng").args(["-v", "th", "-s", rate, text]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()
}

#[cfg(target_os = "windows")]
fn detect() -> bool {
    let script = "Add-Type -AssemblyName System.Speech; (New-Object System.Speech.Synthesis.SpeechSynthesizer).GetInstalledVoices() | % { $_.VoiceInfo.Culture.Name }";
    Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("th-TH"))
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn spawn_speech(text: &str, slow: bool) -> Option<Child> {
    let rate = if slow { -4 } else { 0 };
    let safe = text.replace('\'', "");
    let script = format!(
        "Add-Type -AssemblyName System.Speech; $s=New-Object System.Speech.Synthesis.SpeechSynthesizer; $v=$s.GetInstalledVoices()|?{{$_.VoiceInfo.Culture.Name -eq 'th-TH'}}|select -First 1; if($v){{$s.SelectVoice($v.VoiceInfo.Name)}}; $s.Rate={rate}; $s.Speak('{safe}')"
    );
    Command::new("powershell").args(["-NoProfile", "-Command", &script]).stdout(Stdio::null()).stderr(Stdio::null()).spawn().ok()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn detect() -> bool {
    false
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn spawn_speech(_text: &str, _slow: bool) -> Option<Child> {
    None
}
