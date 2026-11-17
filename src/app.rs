//! The application: state, saving, and every screen.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::Datelike;
use eframe::egui::{
    self, vec2, Align, Align2, Color32, CornerRadius, CursorIcon, Key, Layout, Margin, Pos2, Rect, RichText, Sense,
    Stroke, StrokeKind, Ui, Vec2,
};
use serde::{Deserialize, Serialize};

use crate::audio::Audio;
use crate::data::{tone_of, CONSONANTS, LESSONS_PER_UNIT, TONES, TONE_RULES, UNITS, VOWELS};
use crate::kit::*;
use crate::notify;
use crate::lesson::{self, Body, Exercise, LessonKind, Rng};

/// Right answers in a row that take a question out of the mistake notebook.
const GRADUATE: u8 = 2;
/// Google Play build: units 1..=FREE_UNITS are free, the rest need the one-time purchase.
const FREE_UNITS: usize = 2;

/// The app's version (from the project settings).
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const STORE_KEY: &str = "progress_v1";

/// Where the voice-comparison practice is.
#[derive(Clone, Copy, PartialEq)]
enum RecUi {
    Idle,
    /// Recording since this time (seconds).
    Recording(f64),
    /// A recording of the learner exists and can be played.
    Have,
}

const BACKUP_HEADER: &str = "SAWASDEE-BACKUP-1";

fn today() -> i32 {
    chrono::Local::now().date_naive().num_days_from_ce()
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- saved progress

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Progress {
    pub done: Vec<u8>,
    pub xp_total: u32,
    pub xp_by_day: BTreeMap<i32, u32>,
    pub streak: u32,
    pub best_streak: u32,
    pub last_day: i32,
    pub daily_goal: u32,
    pub lessons_done: u32,
    pub perfect_lessons: u32,
    pub muted: bool,
    /// 0 = not chosen yet, 1 = male speech (ครับ/ผม), 2 = female speech (ค่ะ/ฉัน).
    pub gender: u8,
    /// Questions answered wrongly, kept until answered right twice in a row.
    pub notebook: Vec<Mistake>,
    /// Memory of each course word, keyed "unit-word".
    pub stats: BTreeMap<String, WordStat>,
    /// 0 = follow the computer, 1 = light, 2 = dark.
    pub theme: u8,
    /// Daily study reminder: on/off and the time, in minutes after midnight.
    pub reminder_on: bool,
    pub reminder_min: u32,
}

/// One entry of the mistake notebook: a word or a sentence answered wrongly.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Mistake {
    pub unit: usize,
    pub item: usize,
    pub sentence: bool,
    pub wrong: u32,
    /// Right answers in a row since the last mistake.
    pub streak: u8,
    pub last_day: i32,
}

/// How well a word is remembered (simple spaced repetition).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct WordStat {
    pub level: u8,
    pub due: i32,
    pub right: u32,
    pub wrong: u32,
}

/// Days until a word comes back, by memory level.
const INTERVALS: [i32; 7] = [0, 1, 2, 4, 7, 14, 30];

fn wkey(u: usize, i: usize) -> String {
    format!("{u}-{i}")
}

impl Default for Progress {
    fn default() -> Self {
        Progress {
            done: vec![0; UNITS.len()],
            xp_total: 0,
            xp_by_day: BTreeMap::new(),
            streak: 0,
            best_streak: 0,
            last_day: i32::MIN / 2,
            daily_goal: 20,
            lessons_done: 0,
            perfect_lessons: 0,
            muted: false,
            gender: 0,
            notebook: Vec::new(),
            stats: BTreeMap::new(),
            theme: 0,
            reminder_on: false,
            reminder_min: 20 * 60,
        }
    }
}

impl Progress {
    fn fix(&mut self) {
        self.done.resize(UNITS.len(), 0);
        for d in &mut self.done {
            *d = (*d).min(LESSONS_PER_UNIT as u8);
        }
        self.notebook.retain(|m| m.unit < UNITS.len() && if m.sentence { m.item < UNITS[m.unit].sentences.len() } else { m.item < UNITS[m.unit].words.len() });
    }

    pub fn streak_now(&self) -> u32 {
        let t = today();
        if self.last_day == t || self.last_day == t - 1 {
            self.streak
        } else {
            0
        }
    }

    fn xp_today(&self) -> u32 {
        *self.xp_by_day.get(&today()).unwrap_or(&0)
    }

    /// The unit and lesson the learner is on now (None when the course is finished).
    pub fn current(&self) -> Option<(usize, usize)> {
        self.done.iter().enumerate().find(|(_, d)| (**d as usize) < LESSONS_PER_UNIT).map(|(u, d)| (u, *d as usize))
    }

    fn words_learned(&self) -> usize {
        UNITS
            .iter()
            .enumerate()
            .map(|(u, unit)| {
                let d = self.done[u] as usize;
                if d >= 2 {
                    unit.words.len()
                } else if d == 1 {
                    5.min(unit.words.len())
                } else {
                    0
                }
            })
            .sum()
    }

    fn word_learned(&self, unit: usize, word: usize) -> bool {
        (self.done[unit] as usize) > word / 5
    }

    /// Start at a later level: earlier units count as done (moves forward only).
    fn skip_to(&mut self, first_unit: usize) {
        for u in 0..first_unit.min(UNITS.len()) {
            self.done[u] = LESSONS_PER_UNIT as u8;
        }
    }

    /// Put a wrongly answered word or sentence into the mistake notebook.
    fn note_wrong(&mut self, unit: usize, item: usize, sentence: bool) {
        let t = today();
        if let Some(m) = self.notebook.iter_mut().find(|m| m.unit == unit && m.item == item && m.sentence == sentence) {
            m.wrong += 1;
            m.streak = 0;
            m.last_day = t;
        } else {
            self.notebook.push(Mistake { unit, item, sentence, wrong: 1, streak: 0, last_day: t });
        }
    }

    /// A right answer: returns true when the entry graduates out of the notebook.
    fn note_right(&mut self, unit: usize, item: usize, sentence: bool) -> bool {
        if let Some(pos) = self.notebook.iter().position(|m| m.unit == unit && m.item == item && m.sentence == sentence) {
            let m = &mut self.notebook[pos];
            m.streak += 1;
            if m.streak >= GRADUATE {
                self.notebook.remove(pos);
                return true;
            }
        }
        false
    }

    /// Notebook entries, most needed first.
    fn notebook_order(&self) -> Vec<(usize, usize, bool)> {
        let mut list: Vec<&Mistake> = self.notebook.iter().collect();
        list.sort_by_key(|m| (m.streak, std::cmp::Reverse(m.wrong), m.last_day));
        list.into_iter().map(|m| (m.unit, m.item, m.sentence)).collect()
    }

    fn female(&self) -> bool {
        self.gender == 2
    }

    /// Record an answer for a word.
    fn record(&mut self, (u, i): (usize, usize), right: bool) {
        let t = today();
        let st = self.stats.entry(wkey(u, i)).or_insert(WordStat { level: 0, due: t, right: 0, wrong: 0 });
        if right {
            st.right += 1;
            if st.due <= t {
                st.level = (st.level + 1).min(INTERVALS.len() as u8 - 1);
                st.due = t + INTERVALS[st.level as usize];
            }
        } else {
            st.wrong += 1;
            st.level = 0;
            st.due = t;
        }
    }

    pub fn stat(&self, u: usize, i: usize) -> Option<&WordStat> {
        self.stats.get(&wkey(u, i))
    }

    /// Learned words in the order they most need practice.
    fn review_order(&self) -> Vec<(usize, usize)> {
        let t = today();
        let mut list: Vec<((bool, u8, i64), (usize, usize))> = Vec::new();
        for (u, unit) in UNITS.iter().enumerate() {
            for i in 0..unit.words.len() {
                if !self.word_learned(u, i) {
                    continue;
                }
                let (due, level, wrong) = match self.stat(u, i) {
                    Some(s) => (s.due <= t, s.level, s.wrong as i64),
                    None => (true, 0, 0),
                };
                list.push(((!due, level, -wrong), (u, i)));
            }
        }
        list.sort_by_key(|(k, _)| *k);
        list.into_iter().map(|(_, w)| w).collect()
    }

    /// How many learned words are due for review today.
    fn due_count(&self) -> usize {
        let t = today();
        UNITS
            .iter()
            .enumerate()
            .flat_map(|(u, unit)| (0..unit.words.len()).map(move |i| (u, i)))
            .filter(|&(u, i)| self.word_learned(u, i) && self.stat(u, i).map(|s| s.due <= t).unwrap_or(true))
            .count()
    }
}

// ---------------------------------------------------------------- lesson run

#[derive(Default)]
struct ExState {
    selected: Option<usize>,
    placed: Vec<usize>,
    m_left: Option<usize>,
    m_right: Option<usize>,
    matched_left: Vec<bool>,
    matched_right: Vec<bool>,
    wrong_pair: Option<(usize, usize, f64)>,
    match_wrong: usize,
    match_bad: Vec<usize>,
    spoke: bool,
    /// Flip card shows its meaning.
    flipped: bool,
}

struct Run {
    kind: LessonKind,
    queue: Vec<(Exercise, bool)>,
    pos: usize,
    first_total: usize,
    first_right: usize,
    mistakes: usize,
    combo: u32,
    st: ExState,
    feedback: Option<bool>,
    started: f64,
    /// Solutions of the questions missed in this lesson, for the summary.
    missed: Vec<String>,
    confirm_quit: bool,
    /// Shown after a right answer that differs from the main one.
    note: Option<String>,
    /// Notebook entries that graduated during this lesson.
    graduated: usize,
}

impl Run {
    fn ex(&self) -> &Exercise {
        &self.queue[self.pos].0
    }
}

struct Summary {
    xp: u32,
    accuracy: u32,
    secs: u32,
    practice: bool,
    review: bool,
    streak_up: Option<u32>,
    unit_finished: Option<usize>,
    /// Solutions of questions missed in this lesson.
    missed: Vec<String>,
    mistake_review: bool,
    /// Notebook entries that graduated in this lesson.
    graduated: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Learn,
    Letters,
    Words,
    Notebook,
    Profile,
}

struct Shot {
    path: PathBuf,
    frames: u32,
    requested: bool,
    done: Arc<AtomicBool>,
}

pub struct App {
    p: Progress,
    tab: Tab,
    /// Letters tab page: 0 consonants, 1 vowels, 2 tones.
    letters_page: u8,
    /// Choices on the first-run welcome screen.
    welcome_gender: u8,
    welcome_level: usize,
    run: Option<Run>,
    summary: Option<Summary>,
    audio: Audio,
    rng: Rng,
    shot: Option<Shot>,
    toast: Option<(String, f64)>,
    confirm_reset: bool,
    /// Learning-record backup: text typed or pasted on computers, and a backup waiting for "overwrite?".
    backup_input: String,
    /// "Compare my voice" practice on the listen-and-repeat card.
    rec: RecUi,
    rec_key: usize,
    back_polled: f64,
    /// Opened on a demo screen: nothing is saved.
    demo: bool,
    scale_polled: f64,
    zoom: f32,
    /// Pitch shapes (semitones, 60 points) of the native clip and of the learner's recording.
    rec_native: Vec<f32>,
    rec_mine: Vec<f32>,
    backup_pending: Option<Progress>,
    confirm_exit: bool,
    /// Google Play build: the full unlock has been bought (asked from Play at every start)
    owned: bool,
    show_paywall: bool,
    paywall_demo: bool,
    buy_state: i32,
    buy_price: String,
    buy_poll: f64,
    theme_applied: bool,
    /// The opening (cover) screen, shown until the learner taps start.
    splash: bool,
    splash_t0: Option<f64>,
    /// Test switch (`--open phone-...`): draw the phone layout in a big window.
    force_narrow: bool,
    frames: u32,
    /// Draws only the app icon (used once to make the Android launcher icon).
    icon_mode: bool,
}

// ---------------------------------------------------------------- fonts

/// Fonts are bundled inside the app, so Thai and Korean look the same on every
/// computer and phone (no system fonts needed).
static KO_REGULAR: &[u8] = include_bytes!("../assets/fonts/NotoSansKR-Regular.otf");
static KO_BOLD: &[u8] = include_bytes!("../assets/fonts/NotoSansKR-Bold.otf");
static TH_VARIABLE: &[u8] = include_bytes!("../assets/fonts/NotoSansThai-Variable.ttf");

fn setup_fonts(ctx: &egui::Context) {
    let mut defs = egui::FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8], weight: Option<f32>| {
        let mut fd = egui::FontData::from_static(bytes);
        if let Some(w) = weight {
            fd.tweak.coords = egui::epaint::text::VariationCoords::new([(b"wght", w)]);
        }
        defs.font_data.insert(name.to_string(), Arc::new(fd));
    };
    add("th", TH_VARIABLE, Some(450.0));
    add("th_b", TH_VARIABLE, Some(700.0));
    add("ko", KO_REGULAR, None);
    add("ko_b", KO_BOLD, None);
    let defaults = defs.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
    let mut prop = vec!["th".to_string(), "ko".to_string()];
    prop.extend(defaults.iter().cloned());
    let mut b = vec!["th_b".to_string(), "ko_b".to_string(), "th".to_string(), "ko".to_string()];
    b.extend(defaults.iter().cloned());
    defs.families.insert(egui::FontFamily::Proportional, prop);
    defs.families.insert(egui::FontFamily::Name("bold".into()), b);
    if let Some(mono) = defs.families.get_mut(&egui::FontFamily::Monospace) {
        mono.extend(["th".to_string(), "ko".to_string()]);
    }
    ctx.set_fonts(defs);
}

/// Apply the light or dark colours to egui's own widgets (tooltips, scroll bars...).
fn apply_theme(ctx: &egui::Context, dark: bool) {
    set_dark(dark);
    let theme = if dark { egui::Theme::Dark } else { egui::Theme::Light };
    ctx.set_theme(theme);
    ctx.style_mut_of(theme, |s| {
        s.visuals.panel_fill = bg();
        s.visuals.window_fill = surface();
        s.visuals.window_stroke = Stroke::new(1.0, border());
        s.visuals.override_text_color = Some(fg());
        s.interaction.selectable_labels = false;
        s.spacing.item_spacing = vec2(8.0, 8.0);
        s.spacing.scroll.bar_width = 8.0;
        s.text_styles.insert(egui::TextStyle::Body, font(15.0));
        s.text_styles.insert(egui::TextStyle::Button, font(15.0));
    });
}

// ---------------------------------------------------------------- app

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, shot: Option<PathBuf>, open: Option<String>) -> Self {
        setup_fonts(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx, false);
        let mut p: Progress = if shot.is_none() {
            cc.storage.and_then(|s| eframe::get_value(s, STORE_KEY)).unwrap_or_default()
        } else {
            Progress::default()
        };
        p.fix();
        let seed = if shot.is_some() { 7 } else { now_secs() as u64 };
        let muted = p.muted || shot.is_some();
        let mut app = App {
            p,
            tab: Tab::Learn,
            letters_page: 0,
            welcome_gender: 0,
            welcome_level: 0,
            run: None,
            summary: None,
            audio: Audio::new(muted),
            rng: Rng::new(seed),
            shot: shot.map(|path| Shot { path, frames: 0, requested: false, done: Arc::new(AtomicBool::new(false)) }),
            toast: None,
            confirm_reset: false,
            backup_input: String::new(),
            rec: RecUi::Idle,
            rec_key: usize::MAX,
            back_polled: 0.0,
            demo: false,
            scale_polled: -10.0,
            zoom: -1.0, // unknown yet: the first check sets it (a zoom saved by an older session is not trusted)
            rec_native: Vec::new(),
            rec_mine: Vec::new(),
            backup_pending: None,
            confirm_exit: false,
            owned: false,
            show_paywall: false,
            paywall_demo: false,
            buy_state: 0,
            buy_price: String::new(),
            buy_poll: -10.0,
            theme_applied: false,
            splash: false,
            splash_t0: None,
            frames: 0,
            icon_mode: open.as_deref() == Some("icon"),
            force_narrow: open.as_deref().map(|o| o.starts_with("phone")).unwrap_or(false),
        };
        app.splash = (app.shot.is_none() && open.is_none()) || open.as_deref() == Some("splash");
        // Screenshots show the app as set up, unless the welcome screen is asked for.
        app.demo = open.is_some();
        if notify::paywall_enabled() && open.is_none() {
            notify::billing_init();
        }
        if app.demo {
            // start from the top of every list, not where the last real session was scrolled to
            cc.egui_ctx.memory_mut(|m| m.data.clear());
        }
        if (app.shot.is_some() || app.demo) && open.as_deref() != Some("welcome") && app.p.gender == 0 {
            app.p.gender = 1;
        }
        // Demo states, used for checking screens.
        if let Some(o) = open.as_deref() {
            // "light:<screen>" draws the screen in the light theme (test pictures)
            let o = match o.strip_prefix("light:") {
                Some(rest) => {
                    app.p.theme = 1;
                    rest
                }
                None => o,
            };
            match o {
                "letters" | "phone-letters" => app.tab = Tab::Letters,
                "dark" => app.p.theme = 2,
                "dark-lesson" => {
                    app.p.theme = 2;
                    app.start(LessonKind::Normal { unit: 0, lesson: 0 }, 0.0);
                    if let Some(run) = app.run.as_mut() {
                        run.pos = 8.min(run.queue.len() - 1);
                        run.st.placed = vec![0, 1];
                    }
                }
                "letters-more" => {
                    app.tab = Tab::Letters;
                    app.letters_page = 1;
                }
                "vowels" => {
                    app.tab = Tab::Letters;
                    app.letters_page = 2;
                }
                "finals" => {
                    app.tab = Tab::Letters;
                    app.letters_page = 3;
                }
                "tones" => {
                    app.tab = Tab::Letters;
                    app.letters_page = 4;
                }
                "tone-rules" => {
                    app.tab = Tab::Letters;
                    app.letters_page = 5;
                }
                "specials" => {
                    app.tab = Tab::Letters;
                    app.letters_page = 6;
                }
                "digits" => {
                    app.tab = Tab::Letters;
                    app.letters_page = 7;
                }
                "words" => {
                    app.tab = Tab::Words;
                    for u in 0..3 {
                        app.p.done[u] = 5;
                    }
                }
                "profile" => app.tab = Tab::Profile,
                // the learning path with a little progress, for store pictures
                "learn-demo" => {
                    app.p = Progress { gender: app.p.gender, theme: app.p.theme, ..Progress::default() };
                    app.p.fix();
                    app.p.done[0] = 5;
                    app.p.done[1] = 2;
                    app.p.streak = 4;
                    app.p.best_streak = 4;
                    app.p.last_day = today();
                    app.p.xp_total = 180;
                    app.p.xp_by_day.insert(today(), 15);
                    app.p.lessons_done = 7;
                }
                // the purchase offer on the path (units 1-2 done), to check the Google Play build's screens
                "paywall-demo" => {
                    app.p.done[0] = 5;
                    app.p.done[1] = 5;
                    app.p.last_day = today();
                    app.paywall_demo = true;
                    app.show_paywall = true;
                    app.buy_state = 1;
                    app.buy_price = "₩9,900".to_string();
                }
                "notebook" => {
                    app.tab = Tab::Notebook;
                    app.p.done[0] = 2;
                    for (u, i, sen) in [(0usize, 6usize, false), (0, 3, true), (0, 1, false), (0, 8, false)] {
                        app.p.note_wrong(u, i, sen);
                    }
                    app.p.note_wrong(0, 6, false);
                    app.p.note_right(0, 1, false);
                }
                // test pictures of the alphabet quizzes: "q-<kind>" or "q-<kind>@<question number>"
                q if q.starts_with("q-") => {
                    let (name, at) = match q[2..].split_once('@') {
                        Some((n, i)) => (n, i.parse::<usize>().unwrap_or(0)),
                        None => (&q[2..], 0),
                    };
                    let kind = match name {
                        "letters" => Some(LessonKind::Letters),
                        "vowels" => Some(LessonKind::Vowels),
                        "spelling" => Some(LessonKind::Spelling),
                        "class" => Some(LessonKind::ConsonantClass),
                        "tonerules" => Some(LessonKind::ToneRules),
                        "finals" => Some(LessonKind::Finals),
                        "reading" => Some(LessonKind::Reading),
                        "sentence" => Some(LessonKind::SentenceReading),
                        "signs" => Some(LessonKind::Signs),
                        "digits" => Some(LessonKind::Digits),
                        "practice" => Some(LessonKind::Practice),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        for u in 0..UNITS.len() {
                            app.p.done[u] = 5;
                        }
                        app.start(kind, 0.0);
                        if let Some(run) = app.run.as_mut() {
                            run.pos = at.min(run.queue.len().saturating_sub(1));
                        }
                    }
                }
                "dialogue" => {
                    for u in 0..3 {
                        app.p.done[u] = 5;
                    }
                    app.start(LessonKind::Dialogue, 0.0);
                }
                "lesson" | "tiles" | "match" | "feedback" | "tone" | "flash" | "flash-back" | "spell" | "fill" | "shadow" | "wrong" => {
                    app.start(if o == "fill" { LessonKind::Normal { unit: 1, lesson: 2 } } else { LessonKind::Normal { unit: 0, lesson: if o == "shadow" { 1 } else { 0 } } }, 0.0);
                    if let Some(run) = app.run.as_mut() {
                        let want = match o {
                            "tiles" => 13,
                            "match" => 11,
                            "tone" => 9,
                            "feedback" | "wrong" => 5,
                            "spell" => 10,
                            "fill" => 5,
                            "shadow" => 13,
                            _ => 0,
                        };
                        run.pos = want.min(run.queue.len() - 1);
                        if o == "flash-back" {
                            run.st.flipped = true;
                        }
                        if o == "spell" {
                            run.st.placed = vec![0];
                        }
                        if o == "tiles" {
                            run.st.placed = vec![0, 1];
                        }
                        if o == "wrong" {
                            run.st.selected = Some(0);
                            run.feedback = Some(false);
                            run.note = None;
                        }
                        if o == "feedback" {
                            run.st.selected = Some(0);
                            run.feedback = Some(matches!(run.ex().body, Body::Choice { correct: 0, .. }));
                        }
                    }
                }
                "summary" => {
                    app.summary = Some(Summary {
                        xp: 15,
                        accuracy: 80,
                        secs: 154,
                        practice: false,
                        review: false,
                        streak_up: Some(1),
                        unit_finished: None,
                        missed: vec!["ขอโทษ = 죄송합니다".into(), "สวัสดีครับ (싸왓디 크랍)".into()],
                        mistake_review: false,
                        graduated: 0,
                    })
                }
                _ => {}
            }
        }
        if app.shot.is_none() && app.p.reminder_on {
            // Opening the app re-books the next 7 days (and skips today if already studied).
            app.sync_reminder();
        }
        app
    }

    /// Units after the free ones stay closed in the Google Play build until the full unlock is bought.
    fn paywall_locks(&self, unit: usize) -> bool {
        notify::paywall_enabled() && (!self.demo || self.paywall_demo) && !self.owned && unit >= FREE_UNITS
    }

    /// Asks the Java side about the purchase twice a second (cheap) and shows its messages.
    fn poll_billing(&mut self, ctx: &egui::Context, now: f64) {
        if !notify::paywall_enabled() || self.paywall_demo || now - self.buy_poll < 0.5 {
            return;
        }
        self.buy_poll = now;
        self.buy_state = notify::billing_state();
        self.owned = self.buy_state == 2;
        if self.buy_price.is_empty() {
            self.buy_price = notify::billing_price();
        }
        let m = notify::billing_message();
        if !m.is_empty() {
            self.toast(&m, now);
            self.show_paywall = false;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

    /// The window that offers the full unlock.
    fn paywall_window(&mut self, ctx: &egui::Context, now: f64) {
        if !self.show_paywall || self.owned {
            return;
        }
        let mut close = false;
        egui::Window::new("paywall")
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .fixed_size(vec2(ctx.content_rect().width().min(380.0) - 24.0, 0.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(6.0);
                    ui.label(RichText::new("전체 단원 열기").font(bold(22.0)));
                });
                ui.add_space(6.0);
                ui.label(RichText::new(format!("단원 1~{}은 무료예요. 한 번만 결제하면 나머지 {}개 단원이 모두 열려요. 구독이 아니에요.", FREE_UNITS, UNITS.len() - FREE_UNITS)).font(font(15.0)));
                ui.add_space(4.0);
                ui.label(RichText::new("같은 구글 계정이면 앱을 다시 설치하거나 새 폰에서도 '구매 복원'으로 다시 열 수 있어요.").font(font(13.5)).color(muted()));
                ui.add_space(10.0);
                let label = match (self.buy_state, self.buy_price.is_empty()) {
                    (3, _) => "결제 처리 중이에요".to_string(),
                    (1, false) => format!("{} 결제하고 열기", self.buy_price),
                    _ => "결제 정보를 불러오는 중…".to_string(),
                };
                if button3d(ui, &label, vec2(ui.available_width(), 52.0), PRIMARY, self.buy_state == 1).clicked() {
                    notify::billing_buy();
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if outline_button(ui, "구매 복원", vec2(140.0, 44.0), fg()).clicked() {
                        notify::billing_refresh();
                        self.toast("구글 플레이에서 구매 내역을 확인하고 있어요", now);
                    }
                    if outline_button(ui, "닫기", vec2(100.0, 44.0), fg()).clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.show_paywall = false;
        }
    }

    fn toast(&mut self, text: &str, now: f64) {
        self.toast = Some((text.to_string(), now));
    }

    fn build(&mut self, kind: LessonKind) -> Vec<Exercise> {
        let focus = self.p.review_order();
        let mistakes = self.p.notebook_order();
        let ctx = lesson::Ctx { tts: self.audio.available(), female: self.p.female(), focus: &focus, mistakes: &mistakes, done: &self.p.done };
        lesson::build(kind, &mut self.rng, &ctx)
    }

    fn start(&mut self, kind: LessonKind, now: f64) {
        let ex = self.build(kind);
        let n = ex.len();
        self.run = Some(Run {
            kind,
            queue: ex.into_iter().map(|e| (e, false)).collect(),
            pos: 0,
            first_total: n,
            first_right: 0,
            mistakes: 0,
            combo: 0,
            st: ExState::default(),
            feedback: None,
            started: now,
            missed: Vec::new(),
            graduated: 0,
            confirm_quit: false,
            note: None,
        });
        self.summary = None;
    }

    fn try_start(&mut self, unit: usize, lesson: usize, now: f64) {
        self.start(LessonKind::Normal { unit, lesson }, now);
    }

    fn finish(&mut self, run: Run, now: f64) {
        let t = today();
        let (xp, practice, review) = match run.kind {
            LessonKind::Practice | LessonKind::Mistakes | LessonKind::Letters | LessonKind::Vowels | LessonKind::Spelling | LessonKind::ConsonantClass | LessonKind::ToneRules | LessonKind::Finals | LessonKind::Reading | LessonKind::SentenceReading | LessonKind::Signs | LessonKind::Digits | LessonKind::Dialogue => (5 + run.first_right.min(5) as u32, true, false),
            LessonKind::Normal { lesson, .. } => {
                let base = if lesson == LESSONS_PER_UNIT - 1 { 20 } else { 10 };
                (base + if run.mistakes == 0 { 5 } else { 0 }, false, lesson == LESSONS_PER_UNIT - 1)
            }
        };
        let mut unit_finished = None;
        if let LessonKind::Normal { unit, lesson } = run.kind {
            if lesson == self.p.done[unit] as usize {
                self.p.done[unit] += 1;
                if self.p.done[unit] as usize == LESSONS_PER_UNIT {
                    unit_finished = Some(unit);
                }
            }
        }
        let streak_up = if self.p.last_day == t {
            None
        } else {
            self.p.streak = if self.p.last_day == t - 1 { self.p.streak + 1 } else { 1 };
            self.p.last_day = t;
            self.p.best_streak = self.p.best_streak.max(self.p.streak);
            Some(self.p.streak)
        };
        if streak_up.is_some() && self.p.reminder_on {
            // Studied today: move today's reminder out of the way.
            notify::schedule(self.p.reminder_min, true);
        }
        self.p.xp_total += xp;
        *self.p.xp_by_day.entry(t).or_insert(0) += xp;
        self.p.lessons_done += 1;
        if run.mistakes == 0 && !practice {
            self.p.perfect_lessons += 1;
        }
        let accuracy = if run.first_total == 0 { 100 } else { (run.first_right * 100 / run.first_total) as u32 };
        self.summary = Some(Summary {
            xp,
            accuracy,
            secs: (now - run.started).max(0.0) as u32,
            practice,
            review,
            streak_up,
            unit_finished,
            missed: run.missed.clone(),
            mistake_review: run.kind == LessonKind::Mistakes,
            graduated: run.graduated,
        });
        self.audio.effect(true);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        // light / dark
        let want_dark = match self.p.theme {
            1 => false,
            2 => true,
            _ => ctx.system_theme() == Some(egui::Theme::Dark),
        };
        if want_dark != is_dark() || !self.theme_applied {
            apply_theme(&ctx, want_dark);
            self.theme_applied = true;
        }
        self.handle_shot(&ctx);
        self.poll_billing(&ctx, now);
        // Make sure the window really is the portrait (phone) size at the start.
        self.frames += 1;
        if self.frames <= 3 {
            log::info!("frame {} size {:?}", self.frames, ctx.content_rect());
        }
        if self.frames == 2 {
            let args: Vec<String> = std::env::args().collect();
            let size = args
                .iter()
                .position(|a| a == "--size")
                .and_then(|i| args.get(i + 1))
                .and_then(|v| {
                    let (w, h) = v.split_once('x')?;
                    Some(vec2(w.parse().ok()?, h.parse().ok()?))
                })
                .unwrap_or(vec2(430.0, 880.0));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }

        // Phones: keep clear of the status bar (top) and the gesture bar (bottom).
        #[cfg(target_os = "android")]
        {
            let band = if self.splash || !is_dark() { Color32::from_rgb(38, 34, 112) } else { bg() };
            egui::Panel::top("android_top").exact_size(38.0).show_separator_line(false).frame(egui::Frame::NONE.fill(band)).show(ui, |_| {});
            egui::Panel::bottom("android_bottom").exact_size(16.0).show_separator_line(false).frame(egui::Frame::NONE.fill(band)).show(ui, |_| {});
        }
        // iPhone: keep clear of the notch / Dynamic Island (top) and the home bar (bottom).
        #[cfg(target_os = "ios")]
        {
            let (top, bottom) = crate::safe_insets();
            let band = if self.splash { Color32::from_rgb(38, 34, 112) } else { bg() };
            egui::Panel::top("ios_top").exact_size(top).show_separator_line(false).frame(egui::Frame::NONE.fill(band)).show(ui, |_| {});
            egui::Panel::bottom("ios_bottom").exact_size(bottom).show_separator_line(false).frame(egui::Frame::NONE.fill(bg())).show(ui, |_| {});
        }
        // follow the phone's text size setting by enlarging the whole picture (capped, so layouts still fit)
        {
            let t = ui.input(|i| i.time);
            if t - self.scale_polled > 1.5 {
                self.scale_polled = t;
                // never make the picture so large that the screen is narrower than ~335 dp (the card layouts stay clean down to there)
                let width_now = ui.ctx().content_rect().width() * ui.ctx().zoom_factor();
                let cap = (width_now / 335.0).clamp(1.0, 1.3);
                let want = notify::font_scale().clamp(1.0, cap);
                if (want - self.zoom).abs() > 0.01 && self.shot.is_none() {
                    self.zoom = want;
                    ui.ctx().set_zoom_factor(want);
                }
            }
        }
        // the Back button / gesture (Android): step back one level, and exit from the first screen
        let t_now = ui.input(|i| i.time);
        if t_now - self.back_polled > 0.06 {
            self.back_polled = t_now;
            if notify::back_pressed() {
                if let Some(run) = self.run.as_mut() {
                    run.confirm_quit = !run.confirm_quit;
                } else if self.summary.is_some() {
                    self.summary = None;
                } else if self.splash || self.p.gender == 0 {
                    notify::exit_app();
                } else if self.tab != Tab::Learn {
                    self.tab = Tab::Learn;
                } else {
                    notify::exit_app();
                }
            }
        }
        // never keep the microphone open when the app goes to the background
        if matches!(self.rec, RecUi::Recording(_)) && !ui.ctx().input(|i| i.focused) {
            notify::rec_clear();
            self.rec = RecUi::Idle;
            self.rec_key = usize::MAX;
        }
        // the learner's own recording is thrown away as soon as the lesson is left
        if self.run.is_none() && self.rec != RecUi::Idle {
            notify::rec_clear();
            self.rec = RecUi::Idle;
            self.rec_key = usize::MAX;
        }
        if self.icon_mode {
            egui::CentralPanel::no_frame().frame(egui::Frame::NONE.fill(Color32::from_rgb(20, 170, 150))).show(ui, |ui| {
                let r = ui.max_rect();
                let p = ui.painter();
                let c = r.center();
                p.circle_filled(c + vec2(0.0, r.width() * 0.62), r.width() * 0.5, Color32::from_rgb(14, 130, 115));
                paint_mascot(p, c + vec2(0.0, -r.width() * 0.04), r.width() * 0.2, true);
            });
        } else if self.splash {
            self.splash_screen(ui, now);
        } else if self.p.gender == 0 {
            self.welcome_screen(ui);
        } else if self.run.is_some() {
            self.lesson_screen(ui, now);
        } else if self.summary.is_some() {
            self.summary_screen(ui, now);
        } else {
            self.main_screen(ui, now);
        }
        self.paint_toast(&ctx, now);
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // demo screens (test and store pictures) must never overwrite the learner's real record
        if self.shot.is_none() && !self.demo {
            self.p.muted = self.audio.muted;
            eframe::set_value(storage, STORE_KEY, &self.p);
        }
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        std::time::Duration::from_secs(5)
    }

    fn persist_egui_memory(&self) -> bool {
        self.shot.is_none() && !self.demo
    }

    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        bg().to_normalized_gamma_f32()
    }
}

// ---------------------------------------------------------------- screenshot

impl App {
    fn handle_shot(&mut self, ctx: &egui::Context) {
        let Some(shot) = self.shot.as_mut() else { return };
        shot.frames += 1;
        ctx.request_repaint();
        if shot.done.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if shot.frames >= 120 && (!shot.requested || shot.frames % 60 == 0) {
            shot.requested = true;
            let path = shot.path.clone();
            let done = shot.done.clone();
            let ctx2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                let [w, h] = img.size;
                let mut bytes = Vec::with_capacity(w * h * 4);
                for px in &img.pixels {
                    bytes.extend_from_slice(&px.to_srgba_unmultiplied());
                }
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                    match buf.save(&path) {
                        Ok(()) => eprintln!("screenshot saved to {}", path.display()),
                        Err(e) => eprintln!("screenshot failed: {e}"),
                    }
                }
                done.store(true, Ordering::SeqCst);
                ctx2.request_repaint();
            });
        }
        if shot.frames > 1200 {
            eprintln!("screenshot timed out");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn paint_toast(&mut self, ctx: &egui::Context, now: f64) {
        self.paywall_window(ctx, now);
        let Some((text, t0)) = self.toast.clone() else { return };
        let age = now - t0;
        if age > 3.0 {
            self.toast = None;
            return;
        }
        let alpha = ((3.0 - age).min(0.3) / 0.3) as f32;
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("toast")));
        let screen = ctx.content_rect();
        let g = p.layout_no_wrap(text, bold(15.0), WHITE);
        let size = g.size() + vec2(36.0, 22.0);
        let rect = Rect::from_center_size(Pos2::new(screen.center().x, screen.bottom() - 60.0), size);
        p.rect_filled(rect, CornerRadius::same(14), Color32::from_rgba_unmultiplied(50, 55, 65, (235.0 * alpha) as u8));
        p.galley(rect.center() - g.size() / 2.0, g, WHITE);
    }
}

// ---------------------------------------------------------------- main layout

impl App {
    /// Phone layout: a slim top bar with the stats and a tab bar at the bottom.
    fn mobile_bars(&mut self, ui: &mut Ui) {
        egui::Panel::top("m_top")
            .exact_size(52.0)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(surface()).inner_margin(Margin::symmetric(12, 6)))
            .show(ui, |ui| {
                self.stat_row(ui);
            });
        egui::Panel::bottom("m_tabs")
            .exact_size(64.0)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(surface()))
            .show(ui, |ui| {
                let full = ui.max_rect();
                ui.painter().line_segment([full.left_top(), full.right_top()], Stroke::new(1.0, border()));
                let tabs = [(Tab::Learn, "학습"), (Tab::Letters, "알파벳"), (Tab::Words, "단어장"), (Tab::Notebook, "오답"), (Tab::Profile, "내 정보")];
                let w = full.width() / tabs.len() as f32;
                for (k, (tab, label)) in tabs.iter().enumerate() {
                    let r = Rect::from_min_size(Pos2::new(full.left() + w * k as f32, full.top() + 2.0), vec2(w, full.height() - 2.0));
                    let resp = ui.interact(r, ui.scope_id().with(("mtab", k)), Sense::click());
                    let sel = self.tab == *tab;
                    let p = ui.painter();
                    let col = if sel { PRIMARY } else { muted() };
                    let ic = Pos2::new(r.center().x, r.top() + 20.0);
                    if sel {
                        p.rect_filled(Rect::from_center_size(ic, vec2(52.0, 30.0)), CornerRadius::same(15), primary_soft());
                    }
                    match tab {
                        Tab::Learn => paint_star(p, ic, 11.0, if sel { GOLD } else { col }),
                        Tab::Letters => {
                            p.text(ic, Align2::CENTER_CENTER, "ก", bold(20.0), col);
                        }
                        Tab::Words => {
                            p.text(ic, Align2::CENTER_CENTER, "가", bold(18.0), col);
                        }
                        Tab::Notebook => paint_note(p, ic, 20.0, if sel { RED } else { col }),
                        Tab::Profile => {
                            p.circle_filled(ic + vec2(0.0, -4.0), 5.0, col);
                            p.circle_filled(ic + vec2(0.0, 9.0), 8.0, col);
                            p.rect_filled(Rect::from_min_size(ic + vec2(-10.0, 9.0), vec2(20.0, 10.0)), CornerRadius::ZERO, surface());
                        }
                    }
                    p.text(Pos2::new(r.center().x, r.bottom() - 12.0), Align2::CENTER_CENTER, *label, if sel { bold(11.5) } else { font(11.5) }, col);
                    if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                        self.tab = *tab;
                    }
                }
            });
    }

    /// Compact home cards for the phone layout: daily goal, review, mistake notebook.
    fn mobile_cards(&mut self, ui: &mut Ui, now: f64, w: f32) {
        let xp = self.p.xp_today();
        let goal = self.p.daily_goal;
        card_frame().inner_margin(Margin::symmetric(14, 12)).show(ui, |ui| {
            ui.set_width(w - 32.0);
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                paint_bolt(ui.painter(), r.center(), 30.0, GOLD);
                ui.vertical(|ui| {
                    ui.label(RichText::new(format!("오늘의 목표 · XP {} / {}", xp.min(goal), goal)).font(bold(15.0)));
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), Sense::hover());
                    progress_bar(ui.painter(), r, xp as f32 / goal as f32, GOLD);
                });
            });
        });
        ui.add_space(8.0);
        let due = self.p.due_count();
        let n = self.p.notebook.len();
        let half = (w - 8.0) / 2.0;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if self.p.words_learned() > 0 {
                let label = if due > 0 { format!("약한 단어 복습 ({})", due) } else { "복습하기".to_string() };
                if button3d(ui, &label, vec2(half, 46.0), BLUE, true).clicked() {
                    self.start(LessonKind::Practice, now);
                }
            }
            if n > 0 {
                if button3d(ui, &format!("오답 노트 ({})", n), vec2(half, 46.0), RED, true).clicked() {
                    self.start(LessonKind::Mistakes, now);
                }
            }
        });
        ui.add_space(4.0);
    }

    /// The cover screen.
    fn splash_screen(&mut self, ui: &mut Ui, now: f64) {
        let t0 = *self.splash_t0.get_or_insert(now);
        let t = (now - t0) as f32;
        let mut go = false;
        egui::CentralPanel::no_frame().frame(egui::Frame::NONE.fill(Color32::from_rgb(38, 34, 112))).show(ui, |ui| {
            let r = ui.max_rect();
            let p = ui.painter().clone();
            let p = &p;
            // soft background circles
            p.circle_filled(r.left_top() + vec2(r.width() * 0.15, r.height() * 0.2), r.width().min(r.height()) * 0.35, Color32::from_rgba_unmultiplied(255, 255, 255, 10));
            p.circle_filled(r.right_bottom() - vec2(r.width() * 0.1, r.height() * 0.15), r.width().min(r.height()) * 0.45, Color32::from_rgba_unmultiplied(255, 255, 255, 12));
            // floating Thai letters
            for (k, ch) in ["ก", "ข", "ส", "ม", "ว", "ด", "น", "ร"].iter().enumerate() {
                let fx = (k as f32 * 0.137 + 0.08) % 1.0;
                let bob = (t * 0.8 + k as f32 * 1.3).sin() * 10.0;
                let y = r.top() + r.height() * (0.1 + 0.8 * ((k * 5 % 8) as f32 / 8.0)) + bob;
                p.text(Pos2::new(r.left() + r.width() * fx, y), Align2::CENTER_CENTER, *ch, bold(46.0), Color32::from_rgba_unmultiplied(255, 255, 255, 22));
            }
            let cx = r.center().x;
            let mut y = r.top() + r.height() * 0.5 - 190.0;
            if y < r.top() + 10.0 {
                y = r.top() + 10.0;
            }
            let bounce = (t * 2.2).sin() * 5.0;
            paint_mascot(p, Pos2::new(cx, y + 90.0 + bounce), 70.0, true);
            p.text(Pos2::new(cx, y + 215.0), Align2::CENTER_CENTER, "สวัสดี", bold(44.0), Color32::from_rgb(255, 224, 130));
            p.text(Pos2::new(cx, y + 268.0), Align2::CENTER_CENTER, "사왓디", bold(54.0), WHITE);
            p.text(Pos2::new(cx, y + 314.0), Align2::CENTER_CENTER, "한국인을 위한 태국어", font(18.0), Color32::from_white_alpha(220));
            let bw = 320.0f32.min(r.width() - 60.0);
            let btn = Rect::from_center_size(Pos2::new(cx, y + 392.0), vec2(bw, 56.0));
            let mut ui_btn = ui.new_child(egui::UiBuilder::new().max_rect(btn));
            if button3d(&mut ui_btn, "시작하기", vec2(bw, 56.0), Color32::from_rgb(255, 170, 40), true).clicked() {
                go = true;
            }
            p.text(Pos2::new(cx, y + 452.0), Align2::CENTER_CENTER, "화면을 눌러도 시작돼요", font(13.0), Color32::from_white_alpha(170));
            p.text(Pos2::new(cx, r.bottom() - 46.0), Align2::CENTER_CENTER, format!("버전 {}", APP_VERSION), font(13.0), Color32::from_white_alpha(150));
        });
        if go || (t > 0.4 && ui.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Space) || i.pointer.any_click())) {
            self.splash = false;
        }
    }

    fn main_screen(&mut self, ui: &mut Ui, now: f64) {
        let width = ui.available_width();
        let narrow = width < 760.0 || self.force_narrow;
        if narrow {
            self.mobile_bars(ui);
        } else {
            egui::Panel::left("nav")
                .exact_size(220.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(egui::Frame::NONE.fill(surface()).inner_margin(Margin::symmetric(16, 22)))
                .show(ui, |ui| self.nav(ui));
        }
        if width > 1050.0 {
            egui::Panel::right("side")
                .exact_size(330.0)
                .resizable(false)
                .show_separator_line(false)
                .frame(egui::Frame::NONE.fill(bg()).inner_margin(Margin { left: 8, right: 24, top: 22, bottom: 16 }))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.side(ui, now));
                });
        }
        egui::CentralPanel::no_frame().frame(egui::Frame::NONE.fill(bg())).show(ui, |ui| {
            // line between nav and content
            let r = ui.max_rect();
            if !narrow {
                ui.painter().line_segment([r.left_top(), r.left_bottom()], Stroke::new(2.0, border()));
            }
            match self.tab {
                Tab::Learn => self.learn_tab(ui, now),
                Tab::Letters => self.letters_tab(ui),
                Tab::Words => self.words_tab(ui),
                Tab::Notebook => self.notebook_tab(ui, now),
                Tab::Profile => self.profile_tab(ui, now),
            }
        });
    }

    fn nav(&mut self, ui: &mut Ui) {
        // logo
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 64.0), Sense::hover());
        let p = ui.painter();
        paint_mascot(p, Pos2::new(rect.left() + 28.0, rect.center().y - 4.0), 20.0, true);
        p.text(Pos2::new(rect.left() + 62.0, rect.center().y - 12.0), Align2::LEFT_CENTER, "사왓디", bold(27.0), PRIMARY);
        p.text(Pos2::new(rect.left() + 63.0, rect.center().y + 15.0), Align2::LEFT_CENTER, "한국인을 위한 태국어", font(12.5), muted());
        ui.add_space(18.0);
        let items = [(Tab::Learn, "학습"), (Tab::Letters, "알파벳 학습"), (Tab::Words, "단어장"), (Tab::Notebook, "오답 노트"), (Tab::Profile, "프로필")];
        for (tab, label) in items {
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
            let sel = self.tab == tab;
            let p = ui.painter();
            if sel {
                p.rect_filled(rect, CornerRadius::same(12), primary_soft());
                p.rect_stroke(rect, CornerRadius::same(12), Stroke::new(2.0, lighten(PRIMARY, 0.45)), StrokeKind::Inside);
            } else if resp.hovered() {
                p.rect_filled(rect, CornerRadius::same(12), hover());
            }
            let ic = Pos2::new(rect.left() + 28.0, rect.center().y);
            let col = if sel { PRIMARY } else { Color32::from_rgb(120, 125, 135) };
            match tab {
                Tab::Learn => paint_star(p, ic, 13.0, if sel { GOLD } else { col }),
                Tab::Letters => {
                    p.text(ic, Align2::CENTER_CENTER, "ก", bold(24.0), if sel { ORANGE } else { col });
                }
                Tab::Words => {
                    let b = Rect::from_center_size(ic, vec2(22.0, 26.0));
                    p.rect_filled(b, CornerRadius::same(4), if sel { BLUE } else { col });
                    p.text(b.center(), Align2::CENTER_CENTER, "가", bold(12.0), WHITE);
                }
                Tab::Notebook => paint_note(p, ic, 26.0, if sel { RED } else { col }),
                Tab::Profile => {
                    let c = if sel { Color32::from_rgb(140, 100, 230) } else { col };
                    p.circle_filled(ic + vec2(0.0, -6.0), 6.5, c);
                    p.circle_filled(ic + vec2(0.0, 11.0), 11.0, c);
                    p.rect_filled(Rect::from_min_size(ic + vec2(-13.0, 12.0), vec2(26.0, 12.0)), CornerRadius::ZERO, surface());
                    if sel {
                        p.rect_filled(Rect::from_min_size(ic + vec2(-13.0, 12.0), vec2(26.0, 12.0)), CornerRadius::ZERO, primary_soft());
                    } else if resp.hovered() {
                        p.rect_filled(Rect::from_min_size(ic + vec2(-13.0, 12.0), vec2(26.0, 12.0)), CornerRadius::ZERO, hover());
                    }
                }
            }
            p.text(Pos2::new(rect.left() + 56.0, rect.center().y), Align2::LEFT_CENTER, label, bold(16.0), if sel { PRIMARY } else { fg() });
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                self.tab = tab;
            }
            ui.add_space(4.0);
        }
        // bottom: sound toggle
        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            // order is bottom-up: quit, sound, then the theme switch above them
            {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
                let p = ui.painter();
                if resp.hovered() {
                    p.rect_filled(rect, CornerRadius::same(10), hover());
                }
                let c = Pos2::new(rect.left() + 26.0, rect.center().y);
                p.circle_stroke(c, 8.0, Stroke::new(2.5, RED));
                p.line_segment([c + vec2(0.0, -12.0), c + vec2(0.0, -2.0)], Stroke::new(2.5, surface()));
                p.line_segment([c + vec2(0.0, -11.0), c + vec2(0.0, -1.0)], Stroke::new(2.5, RED));
                p.text(Pos2::new(rect.left() + 50.0, rect.center().y), Align2::LEFT_CENTER, "앱 종료", font(14.0), muted());
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    self.tab = Tab::Profile;
                    self.confirm_exit = true;
                }
            }
            self.sound_row(ui);
            let dark = is_dark();
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
            let p = ui.painter();
            if resp.hovered() {
                p.rect_filled(rect, CornerRadius::same(10), hover());
            }
            let c = Pos2::new(rect.left() + 26.0, rect.center().y);
            if dark {
                p.circle_filled(c, 9.0, GOLD);
                p.circle_filled(c + vec2(5.0, -4.0), 8.0, if resp.hovered() { hover() } else { surface() });
            } else {
                p.circle_filled(c, 6.0, ORANGE);
                for k in 0..8 {
                    let a = k as f32 * std::f32::consts::TAU / 8.0;
                    let d = vec2(a.cos(), a.sin());
                    p.line_segment([c + d * 8.5, c + d * 11.5], Stroke::new(2.0, ORANGE));
                }
            }
            let label = if dark { "다크 모드" } else { "라이트 모드" };
            p.text(Pos2::new(rect.left() + 50.0, rect.center().y), Align2::LEFT_CENTER, label, font(14.0), muted());
            if resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text("눌러서 화면 밝기를 바꿔요").clicked() {
                self.p.theme = if dark { 1 } else { 2 };
            }
        });
    }

    fn sound_row(&mut self, ui: &mut Ui) {
        {
            let label = if self.audio.muted { "소리 꺼짐" } else { "소리 켜짐" };
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
            let p = ui.painter();
            if resp.hovered() {
                p.rect_filled(rect, CornerRadius::same(10), hover());
            }
            let c = if self.audio.muted { muted() } else { BLUE };
            paint_speaker(p, Pos2::new(rect.left() + 26.0, rect.center().y), 20.0, c);
            if self.audio.muted {
                p.line_segment([Pos2::new(rect.left() + 16.0, rect.center().y - 10.0), Pos2::new(rect.left() + 36.0, rect.center().y + 10.0)], Stroke::new(2.5, RED));
            }
            p.text(Pos2::new(rect.left() + 50.0, rect.center().y), Align2::LEFT_CENTER, label, font(14.0), muted());
            if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                self.audio.muted = !self.audio.muted;
                self.p.muted = self.audio.muted;
            }
        }
    }

    fn stat_row(&mut self, ui: &mut Ui) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
        let p = ui.painter();
        let y = rect.center().y;
        // wordmark: the little elephant and the app name
        paint_mascot(p, Pos2::new(rect.left() + 16.0, y + 1.0), 12.0, true);
        if rect.width() > 300.0 {
            p.text(Pos2::new(rect.left() + 36.0, y), Align2::LEFT_CENTER, "사왓디", bold(17.0), fg());
        }
        // stat chips, right-aligned: streak, XP, mistake notebook
        let streak_active = self.p.last_day == today();
        let n = self.p.notebook.len();
        let chips: [(u8, String, Color32, bool); 3] = [
            (0, self.p.streak_now().to_string(), ORANGE, streak_active),
            (1, self.p.xp_total.to_string(), GOLD, self.p.xp_total > 0),
            (2, n.to_string(), RED, n > 0),
        ];
        let mut x = rect.right() - 4.0;
        let mut note_rect = Rect::NOTHING;
        for (kind, text, color, on) in chips.iter().rev() {
            let g = p.layout_no_wrap(text.clone(), bold(14.0), if *on { ink(*color, 0.8) } else { muted() });
            let w = 34.0 + g.size().x + 12.0;
            let chip = Rect::from_min_max(Pos2::new(x - w, y - 15.0), Pos2::new(x, y + 15.0));
            p.rect_filled(chip, CornerRadius::same(15), if *on { lighten(*color, 0.86) } else { hover() });
            let ic = Pos2::new(chip.left() + 18.0, y);
            let icol = if *on { *color } else { faint() };
            match kind {
                0 => paint_flame(p, ic, 18.0, icol),
                1 => paint_bolt(p, ic, 18.0, icol),
                _ => paint_note(p, ic, 17.0, icol),
            }
            let tcol = if *on { ink(*color, 0.8) } else { muted() };
            p.galley(Pos2::new(chip.left() + 31.0, y - g.size().y / 2.0), g, tcol);
            if *kind == 2 {
                note_rect = chip;
            }
            x = chip.left() - 8.0;
        }
        let tip = ui.interact(note_rect, ui.scope_id().with("note_tip"), Sense::click());
        if tip.on_hover_cursor(CursorIcon::PointingHand).on_hover_text("오답 노트: 틀린 문제 수 (눌러서 열기)").clicked() {
            self.tab = Tab::Notebook;
        }
    }

    fn side(&mut self, ui: &mut Ui, now: f64) {
        self.stat_row(ui);
        ui.add_space(14.0);
        let w = ui.available_width();
        // daily goal
        card_frame().show(ui, |ui| {
            ui.set_width(w - 40.0);
            ui.label(RichText::new("오늘의 목표").font(bold(18.0)));
            ui.add_space(4.0);
            let xp = self.p.xp_today();
            let goal = self.p.daily_goal;
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                paint_bolt(ui.painter(), r.center(), 40.0, GOLD);
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("XP {} 얻기", goal)).font(bold(15.0)));
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
                    progress_bar(ui.painter(), r, xp as f32 / goal as f32, GOLD);
                    ui.label(RichText::new(format!("{} / {} XP", xp.min(goal), goal)).font(font(13.0)).color(muted()));
                });
            });
        });
        ui.add_space(12.0);
        // hearts
        // review of weak words
        let due = self.p.due_count();
        let learned = self.p.words_learned();
        if learned > 0 {
            card_frame().show(ui, |ui| {
                ui.set_width(w - 40.0);
                ui.label(RichText::new("복습하기").font(bold(18.0)));
                let msg = if due > 0 {
                    format!("다시 볼 때가 된 단어가 {}개 있어요.", due)
                } else {
                    "오늘 복습할 단어를 모두 끝냈어요!".to_string()
                };
                ui.label(RichText::new(msg).font(font(14.0)).color(muted()));
                ui.add_space(4.0);
                let label = if due > 0 { "약한 단어 복습하기" } else { "그래도 연습하기" };
                if button3d(ui, label, vec2(ui.available_width(), 44.0), BLUE, true).clicked() {
                    self.start(LessonKind::Practice, now);
                }
            });
            ui.add_space(12.0);
        }
        // short conversations
        if crate::data::dialogues_open(&self.p.done).len() >= 3 {
            card_frame().show(ui, |ui| {
                ui.set_width(w - 40.0);
                ui.label(RichText::new("대화 연습").font(bold(18.0)));
                ui.label(RichText::new("상대의 말을 듣고 알맞은 대답을 골라 보세요.").font(font(14.0)).color(muted()));
                ui.add_space(4.0);
                if button3d(ui, "대화 시작하기", vec2(ui.available_width(), 44.0), PRIMARY, true).clicked() {
                    self.start(LessonKind::Dialogue, now);
                }
            });
            ui.add_space(12.0);
        }
        // mistake notebook
        {
            let n = self.p.notebook.len();
            card_frame().show(ui, |ui| {
                ui.set_width(w - 40.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                    paint_note(ui.painter(), r.center(), 26.0, RED);
                    ui.label(RichText::new("오답 노트").font(bold(18.0)));
                });
                let msg = if n > 0 {
                    format!("틀린 문제 {}개가 기다리고 있어요. 두 번 연달아 맞히면 졸업해요.", n)
                } else {
                    "틀린 문제가 없어요. 레슨에서 틀리면 여기에 모여요.".to_string()
                };
                ui.label(RichText::new(msg).font(font(14.0)).color(muted()));
                if n > 0 {
                    ui.add_space(4.0);
                    if button3d(ui, "틀린 문제 다시 풀기", vec2(ui.available_width(), 44.0), RED, true).clicked() {
                        self.start(LessonKind::Mistakes, now);
                    }
                }
            });
            ui.add_space(12.0);
        }
        // phrase of the day
        let all: Vec<(usize, usize)> = UNITS.iter().enumerate().flat_map(|(u, unit)| (0..unit.sentences.len()).map(move |s| (u, s))).collect();
        let (u, s) = all[(today().rem_euclid(all.len() as i32)) as usize];
        let sent = &UNITS[u].sentences[s];
        let female = self.p.female();
        card_frame().show(ui, |ui| {
            ui.set_width(w - 40.0);
            ui.label(RichText::new("오늘의 표현").font(bold(18.0)));
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if speaker_button(ui, 44.0, BLUE).clicked() {
                    self.audio.speak(&lesson::sentence_thai(sent, female), false);
                }
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(lesson::sentence_thai(sent, female)).font(bold(22.0)).color(fg()));
                    ui.label(RichText::new(lesson::sentence_pron(sent, female)).font(font(13.5)).color(muted()));
                    ui.label(RichText::new(lesson::sentence_tones(sent, female)).font(bold(13.5)).color(TONE));
                });
            });
            ui.label(RichText::new(sent.korean()).font(font(15.0)));
        });
        ui.add_space(12.0);
        // tip
        card_frame().show(ui, |ui| {
            ui.set_width(w - 40.0);
            ui.label(RichText::new("알아 두면 좋아요").font(bold(16.0)));
            ui.label(
                RichText::new("한글 발음이 같아도 성조가 다르면 다른 말이에요. ไม่ 마이↘ (아니다), ไหม 마이↗ (질문), ใหม่ 마이↓ (새로운). 주황색 화살표가 성조예요.")
                    .font(font(14.0))
                    .color(muted()),
            );
        });
    }
}

// ---------------------------------------------------------------- learn tab (path)

impl App {
    fn learn_tab(&mut self, ui: &mut Ui, now: f64) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let full = ui.available_width();
            let col_w = 560.0f32.min(full - 48.0);
            let left = ui.max_rect().left() + (full - col_w) / 2.0;
            ui.add_space(24.0);
            let current = self.p.current();
            if full < 760.0 || self.force_narrow {
                ui.add_space(0.0);
                let pad = ((full - col_w) / 2.0).max(0.0);
                ui.horizontal(|ui| {
                    ui.add_space(pad);
                    ui.vertical(|ui| {
                        ui.set_width(col_w);
                        self.mobile_cards(ui, now, col_w);
                    });
                });
            }
            let mut start: Option<(usize, usize)> = None;
            let mut locked_click = false;
            for (u, unit) in UNITS.iter().enumerate() {
                let color = rgb(unit.color);
                // course level heading at the start of each level
                if let Some((name, desc)) = crate::data::level_start(u) {
                    let (row, _) = ui.allocate_exact_size(vec2(full, if u == 0 { 56.0 } else { 80.0 }), Sense::hover());
                    let p = ui.painter();
                    let y = row.bottom() - 30.0;
                    let g = p.layout_no_wrap(format!("{name} 과정 · {desc}"), bold(16.0), muted());
                    let tw = g.size().x + 30.0;
                    let cx = left + col_w / 2.0;
                    p.line_segment([Pos2::new(left, y), Pos2::new(cx - tw / 2.0, y)], Stroke::new(2.0, border()));
                    p.line_segment([Pos2::new(cx + tw / 2.0, y), Pos2::new(left + col_w, y)], Stroke::new(2.0, border()));
                    p.galley(Pos2::new(cx - g.size().x / 2.0, y - g.size().y / 2.0), g, muted());
                }
                // banner
                let (row, _) = ui.allocate_exact_size(vec2(full, 92.0), Sense::hover());
                let banner = Rect::from_min_size(Pos2::new(left, row.top()), vec2(col_w, 88.0));
                let paid_locked = self.paywall_locks(u);
                let unlocked = !paid_locked && (self.p.done[u] > 0 || current.map(|c| c.0 == u).unwrap_or(false) || u == 0);
                let bc = if unlocked { color } else { faint() };
                let p = ui.painter();
                soft_shadow(p, banner, CornerRadius::same(22), 1.2);
                p.rect_filled(banner, CornerRadius::same(22), bc);
                p.rect_filled(Rect::from_min_max(banner.min, Pos2::new(banner.max.x, banner.min.y + banner.height() * 0.5)), CornerRadius { nw: 22, ne: 22, sw: 0, se: 0 }, Color32::from_white_alpha(14));
                if col_w >= 420.0 {
                    paint_diamond_row(p, banner.left_bottom() + vec2(30.0, -9.0), (col_w / 22.0) as usize - 2, 22.0, 4.5, Color32::from_white_alpha(38));
                }
                p.text(banner.left_top() + vec2(22.0, if col_w < 420.0 { 10.0 } else { 18.0 }), Align2::LEFT_TOP, format!("단원 {} · {}", u + 1, crate::data::level_name(u)), bold(14.0), Color32::from_white_alpha(225));
                p.text(banner.left_top() + vec2(22.0, if col_w < 420.0 { 32.0 } else { 38.0 }), Align2::LEFT_TOP, unit.title, bold(if col_w < 420.0 { 21.0 } else { 23.0 }), WHITE);
                if col_w < 420.0 {
                    p.text(banner.left_top() + vec2(22.0, 62.0), Align2::LEFT_TOP, unit.desc, font(12.5), Color32::from_white_alpha(235));
                } else {
                    p.text(banner.right_center() + vec2(-22.0, 0.0), Align2::RIGHT_CENTER, unit.desc, font(14.0), Color32::from_white_alpha(235));
                }
                let doneu = self.p.done[u] as usize;
                p.text(
                    banner.right_top() + vec2(-22.0, 16.0),
                    Align2::RIGHT_TOP,
                    if paid_locked { "전체 열기".to_string() } else { format!("{}/{}", doneu, LESSONS_PER_UNIT) },
                    bold(13.0),
                    Color32::from_white_alpha(225),
                );
                ui.add_space(26.0);
                let sign = if u % 2 == 0 { -1.0 } else { 1.0 };
                let offsets = [0.0, 62.0, 96.0, 62.0, 0.0];
                let mut prev_node: Option<Pos2> = None;
                for l in 0..LESSONS_PER_UNIT {
                    let (row, _) = ui.allocate_exact_size(vec2(full, 100.0), Sense::hover());
                    let c = Pos2::new(left + col_w / 2.0 + sign * offsets[l], row.center().y);
                    let state = if paid_locked {
                        0
                    } else if l < doneu {
                        1
                    } else if current == Some((u, l)) {
                        2
                    } else {
                        0
                    };
                    let node = Rect::from_center_size(c, vec2(78.0, 78.0));
                    let resp = ui.interact(node, ui.scope_id().with(("node", u, l)), Sense::click());
                    let p = ui.painter();
                    let r = 36.0;
                    let (face, edge) = match state {
                        0 => (locked(), darken(locked(), 0.85)),
                        _ => (color, darken(color, 0.78)),
                    };
                    let pressed = resp.is_pointer_button_down_on() && state != 0;
                    let lift = if pressed { 6.0 } else { 0.0 };
                    // dotted path from the previous node (only the middle part, clear of both nodes)
                    if let Some(pc) = prev_node {
                        for k in 0..4 {
                            let t = 0.36 + 0.093 * k as f32;
                            let dot = pc + (c - pc) * t;
                            p.circle_filled(dot, 3.0, if state == 0 { track() } else { lighten(color, 0.45) });
                        }
                    }
                    prev_node = Some(c);
                    if state == 2 {
                        // ring
                        let mut ring = diamond_points(c + vec2(0.0, 3.0), r + 13.0, 1.45);
                        ring.push(ring[0]);
                        p.add(egui::Shape::line(ring.clone(), Stroke::new(5.0, track())));
                        // the first quarter of the ring, in the unit's colour
                        let part: Vec<Pos2> = ring[30..].iter().chain(ring[..1].iter()).copied().collect();
                        p.add(egui::Shape::line(part, Stroke::new(5.0, color)));
                    }
                    let _ = edge;
                    if state == 0 {
                        // locked: a quiet outline instead of a heavy grey block
                        let mut outline = diamond_points(c, r + 1.0, 1.4);
                        p.add(egui::Shape::convex_polygon(outline.clone(), surface(), Stroke::NONE));
                        outline.push(outline[0]);
                        p.add(egui::Shape::line(outline, Stroke::new(2.0, border())));
                    } else {
                        paint_diamond(p, c + vec2(0.0, 6.0), r + 3.0, Color32::from_rgba_unmultiplied(40, 44, 90, if is_dark() { 60 } else { 24 }));
                        let fc = if resp.hovered() { lighten(face, 0.08) } else { face };
                        paint_diamond(p, c + vec2(0.0, lift), r + 3.0, fc);
                    }
                    let ic = c + vec2(0.0, lift);
                    let review = l == LESSONS_PER_UNIT - 1;
                    match state {
                        1 => {
                            if review {
                                paint_trophy(p, ic, 38.0, GOLD);
                            } else {
                                paint_check(p, ic, 32.0, WHITE, 7.0);
                            }
                        }
                        2 => {
                            if review {
                                paint_trophy(p, ic, 38.0, WHITE);
                            } else {
                                paint_star(p, ic, 18.0, WHITE);
                            }
                        }
                        _ => {
                            if review {
                                paint_trophy(p, ic, 38.0, faint());
                            } else {
                                paint_lock(p, ic, 30.0, faint());
                            }
                        }
                    }
                    if state == 2 {
                        let bob = (now * 3.0).sin() as f32 * 3.0;
                        let label = if l == 0 && u == 0 && self.p.lessons_done == 0 { "시작하기" } else { "시작" };
                        let g = p.layout_no_wrap(label.into(), bold(15.0), color);
                        let br = Rect::from_center_size(c + vec2(0.0, -r - 34.0 + bob), g.size() + vec2(26.0, 18.0));
                        p.rect_filled(br.translate(vec2(0.0, 3.0)), CornerRadius::same(10), border());
                        p.rect_filled(br, CornerRadius::same(10), surface());
                        p.rect_stroke(br, CornerRadius::same(10), Stroke::new(2.0, border()), StrokeKind::Inside);
                        let tip = br.center_bottom();
                        p.add(egui::Shape::convex_polygon(
                            vec![tip + vec2(-8.0, -1.0), tip + vec2(8.0, -1.0), tip + vec2(0.0, 8.0)],
                            surface(),
                            Stroke::NONE,
                        ));
                        p.line_segment([tip + vec2(-8.0, 0.0), tip + vec2(0.0, 8.0)], Stroke::new(2.0, border()));
                        p.line_segment([tip + vec2(8.0, 0.0), tip + vec2(0.0, 8.0)], Stroke::new(2.0, border()));
                        p.galley(br.center() - g.size() / 2.0, g, color);
                    }
                    // mascot beside the path
                    if l == 2 && (state != 0 || (u == 0 && !paid_locked)) {
                        let mc = Pos2::new(left + col_w / 2.0 - sign * 150.0, row.center().y + 10.0);
                        paint_mascot(p, mc, 34.0, state == 1);
                    }
                    let hover = if review { "단원 복습 · +20 XP" } else { "레슨 · +10 XP" };
                    let resp = if paid_locked {
                        resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text("전체 단원 열기")
                    } else if state != 0 {
                        resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(format!("{} {}/{} · {}", unit.title, l + 1, LESSONS_PER_UNIT, hover))
                    } else {
                        resp.on_hover_text("앞의 레슨을 먼저 끝내세요")
                    };
                    if resp.clicked() {
                        if paid_locked {
                            self.show_paywall = true;
                        } else if state == 0 {
                            locked_click = true;
                        } else {
                            start = Some((u, l));
                        }
                    }
                }
                ui.add_space(30.0);
            }
            let (row, _) = ui.allocate_exact_size(vec2(full, 80.0), Sense::hover());
            ui.painter().text(row.center(), Align2::CENTER_CENTER, "새 단원이 곧 추가될 예정이에요!", font(15.0), muted());
            if let Some((u, l)) = start {
                self.try_start(u, l, now);
            }
            if locked_click {
                self.toast("앞의 레슨을 먼저 끝내야 열려요", now);
            }
        });
    }
}

// ---------------------------------------------------------------- letters tab

impl App {
    fn page_header(ui: &mut Ui, title: &str, sub: &str) {
        ui.add_space(26.0);
        // the little elephant sits at the right end of the title line
        let top = ui.cursor().top();
        let right = ui.max_rect().right();
        paint_mascot(ui.painter(), Pos2::new(right - 30.0, top + 18.0), 17.0, true);
        ui.label(RichText::new(title).font(bold(28.0)));
        ui.label(RichText::new(sub).font(font(15.0)).color(muted()));
        ui.add_space(14.0);
    }

    fn letters_tab(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let full = ui.available_width();
            let col_w = 700.0f32.min(full - 48.0);
            let pad = ((full - col_w) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(pad);
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    Self::page_header(ui, "알파벳 학습", "태국 글자를 눌러 소리를 들어 보고, 아래 퀴즈로 연습해 보세요.");
                    ui.label(RichText::new("퀴즈 (각 10문제)").font(bold(19.0)));
                    ui.add_space(6.0);
                    let now = ui.input(|i| i.time);
                    for (group, items) in [
                        (
                            "글자와 소리",
                            &[
                                ("자음 이름 읽기", LessonKind::Letters),
                                ("자음 계급 맞추기", LessonKind::ConsonantClass),
                                ("모음 소리 맞추기", LessonKind::Vowels),
                                ("받침 소리 맞추기", LessonKind::Finals),
                                ("성조 규칙 맞추기", LessonKind::ToneRules),
                            ][..],
                        ),
                        (
                            "읽기 연습",
                            &[
                                ("단어 글자 맞추기", LessonKind::Spelling),
                                ("단어 읽기 연습", LessonKind::Reading),
                                ("문장 읽기 (뜻 고르기)", LessonKind::SentenceReading),
                                ("표지판 읽기", LessonKind::Signs),
                                ("태국 숫자 읽기", LessonKind::Digits),
                            ][..],
                        ),
                    ] {
                        ui.label(RichText::new(group).font(font(14.0)).color(muted()));
                        ui.add_space(2.0);
                        for (label, kind) in items {
                            if button3d(ui, label, vec2(col_w.min(420.0), 44.0), ORANGE, true).clicked() {
                                self.start(*kind, now);
                            }
                            ui.add_space(6.0);
                        }
                        ui.add_space(4.0);
                    }
                    ui.add_space(10.0);
                    ui.label(RichText::new("글자표").font(bold(19.0)));
                    ui.label(RichText::new("글자를 눌러 소리를 들어 보세요. 자음은 성조 규칙에 따라 세 무리로 나뉘어요.").font(font(14.0)).color(muted()));
                    ui.add_space(8.0);
                    // pages: 0 basic consonants, 1 other consonants, 2 vowels, 3 final sounds, 4 the five tones, 5 tone rules
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                        for (v, label) in [(0u8, "자음 28자"), (1, "자음 16자"), (2, "모음 23개"), (3, "받침 8개"), (4, "성조 5개"), (5, "성조 규칙"), (6, "특수 표기"), (7, "태국 숫자")] {
                            let sel = self.letters_page == v;
                            let r = if sel {
                                button3d(ui, label, vec2(108.0, 44.0), PRIMARY, true)
                            } else {
                                outline_button(ui, label, vec2(108.0, 44.0), muted())
                            };
                            if r.clicked() {
                                self.letters_page = v;
                            }
                        }
                    });
                    ui.add_space(12.0);
                    if self.letters_page <= 1 {
                        let classes = [(PRIMARY, "중자음"), (ORANGE, "고자음"), (BLUE, "저자음")];
                        ui.horizontal(|ui| {
                            for (c, label) in classes {
                                let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                                ui.painter().circle_filled(r.center(), 6.0, c);
                                ui.label(RichText::new(label).font(font(14.0)).color(muted()));
                                ui.add_space(10.0);
                            }
                        });
                        ui.add_space(8.0);
                        let per_row = ((col_w + 10.0) / (128.0 + 10.0)).floor().max(1.0) as usize;
                        // widen the cells so a row fills the column
                        let cell = vec2(((col_w - 10.0 * (per_row as f32 - 1.0)) / per_row as f32).floor().min(190.0), 132.0);
                        let list: &[crate::data::Consonant] = if self.letters_page == 0 { CONSONANTS } else { crate::data::CONSONANTS_MORE };
                        if self.letters_page == 1 {
                            ui.label(RichText::new("자주 쓰지 않는 글자들이에요. ฃ ฅ 두 글자는 지금은 쓰지 않아요. 눌러서 이름을 들어 보세요.").font(font(14.0)).color(muted()));
                            ui.add_space(6.0);
                        }
                        for row in list.chunks(per_row) {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 10.0;
                                for cons in row {
                                    let (rect, resp) = ui.allocate_exact_size(cell, Sense::click());
                                    let p = ui.painter();
                                    let col = [PRIMARY, ORANGE, BLUE][cons.class as usize];
                                    let face = Rect::from_min_max(rect.min, rect.max - vec2(0.0, 3.0));
                                    soft_shadow(p, face, CornerRadius::same(16), 0.6);
                                    p.rect_filled(face, CornerRadius::same(16), if resp.hovered() { lighten(col, 0.9) } else { surface() });
                                    p.rect_stroke(face, CornerRadius::same(16), Stroke::new(1.2, if resp.hovered() { lighten(col, 0.4) } else { border() }), StrokeKind::Inside);
                                    p.text(face.center_top() + vec2(0.0, 12.0), Align2::CENTER_TOP, cons.letter, bold(42.0), col);
                                    p.text(face.center_top() + vec2(0.0, 72.0), Align2::CENTER_TOP, cons.name_pron, bold(13.5), fg());
                                    p.text(face.center_top() + vec2(0.0, 93.0), Align2::CENTER_TOP, format!("{} · {}", cons.sound, cons.meaning), font(12.0), muted());
                                    if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                                        self.audio.speak(cons.name, true);
                                    }
                                }
                            });
                            ui.add_space(4.0);
                        }
                    } else if self.letters_page == 2 {
                        ui.label(RichText::new("모음은 짧은 소리와 긴 소리가 따로 있어요. 짧고 긴 차이가 단어의 뜻과 성조를 바꿔요. 아래는 자음 ก 위에 모음을 얹은 모양이에요(ก 자리에는 어떤 자음이든 올 수 있어요).").font(font(14.0)).color(muted()));
                        ui.add_space(8.0);
                        for v in VOWELS {
                            card_frame().inner_margin(Margin::symmetric(14, 8)).show(ui, |ui| {
                                ui.set_width(col_w - 32.0);
                                ui.horizontal(|ui| {
                                    let (r, _) = ui.allocate_exact_size(vec2(92.0, 52.0), Sense::hover());
                                    ui.painter().text(r.left_center(), Align2::LEFT_CENTER, v.form, bold(26.0), ORANGE);
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(v.sound).font(bold(16.0)));
                                        ui.label(RichText::new(format!("예: {}  ({})", v.example, v.example_pron)).font(font(15.0)).color(muted()));
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if small_speaker(ui, BLUE).clicked() {
                                            self.audio.speak(v.example, true);
                                        }
                                    });
                                });
                            });
                            ui.add_space(6.0);
                        }
                    } else if self.letters_page == 3 {
                        self.finals_page(ui, col_w);
                    } else if self.letters_page == 4 {
                        ui.label(RichText::new("태국어에는 성조가 5개 있어요. 한글로 똑같이 '마이'라고 써도, 성조가 다르면 전혀 다른 말이 돼요. 눌러서 들어 보세요.").font(font(15.0)).color(muted()));
                        ui.add_space(8.0);
                        for t in TONES {
                            card_frame().inner_margin(Margin::symmetric(18, 10)).show(ui, |ui| {
                                ui.set_width(col_w - 40.0);
                                ui.horizontal(|ui| {
                                    let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                                    let p = ui.painter();
                                    p.rect_filled(r, CornerRadius::same(14), lighten(TONE, 0.85));
                                    p.text(r.center(), Align2::CENTER_CENTER, t.mark, bold(30.0), TONE);
                                    ui.add_space(8.0);
                                    ui.vertical(|ui| {
                                        ui.set_width(250.0);
                                        ui.add_space(6.0);
                                        ui.label(RichText::new(t.name).font(bold(18.0)));
                                        ui.label(RichText::new(t.how).font(font(14.0)).color(muted()));
                                    });
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if small_speaker(ui, BLUE).clicked() {
                                            self.audio.speak(t.example, true);
                                        }
                                        ui.vertical(|ui| {
                                            ui.add_space(6.0);
                                            ui.label(RichText::new(format!("{}  {}{}", t.example, t.example_pron, t.mark)).font(bold(20.0)));
                                            ui.label(RichText::new(t.example_ko).font(font(14.0)).color(muted()));
                                        });
                                    });
                                });
                            });
                            ui.add_space(6.0);
                        }
                        ui.add_space(8.0);
                        card_frame().show(ui, |ui| {
                            ui.set_width(col_w - 40.0);
                            ui.label(RichText::new("성조는 어떻게 정해질까요?").font(bold(17.0)));
                            ui.label(
                                RichText::new(
                                    "글자의 자음 무리(중자음·고자음·저자음), 성조 기호(่ ้ ๊ ๋), 음절이 끝나는 방식이 합쳐져 성조가 정해져요. \
                                     처음에는 규칙보다 단어마다 붙은 주황색 화살표를 보고 소리를 따라 하며 익히는 게 쉬워요.",
                                )
                                .font(font(14.0))
                                .color(muted()),
                            );
                        });
                    } else if self.letters_page == 5 {
                        self.tone_rules_page(ui, col_w);
                    } else if self.letters_page == 6 {
                        self.specials_page(ui, col_w);
                    } else {
                        self.digits_page(ui, col_w);
                    }
                    ui.add_space(30.0);
                });
            });
        });
    }

    /// Profile "정보" sections: learning summary, notice about AI voice and checking, licenses.
    /// Reads the learner's recording and the native clip and works out their pitch shapes.
    fn analyze_voice(&mut self, text: &str) {
        self.rec_mine = notify::rec_path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| crate::pitch::parse_wav(&b))
            .map(|pcm| crate::pitch::shape(&pcm, 60))
            .unwrap_or_default();
        self.rec_native = crate::audio::clip_pcm(text).map(|pcm| crate::pitch::shape(&pcm, 60)).unwrap_or_default();
    }

    /// The learner stopped (or the time ran out): keep the recording if there is one and analyse it.
    fn finish_recording(&mut self, text: &str) {
        notify::rec_stop();
        if notify::rec_has() {
            self.rec = RecUi::Have;
            self.analyze_voice(text);
        } else {
            self.rec = RecUi::Idle;
        }
    }

    /// The backup text: a header line and the whole learning record.
    fn backup_code(&self) -> String {
        format!("{}\n{}", BACKUP_HEADER, ron::to_string(&self.p).unwrap_or_default())
    }

    fn parse_backup(text: &str) -> Result<Progress, String> {
        let text = text.trim();
        let body = text.strip_prefix(BACKUP_HEADER).ok_or("사왓디 백업 코드가 아니에요. 코드를 다시 복사해 주세요")?;
        ron::from_str::<Progress>(body.trim()).map_err(|_| "백업 코드가 깨졌어요. 처음부터 끝까지 복사했는지 확인해 주세요".to_string())
    }

    /// Books (or cancels) the daily reminders to match the saved settings.
    fn sync_reminder(&self) {
        if self.p.reminder_on {
            notify::schedule(self.p.reminder_min, self.p.last_day == today());
        } else {
            notify::cancel();
        }
    }

    fn about_sections(&mut self, ui: &mut Ui, col_w: f32, now: f64) {
        ui.label(RichText::new("정보").font(bold(21.0)));
        ui.add_space(4.0);
        egui::CollapsingHeader::new(RichText::new("학습 알림").font(bold(17.0))).default_open(true).show(ui, |ui| {
            if !notify::supported() {
                ui.label(RichText::new("학습 알림은 휴대폰에서만 쓸 수 있어요.").font(font(14.0)).color(muted()));
                return;
            }
            let mut changed = false;
            let mut on = self.p.reminder_on;
            if ui.checkbox(&mut on, RichText::new("매일 학습 알림 받기").font(font(15.0))).changed() {
                self.p.reminder_on = on;
                if on {
                    notify::request_permission();
                }
                changed = true;
            }
            if self.p.reminder_on {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("알림 시각  {}", notify::time_text(self.p.reminder_min))).font(font(15.0)));
                    if ui.button(RichText::new("−30분").font(font(14.0))).clicked() {
                        self.p.reminder_min = (self.p.reminder_min + 24 * 60 - 30) % (24 * 60);
                        changed = true;
                    }
                    if ui.button(RichText::new("+30분").font(font(14.0))).clicked() {
                        self.p.reminder_min = (self.p.reminder_min + 30) % (24 * 60);
                        changed = true;
                    }
                });
                if ui.button(RichText::new("알림 시험").font(font(14.0))).clicked() {
                    notify::request_permission();
                    notify::test();
                }
            }
            ui.add_space(2.0);
            ui.label(RichText::new("정해 둔 시각 무렵에 알림이 한 번 와요(절전 때문에 최대 1시간 늦을 수 있어요). 그날 이미 공부했으면 쉬고, 앱을 7일 동안 열지 않으면 알림이 멈춰요. 알림이 오지 않으면 휴대폰 설정에서 알림을 허용해 주세요.").font(font(13.0)).color(muted()));
            if changed {
                self.sync_reminder();
            }
        });
        if notify::paywall_enabled() {
            egui::CollapsingHeader::new(RichText::new("전체 단원 · 구매 복원").font(bold(17.0))).show(ui, |ui| {
                ui.set_width(col_w - 40.0);
                if self.owned {
                    ui.label(RichText::new("전체 단원이 열려 있어요. 구매해 주셔서 감사해요!").font(font(14.0)));
                } else {
                    ui.label(RichText::new(format!("단원 1~{}은 무료예요. 한 번 결제하면 모든 단원이 열려요(구독 아님).", FREE_UNITS)).font(font(13.5)).color(muted()));
                    if button3d(ui, "전체 단원 열기", vec2(220.0, 46.0), PRIMARY, true).clicked() {
                        self.show_paywall = true;
                    }
                }
                ui.add_space(4.0);
                ui.label(RichText::new("앱을 다시 설치했거나 새 폰이라면, 구매할 때 쓴 구글 계정으로 로그인한 상태에서 아래 버튼을 눌러 주세요.").font(font(13.5)).color(muted()));
                if outline_button(ui, "구매 복원", vec2(200.0, 44.0), fg()).clicked() {
                    notify::billing_refresh();
                    self.toast("구글 플레이에서 구매 내역을 확인하고 있어요", now);
                }
            });
        }
        egui::CollapsingHeader::new(RichText::new("학습 기록 백업").font(bold(17.0))).show(ui, |ui| {
            ui.set_width(col_w - 40.0);
            ui.label(RichText::new("폰을 바꾸거나 앱을 다시 설치하기 전에 백업 코드를 복사해 메모 앱이나 메시지(나에게 보내기)에 붙여 두세요. 새 기기에서 코드를 복사한 뒤 '불러오기'를 누르면 기록이 돌아와요. 코드는 이 기기 밖으로 자동 전송되지 않아요.").font(font(13.5)).color(muted()));
            ui.add_space(4.0);
            if outline_button(ui, "백업 코드 복사", vec2(200.0, 44.0), fg()).clicked() {
                let code = self.backup_code();
                if !notify::clip_set(&code) {
                    ui.ctx().copy_text(code);
                }
                self.toast("백업 코드를 복사했어요", now);
            }
            if !notify::supported() {
                ui.add_space(4.0);
                ui.label(RichText::new("불러오기: 코드를 아래 칸에 붙여 넣으세요.").font(font(13.5)).color(muted()));
                ui.add(egui::TextEdit::multiline(&mut self.backup_input).desired_rows(3).desired_width(f32::INFINITY));
            }
            ui.add_space(4.0);
            if outline_button(ui, if notify::supported() { "클립보드에서 불러오기" } else { "붙여 넣은 코드로 불러오기" }, vec2(260.0, 44.0), fg()).clicked() {
                let text = if notify::supported() { notify::clip_get().unwrap_or_default() } else { self.backup_input.clone() };
                match Self::parse_backup(&text) {
                    Ok(p) => self.backup_pending = Some(p),
                    Err(e) => self.toast(&e, now),
                }
            }
            if let Some(p) = self.backup_pending.clone() {
                ui.add_space(4.0);
                let finished = p.done.iter().filter(|&&d| d as usize >= LESSONS_PER_UNIT).count();
                ui.label(RichText::new(format!("이 백업: 끝낸 레슨 {}개, 끝낸 단원 {}개, 연속 {}일. 지금 기록을 이걸로 바꿀까요?", p.lessons_done, finished, p.streak)).font(font(14.0)).color(RED));
                ui.horizontal(|ui| {
                    if button3d(ui, "바꾸기", vec2(120.0, 44.0), RED, true).clicked() {
                        let muted = self.p.muted;
                        self.p = p;
                        self.p.muted = muted;
                        self.p.fix();
                        self.backup_pending = None;
                        self.backup_input.clear();
                        self.sync_reminder();
                        self.toast("학습 기록을 불러왔어요", now);
                    }
                    if outline_button(ui, "취소", vec2(120.0, 44.0), fg()).clicked() {
                        self.backup_pending = None;
                    }
                });
            }
        });
        egui::CollapsingHeader::new(RichText::new("학습 요약").font(bold(17.0))).show(ui, |ui| {
            let p = &self.p;
            let finished = p.done.iter().filter(|&&d| d as usize >= LESSONS_PER_UNIT).count();
            let tried = p.stats.len();
            let strong = p.stats.values().filter(|w| w.level >= 4).count();
            let (right, wrong) = p.stats.values().fold((0u32, 0u32), |a, w| (a.0 + w.right, a.1 + w.wrong));
            let acc = if right + wrong > 0 { right * 100 / (right + wrong) } else { 0 };
            let t = today();
            let week: u32 = (0..7).map(|k| p.xp_by_day.get(&(t - k)).copied().unwrap_or(0)).sum();
            let lines = [
                format!("끝낸 단원  {} / {}", finished, UNITS.len()),
                format!("끝낸 레슨  {}  (만점 {})", p.lessons_done, p.perfect_lessons),
                format!("만난 단어  {}  (익힘 {})", tried, strong),
                format!("정답률  {}%  (맞음 {} · 틀림 {})", acc, right, wrong),
                format!("오답 노트  {}개", p.notebook.len()),
                format!("연속 학습  {}일  (최고 {}일)", p.streak, p.best_streak),
                format!("경험치  합계 {}  (최근 7일 {})", p.xp_total, week),
                format!("앱 버전 {} · {}", APP_VERSION, std::env::consts::OS),
            ];
            for l in lines {
                ui.label(RichText::new(l).font(font(15.0)));
            }
            ui.add_space(4.0);
            ui.label(RichText::new("시험 사용에 참여 중이라면 이 화면을 캡처해서 보내 주세요. 이 정보는 기기 밖으로 자동 전송되지 않아요.").font(font(13.0)).color(muted()));
        });
        egui::CollapsingHeader::new(RichText::new("음성과 내용 안내").font(bold(17.0))).show(ui, |ui| {
            ui.set_width(col_w - 40.0);
            for l in [
                "태국어 음성은 AI 음성 합성(Google Cloud Chirp 3 HD)으로 만들었어요. 사람이 녹음한 것이 아니라서 발음이나 성조가 어색한 곳이 있을 수 있어요.",
                "단어, 문장, 한글 발음, 성조 표시는 AI의 도움으로 만들었고 태국어 원어민의 검수가 아직 끝나지 않았어요. 틀린 곳을 발견하면 알려 주세요.",
                "한글 발음은 근사치예요. 한글로는 태국어의 성조와 길이를 모두 나타낼 수 없어요.",
                "이 앱은 카메라, 위치 같은 권한을 쓰지 않아요. 학습 알림을 켤 때 알림 권한을, '내 소리 녹음'을 처음 누를 때 마이크 권한을 물어봐요. 녹음은 이 기기 안에 잠깐 저장되고 레슨을 나가면 지워지며, 어디로도 전송되지 않아요. 학습 기록은 이 기기 안에만 저장돼요.",
            ] {
                ui.label(RichText::new(l).font(font(14.0)));
                ui.add_space(4.0);
            }
        });
        egui::CollapsingHeader::new(RichText::new("오픈소스 라이선스").font(bold(17.0))).show(ui, |ui| {
            ui.set_width(col_w - 40.0);
            ui.label(RichText::new(include_str!("../assets/legal/licenses.txt")).font(font(12.0)).color(muted()));
        });
    }

    /// "특수 표기" page: letter clusters, leading ห and อ, silent letters, the repeat sign.
    fn specials_page(&mut self, ui: &mut Ui, col_w: f32) {
        ui.label(RichText::new("글자를 읽다 보면 규칙대로 읽히지 않는 모양이 나와요. 다섯 가지만 알면 대부분 읽을 수 있어요.").font(font(15.0)).color(muted()));
        ui.add_space(8.0);
        for sp in crate::data::SPECIALS {
            card_frame().inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                ui.set_width(col_w - 32.0);
                ui.label(RichText::new(sp.title).font(bold(18.0)).color(ORANGE));
                ui.label(RichText::new(sp.desc).font(font(14.0)).color(muted()));
                ui.add_space(6.0);
                for (th, pron, ko) in sp.examples {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(*th).font(bold(22.0)));
                        ui.label(RichText::new(format!("{} · {}", pron, ko)).font(font(14.0)).color(muted()));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if small_speaker(ui, BLUE).clicked() {
                                self.audio.speak(th, true);
                            }
                        });
                    });
                }
            });
            ui.add_space(8.0);
        }
    }

    /// "태국 숫자" page: the ten Thai digits.
    fn digits_page(&mut self, ui: &mut Ui, col_w: f32) {
        ui.label(RichText::new("태국에서는 가격표나 메뉴에 태국 숫자가 자주 나와요. 모양을 익혀 두면 편해요. 눌러서 읽는 소리를 들어 보세요.").font(font(15.0)).color(muted()));
        ui.add_space(8.0);
        for d in crate::data::DIGITS {
            card_frame().inner_margin(Margin::symmetric(14, 8)).show(ui, |ui| {
                ui.set_width(col_w - 32.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(64.0, 50.0), Sense::hover());
                    ui.painter().text(r.left_center(), Align2::LEFT_CENTER, d.glyph, bold(34.0), ORANGE);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(format!("{}  ·  {}", d.arabic, d.ko)).font(bold(16.0)));
                        ui.label(RichText::new(format!("{}  ({})", d.name, d.pron)).font(font(15.0)).color(muted()));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if small_speaker(ui, BLUE).clicked() {
                            self.audio.speak(d.name, true);
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
    }

    /// "받침 8개" page: every letter that can end a syllable makes one of eight sounds.
    fn finals_page(&mut self, ui: &mut Ui, col_w: f32) {
        ui.label(RichText::new("받침(음절 끝에 오는 자음)은 글자가 많아도 소리는 8가지뿐이에요. 같은 소리끼리 묶어서 외워 보세요.").font(font(15.0)).color(muted()));
        ui.add_space(8.0);
        for f in crate::data::FINALS {
            card_frame().inner_margin(Margin::symmetric(14, 10)).show(ui, |ui| {
                ui.set_width(col_w - 32.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(col_w - 120.0);
                        ui.label(RichText::new(f.sound).font(bold(20.0)).color(ORANGE));
                        ui.label(RichText::new(f.letters).font(bold(20.0)));
                        ui.label(RichText::new(format!("예: {}  {} · {}", f.example, f.example_pron, f.example_ko)).font(font(15.0)).color(muted()));
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if small_speaker(ui, BLUE).clicked() {
                            self.audio.speak(f.example, true);
                        }
                    });
                });
            });
            ui.add_space(6.0);
        }
        card_frame().show(ui, |ui| {
            ui.set_width(col_w - 32.0);
            ui.label(RichText::new("알아 두세요").font(bold(17.0)));
            ui.add_space(4.0);
            ui.label(RichText::new("ฉ ผ ฝ ห อ ฮ 같은 글자는 받침으로 거의 쓰이지 않아요. 받침 ร ล ฬ는 ㄴ 소리가 나요(예: บ้าน 반). 받침이 ㄱ ㄷ ㅂ이면 죽은 음절이라 성조 규칙이 달라져요.").font(font(14.0)).color(muted()));
        });
    }

    /// "성조 규칙" page: consonant class + syllable type + tone mark decide the tone.
    fn tone_rules_page(&mut self, ui: &mut Ui, col_w: f32) {
        ui.label(RichText::new("성조는 ① 자음 계급 ② 음절의 끝 ③ 성조 부호로 정해져요. 계급별 표를 보고 예시를 눌러 들어 보세요.").font(font(15.0)).color(muted()));
        ui.add_space(8.0);
        card_frame().show(ui, |ui| {
            ui.set_width(col_w - 40.0);
            ui.label(RichText::new("먼저 알아 둘 것").font(bold(17.0)));
            ui.add_space(4.0);
            for line in [
                "산 음절: 긴 모음이거나, 끝소리가 ㅇ ㄴ ㅁ 이 우(ง น ม ย ว)인 음절",
                "죽은 음절: 짧은 모음으로 끝나거나, 끝소리가 ㄱ ㅂ ㄷ(ก บ ด 등)인 음절",
                "성조 부호: ่ 마이 엑 · ้ 마이 토 · ๊ 마이 뜨리 · ๋ 마이 짜따와",
            ] {
                ui.label(RichText::new(line).font(font(14.0)).color(muted()));
            }
        });
        ui.add_space(8.0);
        for class in 0..3u8 {
            let col = [PRIMARY, ORANGE, BLUE][class as usize];
            card_frame().inner_margin(Margin::symmetric(18, 12)).show(ui, |ui| {
                ui.set_width(col_w - 40.0);
                ui.label(RichText::new(crate::data::class_name(class)).font(bold(19.0)).color(col));
                ui.label(RichText::new(crate::data::class_letters(class)).font(bold(21.0)));
                ui.add_space(6.0);
                for r in TONE_RULES.iter().filter(|r| r.class == class) {
                    let t = tone_of(r.tone);
                    ui.label(RichText::new(r.when).font(font(14.0)).color(muted()));
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(44.0, 36.0), Sense::hover());
                        let p = ui.painter();
                        p.rect_filled(rect, CornerRadius::same(10), lighten(TONE, 0.85));
                        p.text(rect.center(), Align2::CENTER_CENTER, t.map(|t| t.mark).unwrap_or(""), bold(22.0), TONE);
                        ui.label(RichText::new(t.map(|t| t.name).unwrap_or("")).font(bold(16.0)));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if small_speaker(ui, BLUE).clicked() {
                                self.audio.speak(r.example, true);
                            }
                            ui.label(RichText::new(format!("{}  {} · {}", r.example, r.example_pron, r.example_ko)).font(font(15.0)));
                        });
                    });
                    ui.add_space(6.0);
                }
            });
            ui.add_space(8.0);
        }
        card_frame().show(ui, |ui| {
            ui.set_width(col_w - 40.0);
            ui.label(RichText::new("예외 두 가지").font(bold(17.0)));
            ui.add_space(4.0);
            ui.label(RichText::new("ห + 저자음(ง ญ น ม ย ร ล ว): 앞의 ห는 소리 나지 않고, 뒤 글자를 고자음처럼 읽어요. 예: หมู (무, 상승)").font(font(14.0)).color(muted()));
            ui.label(RichText::new("อ + ย: อยาก อยู่ อย่า อย่าง 네 단어는 อ가 소리 나지 않고, ย를 중자음처럼 읽어요.").font(font(14.0)).color(muted()));
            ui.add_space(4.0);
            ui.label(RichText::new("이 규칙은 처음엔 어려워요. 단어마다 붙은 주황색 화살표로 소리를 익히다가, 나중에 규칙으로 확인해 보세요.").font(font(14.0)).color(muted()));
        });
    }

    // ------------------------------------------------------------ mistake notebook tab

    fn notebook_tab(&mut self, ui: &mut Ui, now: f64) {
        let mut start = false;
        let mut remove: Option<usize> = None;
        let mut speak: Option<String> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let full = ui.available_width();
            let col_w = 680.0f32.min(full - 48.0);
            let pad = ((full - col_w) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(pad);
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    let n = self.p.notebook.len();
                    Self::page_header(
                        ui,
                        "오답 노트",
                        &format!("틀린 문제 {}개 · 처음 시도에서 두 번 연달아 맞히면 노트에서 졸업해요", n),
                    );
                    if n == 0 {
                        card_frame().show(ui, |ui| {
                            ui.set_width(col_w - 40.0);
                            ui.vertical_centered(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(140.0, 120.0), Sense::hover());
                                paint_mascot(ui.painter(), r.center(), 40.0, true);
                                ui.label(RichText::new("틀린 문제가 하나도 없어요!").font(bold(19.0)));
                                ui.label(RichText::new("레슨에서 틀리거나 건너뛴 문제는 여기에 모여요.").font(font(14.0)).color(muted()));
                            });
                        });
                        return;
                    }
                    if button3d(ui, &format!("틀린 문제 다시 풀기 ({}개)", n.min(12)), vec2(col_w, 52.0), RED, true).clicked() {
                        start = true;
                    }
                    ui.add_space(12.0);
                    let female = self.p.female();
                    let order = self.p.notebook_order();
                    for (u, item, sentence) in order {
                        let Some(idx) = self.p.notebook.iter().position(|m| m.unit == u && m.item == item && m.sentence == sentence) else { continue };
                        let m = self.p.notebook[idx].clone();
                        let (th, pron, tones, ko) = if sentence {
                            let s = &UNITS[u].sentences[item];
                            (lesson::sentence_thai(s, female), lesson::sentence_pron(s, female), lesson::sentence_tones(s, female), s.korean())
                        } else {
                            let w = &UNITS[u].words[item];
                            (w.th.to_string(), w.pron.to_string(), crate::data::tone_marks(w.tone), w.ko.to_string())
                        };
                        card_frame().inner_margin(Margin::symmetric(16, 12)).show(ui, |ui| {
                            ui.set_width(col_w - 34.0);
                            ui.horizontal(|ui| {
                                if small_speaker(ui, BLUE).clicked() {
                                    speak = Some(th.clone());
                                }
                                ui.add_space(4.0);
                                ui.vertical(|ui| {
                                    ui.set_width(if col_w >= 560.0 { col_w - 250.0 } else { col_w - 90.0 });
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&th).font(bold(22.0)));
                                        ui.label(RichText::new(if sentence { "문장" } else { "단어" }).font(font(12.0)).color(muted()));
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&pron).font(font(14.0)).color(muted()));
                                        ui.label(RichText::new(&tones).font(bold(14.0)).color(TONE));
                                    });
                                    ui.label(RichText::new(&ko).font(font(15.0)));
                                    ui.label(RichText::new(format!("단원 {} · {}", u + 1, UNITS[u].title)).font(font(12.0)).color(muted()));
                                });
                                if col_w >= 560.0 {
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if outline_button(ui, "외웠어요", vec2(84.0, 38.0), muted()).on_hover_text("노트에서 빼기").clicked() {
                                            remove = Some(idx);
                                        }
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(format!("{}번 틀림", m.wrong)).font(bold(14.0)).color(RED));
                                            ui.horizontal(|ui| {
                                                for k in 0..GRADUATE {
                                                    let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                                                    ui.painter().circle_filled(r.center(), 6.0, if k < m.streak { GREEN } else { locked() });
                                                }
                                            });
                                        });
                                    });
                                }
                            });
                            if col_w < 560.0 {
                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("{}번 틀림", m.wrong)).font(bold(14.0)).color(RED));
                                    for k in 0..GRADUATE {
                                        let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
                                        ui.painter().circle_filled(r.center(), 6.0, if k < m.streak { GREEN } else { locked() });
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if outline_button(ui, "외웠어요", vec2(84.0, 36.0), muted()).clicked() {
                                            remove = Some(idx);
                                        }
                                    });
                                });
                            }
                        });
                        ui.add_space(8.0);
                    }
                    ui.add_space(30.0);
                });
            });
        });
        if let Some(t) = speak {
            self.audio.speak(&t, false);
        }
        if let Some(i) = remove {
            self.p.notebook.remove(i);
            self.toast("오답 노트에서 뺐어요", now);
        }
        if start {
            self.start(LessonKind::Mistakes, now);
        }
    }

    // ------------------------------------------------------------ words tab

    fn words_tab(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let full = ui.available_width();
            let col_w = 680.0f32.min(full - 48.0);
            let pad = ((full - col_w) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(pad);
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    let learned = self.p.words_learned();
                    let total: usize = UNITS.iter().map(|u| u.words.len()).sum();
                    Self::page_header(ui, "단어장", &format!("배운 단어 {} / {}개 · 레슨을 끝내면 단어가 열려요", learned, total));
                    for (u, unit) in UNITS.iter().enumerate() {
                        let color = rgb(unit.color);
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(8.0, 26.0), Sense::hover());
                            ui.painter().rect_filled(r, CornerRadius::same(4), color);
                            ui.label(RichText::new(format!("단원 {} · {}", u + 1, unit.title)).font(bold(19.0)));
                        });
                        ui.add_space(4.0);
                        card_frame().inner_margin(Margin::symmetric(8, 6)).show(ui, |ui| {
                            ui.set_width(col_w - 20.0);
                            for (i, wd) in unit.words.iter().enumerate() {
                                let known = self.p.word_learned(u, i);
                                // on a phone the row needs three lines, or the meaning runs under the speaker button
                                let narrow = col_w < 560.0;
                                let (rect, _) = ui.allocate_exact_size(vec2(col_w - 20.0, if narrow { 82.0 } else { 48.0 }), Sense::hover());
                                let p = ui.painter();
                                if i > 0 {
                                    p.line_segment([rect.left_top() + vec2(10.0, 0.0), rect.right_top() - vec2(10.0, 0.0)], Stroke::new(1.0, border()));
                                }
                                let y = rect.center().y;
                                if known {
                                    if narrow {
                                        p.text(Pos2::new(rect.left() + 14.0, y - 26.0), Align2::LEFT_CENTER, wd.th, bold(22.0), fg());
                                        paint_pron(p, Pos2::new(rect.left() + 14.0, y + 1.0), false, wd.pron, &crate::data::tone_marks(wd.tone), 14.5);
                                        p.text(Pos2::new(rect.left() + 14.0, y + 25.0), Align2::LEFT_CENTER, wd.ko, font(15.0), fg());
                                    } else {
                                        p.text(Pos2::new(rect.left() + 14.0, y), Align2::LEFT_CENTER, wd.th, bold(22.0), fg());
                                        paint_pron(p, Pos2::new(rect.left() + 180.0, y), false, wd.pron, &crate::data::tone_marks(wd.tone), 15.0);
                                        p.text(Pos2::new(rect.left() + 330.0, y), Align2::LEFT_CENTER, wd.ko, font(15.5), fg());
                                    }
                                    // memory strength: 4 bars
                                    let level = self.p.stat(u, i).map(|s| s.level).unwrap_or(0);
                                    let bars = ((level as usize) + 1).min(4);
                                    for b in 0..4 {
                                        let bx = rect.right() - 92.0 + b as f32 * 9.0;
                                        let bh = 6.0 + b as f32 * 4.0;
                                        let r = Rect::from_min_max(Pos2::new(bx, y + 9.0 - bh), Pos2::new(bx + 6.0, y + 9.0));
                                        p.rect_filled(r, CornerRadius::same(2), if b < bars { GOLD } else { locked() });
                                    }
                                    let sp = Rect::from_center_size(Pos2::new(rect.right() - 28.0, y), vec2(44.0, 44.0));
                                    let resp = ui.interact(sp, ui.scope_id().with(("w", u, i)), Sense::click());
                                    let p = ui.painter();
                                    if resp.hovered() {
                                        p.rect_filled(sp, CornerRadius::same(10), lighten(BLUE, 0.85));
                                    }
                                    paint_speaker(p, sp.center(), 22.0, BLUE);
                                    if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                                        self.audio.speak(wd.th, false);
                                    }
                                } else {
                                    p.text(Pos2::new(rect.left() + 14.0, y), Align2::LEFT_CENTER, wd.th, bold(22.0), faint());
                                    p.text(Pos2::new(rect.left() + if narrow { 150.0 } else { 190.0 }, y), Align2::LEFT_CENTER, "? ? ?", font(15.0), faint());
                                    paint_lock(p, Pos2::new(rect.right() - 28.0, y), 20.0, faint());
                                }
                            }
                        });
                        ui.add_space(18.0);
                    }
                });
            });
        });
    }

    // ------------------------------------------------------------ profile tab

    fn profile_tab(&mut self, ui: &mut Ui, now: f64) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let full = ui.available_width();
            let col_w = 640.0f32.min(full - 48.0);
            let pad = ((full - col_w) / 2.0).max(0.0);
            ui.horizontal(|ui| {
                ui.add_space(pad);
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    ui.add_space(24.0);
                    // header
                    let (rect, _) = ui.allocate_exact_size(vec2(col_w, 150.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(rect, CornerRadius::same(18), primary_soft());
                    paint_mascot(p, Pos2::new(rect.left() + 90.0, rect.center().y - 4.0), 44.0, true);
                    p.text(Pos2::new(rect.left() + 170.0, rect.center().y - 22.0), Align2::LEFT_CENTER, "나의 태국어", bold(26.0), ink(PRIMARY, 0.75));
                    let lvl = 1 + self.p.xp_total / 100;
                    p.text(
                        Pos2::new(rect.left() + 170.0, rect.center().y + 14.0),
                        Align2::LEFT_CENTER,
                        format!("레벨 {} · 최고 연속 기록 {}일", lvl, self.p.best_streak),
                        font(15.0),
                        ink(PRIMARY, 0.7),
                    );
                    ui.add_space(18.0);
                    ui.label(RichText::new("통계").font(bold(21.0)));
                    let stats = [
                        ("연속 학습", format!("{}일", self.p.streak_now()), ORANGE, 0),
                        ("총 XP", format!("{}", self.p.xp_total), GOLD, 1),
                        ("완료한 레슨", format!("{}", self.p.lessons_done), PRIMARY, 2),
                        ("배운 단어", format!("{}", self.p.words_learned()), BLUE, 3),
                    ];
                    let cw = (col_w - 12.0) / 2.0;
                    for pair in stats.chunks(2) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            for (label, value, c, icon) in pair {
                                let (r, _) = ui.allocate_exact_size(vec2(cw, 76.0), Sense::hover());
                                let p = ui.painter();
                                p.rect_filled(r, CornerRadius::same(14), surface());
                                p.rect_stroke(r, CornerRadius::same(14), Stroke::new(2.0, border()), StrokeKind::Inside);
                                let ic = Pos2::new(r.left() + 34.0, r.center().y);
                                match icon {
                                    0 => paint_flame(p, ic, 30.0, *c),
                                    1 => paint_bolt(p, ic, 30.0, *c),
                                    2 => paint_check(p, ic, 26.0, *c, 6.0),
                                    _ => {
                                        p.text(ic, Align2::CENTER_CENTER, "ก", bold(28.0), *c);
                                    }
                                }
                                p.text(Pos2::new(r.left() + 64.0, r.center().y - 11.0), Align2::LEFT_CENTER, value, bold(21.0), fg());
                                p.text(Pos2::new(r.left() + 64.0, r.center().y + 14.0), Align2::LEFT_CENTER, *label, font(13.5), muted());
                            }
                        });
                        ui.add_space(4.0);
                    }
                    ui.add_space(14.0);
                    // weekly chart
                    ui.label(RichText::new("이번 주 XP").font(bold(21.0)));
                    card_frame().show(ui, |ui| {
                        ui.set_width(col_w - 40.0);
                        let (r, _) = ui.allocate_exact_size(vec2(col_w - 40.0, 160.0), Sense::hover());
                        let p = ui.painter();
                        let t = today();
                        let vals: Vec<u32> = (0..7).map(|k| *self.p.xp_by_day.get(&(t - 6 + k)).unwrap_or(&0)).collect();
                        let maxv = vals.iter().copied().max().unwrap_or(0).max(self.p.daily_goal) as f32;
                        let bw = r.width() / 7.0;
                        let names = ["월", "화", "수", "목", "금", "토", "일"];
                        let base_wd = chrono::Local::now().date_naive().weekday().num_days_from_monday() as i32;
                        let gy = r.bottom() - 24.0 - (self.p.daily_goal as f32 / maxv) * (r.height() - 50.0);
                        p.line_segment([Pos2::new(r.left(), gy), Pos2::new(r.right(), gy)], Stroke::new(1.5, lighten(GOLD, 0.4)));
                        p.text(Pos2::new(r.right(), gy - 4.0), Align2::RIGHT_BOTTOM, "목표", font(11.0), Color32::from_rgb(220, 160, 0));
                        for (k, v) in vals.iter().enumerate() {
                            let x = r.left() + bw * (k as f32 + 0.5);
                            let h = (*v as f32 / maxv) * (r.height() - 50.0);
                            let bar = Rect::from_min_max(Pos2::new(x - bw * 0.25, r.bottom() - 24.0 - h.max(4.0)), Pos2::new(x + bw * 0.25, r.bottom() - 24.0));
                            p.rect_filled(bar, CornerRadius::same(6), if k == 6 { PRIMARY } else { lighten(PRIMARY, 0.45) });
                            if *v > 0 {
                                p.text(bar.center_top() - vec2(0.0, 4.0), Align2::CENTER_BOTTOM, v.to_string(), bold(12.0), fg());
                            }
                            let wd = (base_wd - 6 + k as i32).rem_euclid(7) as usize;
                            p.text(Pos2::new(x, r.bottom() - 8.0), Align2::CENTER_CENTER, names[wd], if k == 6 { bold(13.0) } else { font(13.0) }, if k == 6 { fg() } else { muted() });
                        }
                    });
                    ui.add_space(14.0);
                    // daily goal
                    ui.label(RichText::new("하루 목표").font(bold(21.0)));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let bw = (col_w - 30.0) / 4.0;
                        for (g, label) in [(10, "가볍게"), (20, "보통"), (30, "열심히"), (50, "집중")] {
                            let txt = if col_w < 560.0 { format!("{}XP", g) } else { format!("{} · {}XP", label, g) };
                            let r = if self.p.daily_goal == g {
                                button3d(ui, &txt, vec2(bw, 48.0), ORANGE, true)
                            } else {
                                outline_button(ui, &txt, vec2(bw, 48.0), fg())
                            };
                            if r.clicked() {
                                self.p.daily_goal = g;
                            }
                        }
                    });
                    ui.add_space(14.0);
                    // achievements
                    ui.label(RichText::new("업적").font(bold(21.0)));
                    let unit1 = self.p.done[0] as u32;
                    let ach = [
                        ("첫걸음", "레슨 1개 끝내기", self.p.lessons_done.min(1), 1, PRIMARY),
                        ("불꽃 시작", "3일 연속 학습하기", self.p.best_streak.min(3), 3, ORANGE),
                        ("단어 수집가", "단어 30개 배우기", (self.p.words_learned() as u32).min(30), 30, BLUE),
                        ("XP 사냥꾼", "XP 200 모으기", self.p.xp_total.min(200), 200, GOLD),
                        ("완벽주의자", "실수 없이 레슨 3개 끝내기", self.p.perfect_lessons.min(3), 3, Color32::from_rgb(140, 100, 230)),
                        ("첫 단원 정복", "단원 1의 모든 레슨 끝내기", unit1.min(5), 5, GREEN),
                    ];
                    card_frame().inner_margin(Margin::symmetric(14, 8)).show(ui, |ui| {
                        ui.set_width(col_w - 30.0);
                        for (i, (name, desc, have, need, c)) in ach.iter().enumerate() {
                            let compact_ach = col_w < 560.0;
                            let (r, _) = ui.allocate_exact_size(vec2(col_w - 30.0, if compact_ach { 92.0 } else { 68.0 }), Sense::hover());
                            let p = ui.painter();
                            if i > 0 {
                                p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, border()));
                            }
                            let done = have >= need;
                            let badge = Rect::from_center_size(Pos2::new(r.left() + 30.0, r.center().y), vec2(48.0, 50.0));
                            p.rect_filled(badge, CornerRadius::same(14), if done { *c } else { hover() });
                            paint_star(p, badge.center(), 14.0, if done { WHITE } else { faint() });
                            p.text(Pos2::new(r.left() + 68.0, r.center().y - 14.0), Align2::LEFT_CENTER, *name, bold(16.0), fg());
                            p.text(Pos2::new(r.left() + 68.0, r.center().y + 6.0), Align2::LEFT_CENTER, *desc, font(13.0), muted());
                            let bar = if compact_ach {
                                Rect::from_min_size(Pos2::new(r.left() + 68.0, r.center().y + 20.0), vec2(r.width() - 68.0 - 64.0, 12.0))
                            } else {
                                Rect::from_min_size(Pos2::new(r.right() - 190.0, r.center().y - 7.0), vec2(130.0, 14.0))
                            };
                            progress_bar(p, bar, *have as f32 / *need as f32, *c);
                            p.text(Pos2::new(r.right() - 8.0, if compact_ach { r.center().y + 26.0 } else { r.center().y }), Align2::RIGHT_CENTER, format!("{}/{}", have, need), bold(13.0), muted());
                        }
                    });
                    ui.add_space(14.0);
                    ui.label(RichText::new("설정").font(bold(21.0)));
                    ui.label(RichText::new("말투 (문장 끝과 '나'를 어떻게 말할지)").font(font(14.0)).color(muted()));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let bw = (col_w - 10.0) / 2.0;
                        for (g, label) in if col_w < 560.0 { [(1u8, "남성 · ครับ"), (2, "여성 · ค่ะ")] } else { [(1u8, "남성 말투 · ครับ 크랍 / ผม 폼"), (2, "여성 말투 · ค่ะ 카 / ฉัน 찬")] } {
                            let r = if self.p.gender == g {
                                button3d(ui, label, vec2(bw, 48.0), PRIMARY, true)
                            } else {
                                outline_button(ui, label, vec2(bw, 48.0), fg())
                            };
                            if r.clicked() {
                                self.p.gender = g;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("단계 건너뛰기 (앞 단계를 모두 끝낸 것으로 해요)").font(font(14.0)).color(muted()));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let bw = (col_w - 10.0) / 2.0;
                        for (start, label) in [(6usize, "초급부터"), (12, "중급부터")] {
                            let reached = self.p.done.iter().take(start).all(|d| *d as usize == LESSONS_PER_UNIT);
                            let r = if reached { outline_button(ui, &format!("{} (완료)", label), vec2(bw, 46.0), muted()) } else { outline_button(ui, label, vec2(bw, 46.0), fg()) };
                            if r.clicked() && !reached {
                                self.p.skip_to(start);
                                self.toast(&format!("{}로 이동했어요", label.trim_end_matches("부터")), now);
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(RichText::new("화면 밝기").font(font(14.0)).color(muted()));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let bw = (col_w - 20.0) / 3.0;
                        for (t, label) in if col_w < 560.0 { [(0u8, "자동"), (1, "라이트"), (2, "다크")] } else { [(0u8, "컴퓨터 설정 따르기"), (1, "라이트 모드"), (2, "다크 모드")] } {
                            let r = if self.p.theme == t {
                                button3d(ui, label, vec2(bw, 48.0), Color32::from_rgb(140, 100, 230), true)
                            } else {
                                outline_button(ui, label, vec2(bw, 48.0), fg())
                            };
                            if r.clicked() {
                                self.p.theme = t;
                            }
                        }
                    });
                    ui.add_space(10.0);
                    // reset
                    if !self.confirm_reset {
                        if outline_button(ui, "학습 기록 초기화", vec2(200.0, 46.0), RED).clicked() {
                            self.confirm_reset = true;
                        }
                    } else {
                        ui.label(RichText::new("모든 진도와 XP가 지워져요. 정말 초기화할까요?").font(font(15.0)).color(RED));
                        ui.horizontal(|ui| {
                            if button3d(ui, "초기화", vec2(140.0, 46.0), RED, true).clicked() {
                                let muted = self.p.muted;
                                self.p = Progress::default();
                                self.p.muted = muted;
                                self.confirm_reset = false;
                                self.toast("학습 기록을 초기화했어요", now);
                            }
                            if outline_button(ui, "취소", vec2(140.0, 46.0), fg()).clicked() {
                                self.confirm_reset = false;
                            }
                        });
                    }
                    ui.add_space(14.0);
                    self.about_sections(ui, col_w, now);
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("앱").font(bold(21.0)));
                        ui.label(RichText::new(format!("버전 {}", APP_VERSION)).font(font(14.0)).color(muted()));
                    });
                    // iPhone apps do not close themselves (Apple's rules): no quit button there.
                    if cfg!(target_os = "ios") {
                    } else if !self.confirm_exit {
                        if button3d(ui, "앱 종료하기", vec2(col_w.min(320.0), 50.0), RED, true).clicked() {
                            self.confirm_exit = true;
                        }
                    } else {
                        ui.label(RichText::new("앱을 종료할까요? 진도는 자동으로 저장돼요.").font(font(15.0)).color(muted()));
                        ui.horizontal(|ui| {
                            if button3d(ui, "종료", vec2(140.0, 46.0), RED, true).clicked() {
                                self.quit_app(ui.ctx());
                            }
                            if outline_button(ui, "취소", vec2(140.0, 46.0), fg()).clicked() {
                                self.confirm_exit = false;
                            }
                        });
                    }
                    ui.add_space(40.0);
                });
            });
        });
    }

    /// Close the app (progress is saved on the way out).
    fn quit_app(&mut self, ctx: &egui::Context) {
        self.p.muted = self.audio.muted;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        // Phones: closing the window does not end the app by itself, so end it
        // a moment later, after the progress has been written.
        #[cfg(target_os = "android")]
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(900));
            std::process::exit(0);
        });
    }
}

// ---------------------------------------------------------------- welcome (first run)

impl App {
    fn choice_box(ui: &mut Ui, size: Vec2, selected: bool, title: &str, line1: &str, line2: &str, accent: Color32) -> bool {
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let p = ui.painter();
        let (fill, border) = if selected { (lighten(accent, 0.86), accent) } else if resp.hovered() { (hover(), border()) } else { (surface(), border()) };
        let face = Rect::from_min_max(rect.min, rect.max - vec2(0.0, 4.0));
        soft_shadow(p, face, CornerRadius::same(16), if selected { 1.0 } else { 0.6 });
        p.rect_filled(face, CornerRadius::same(16), fill);
        p.rect_stroke(face, CornerRadius::same(16), Stroke::new(2.0, border), StrokeKind::Inside);
        p.text(face.left_top() + vec2(20.0, 18.0), Align2::LEFT_TOP, title, bold(19.0), if selected { ink(accent, 0.8) } else { fg() });
        p.text(face.left_top() + vec2(20.0, 52.0), Align2::LEFT_TOP, line1, bold(17.0), fg());
        p.text(face.left_top() + vec2(20.0, 82.0), Align2::LEFT_TOP, line2, font(14.0), muted());
        if selected {
            let c = face.right_top() + vec2(-26.0, 26.0);
            p.circle_filled(c, 13.0, accent);
            paint_check(p, c, 16.0, WHITE, 3.5);
        }
        resp.on_hover_cursor(CursorIcon::PointingHand).clicked()
    }

    fn welcome_screen(&mut self, ui: &mut Ui) {
        egui::CentralPanel::no_frame().frame(egui::Frame::NONE.fill(surface())).show(ui, |ui| {
            let r = ui.max_rect();
            let w = 640.0f32.min(r.width() - 60.0);
            let area = Rect::from_min_max(Pos2::new(r.center().x - w / 2.0, r.top() + 20.0), Pos2::new(r.center().x + w / 2.0, r.bottom()));
            ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (mr, _) = ui.allocate_exact_size(vec2(120.0, 110.0), Sense::hover());
                        paint_mascot(ui.painter(), mr.center(), 38.0, true);
                        ui.vertical(|ui| {
                            ui.add_space(22.0);
                            ui.label(RichText::new("사왓디에 오신 걸 환영해요!").font(bold(28.0)).color(PRIMARY));
                            ui.label(RichText::new("시작하기 전에 말투와 단계를 골라 주세요.").font(font(15.0)).color(muted()));
                        });
                    });
                    ui.add_space(18.0);
                    ui.label(RichText::new("1. 어떤 말투로 배울까요?").font(bold(20.0)));
                    ui.label(RichText::new("태국어는 말하는 사람의 성별에 따라 문장 끝과 '나'가 달라져요.").font(font(14.0)).color(muted()));
                    ui.add_space(4.0);
                    let bw = (w - 12.0) / 2.0;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        if Self::choice_box(ui, vec2(bw, 116.0), self.welcome_gender == 1, "남성 말투", "ครับ 크랍 · ผม 폼", "สวัสดีครับ (싸왓디 크랍)", BLUE) {
                            self.welcome_gender = 1;
                        }
                        if Self::choice_box(ui, vec2(bw, 116.0), self.welcome_gender == 2, "여성 말투", "ค่ะ 카 · ฉัน 찬", "สวัสดีค่ะ (싸왓디 카)", Color32::from_rgb(235, 90, 150)) {
                            self.welcome_gender = 2;
                        }
                    });
                    ui.add_space(18.0);
                    ui.label(RichText::new("2. 어느 단계부터 시작할까요?").font(bold(20.0)));
                    ui.label(RichText::new("이미 아는 단계는 건너뛸 수 있어요. 나중에 프로필에서 바꿀 수도 있어요.").font(font(14.0)).color(muted()));
                    ui.add_space(4.0);
                    let lv = [
                        (0usize, "기초부터", "인사와 꼭 필요한 말", PRIMARY),
                        (6, "초급부터", "생활 속 표현 (기초는 건너뛰기)", ORANGE),
                        (12, "중급부터", "시제·이유·비교 (앞 단계는 건너뛰기)", Color32::from_rgb(140, 100, 230)),
                    ];
                    for (start, title, desc, c) in lv {
                        if Self::choice_box(ui, vec2(w, 108.0), self.welcome_level == start, title, desc, "", c) {
                            self.welcome_level = start;
                        }
                        ui.add_space(6.0);
                    }
                    ui.add_space(12.0);
                    card_frame().show(ui, |ui| {
                        ui.set_width(w - 40.0);
                        ui.horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
                            paint_note(ui.painter(), r.center(), 30.0, RED);
                            ui.vertical(|ui| {
                                ui.label(RichText::new("틀려도 괜찮아요").font(bold(17.0)));
                                ui.label(
                                    RichText::new("틀린 문제는 '오답 노트'에 모였다가 다시 나와요. 두 번 연달아 맞히면 졸업해요.")
                                        .font(font(14.0))
                                        .color(muted()),
                                );
                            });
                        });
                    });
                    ui.add_space(26.0);
                    let ready = self.welcome_gender != 0;
                    if button3d(ui, if ready { "시작하기" } else { "말투를 골라 주세요" }, vec2(w, 54.0), PRIMARY, ready).clicked() {
                        self.p.gender = self.welcome_gender;
                        self.p.skip_to(self.welcome_level);
                    }
                    ui.add_space(20.0);
                });
            });
        });
    }
}

// ---------------------------------------------------------------- lesson screen

impl App {
    fn lesson_screen(&mut self, ui: &mut Ui, now: f64) {
        let Some(mut run) = self.run.take() else { return };
        let mut quit = false;
        let mut finished = false;

        // ---- header
        egui::Panel::top("lesson_top")
            .exact_size(84.0)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(surface()))
            .show(ui, |ui| {
                let full = ui.max_rect();
                let w = 900.0f32.min(full.width() - 60.0);
                let row = Rect::from_center_size(full.center(), vec2(w, 40.0));
                // close
                let close = Rect::from_center_size(Pos2::new(row.left() + 16.0, row.center().y), vec2(36.0, 36.0));
                let resp = ui.interact(close, ui.scope_id().with("close"), Sense::click());
                let p = ui.painter();
                paint_cross(p, close.center(), 22.0, if resp.hovered() { fg() } else { muted() }, 3.0);
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() {
                    run.confirm_quit = true;
                }
                // progress bar
                let total = run.queue.len().max(1);
                let done = run.pos + if run.feedback == Some(true) { 1 } else { 0 };
                let bar = Rect::from_min_max(Pos2::new(row.left() + 52.0, row.center().y - 6.0), Pos2::new(row.right() - 90.0, row.center().y + 6.0));
                let p = ui.painter();
                progress_bar(p, bar, done as f32 / total as f32, PRIMARY);
                if run.combo >= 3 {
                    p.text(bar.center_top() - vec2(0.0, 6.0), Align2::CENTER_BOTTOM, format!("{}연속 정답!", run.combo), bold(13.0), ORANGE);
                }
                // what kind of lesson, and how many missed so far
                let hc = Pos2::new(row.right() - 62.0, row.center().y);
                let (label, c) = match run.kind {
                    LessonKind::Practice => ("복습".to_string(), BLUE),
                    LessonKind::Mistakes => ("오답".to_string(), RED),
                    LessonKind::Letters | LessonKind::Spelling => ("글자".to_string(), ORANGE),
                    LessonKind::Vowels => ("모음".to_string(), ORANGE),
                    LessonKind::ConsonantClass => ("계급".to_string(), ORANGE),
                    LessonKind::ToneRules => ("성조".to_string(), ORANGE),
                    LessonKind::Finals => ("받침".to_string(), ORANGE),
                    LessonKind::Reading | LessonKind::SentenceReading | LessonKind::Signs => ("읽기".to_string(), ORANGE),
                    LessonKind::Digits => ("숫자".to_string(), ORANGE),
                    LessonKind::Dialogue => ("대화".to_string(), BLUE),
                    _ => (format!("{}", run.missed.len()), if run.missed.is_empty() { muted() } else { RED }),
                };
                paint_note(p, hc, 26.0, c);
                p.text(hc + vec2(20.0, 0.0), Align2::LEFT_CENTER, label, bold(17.0), c);
            });

        if run.confirm_quit {
            egui::CentralPanel::no_frame().frame(egui::Frame::NONE.fill(surface())).show(ui, |ui| {
                let r = ui.max_rect();
                ui.scope_builder(egui::UiBuilder::new().max_rect(Rect::from_center_size(r.center(), vec2(420.0, 360.0))), |ui| {
                    ui.vertical_centered(|ui| {
                        let (mr, _) = ui.allocate_exact_size(vec2(160.0, 130.0), Sense::hover());
                        paint_mascot(ui.painter(), mr.center(), 48.0, false);
                        ui.label(RichText::new("정말 그만할까요?").font(bold(24.0)));
                        ui.label(RichText::new("지금 그만두면 이번 레슨의 진행 상황이 사라져요.").font(font(15.0)).color(muted()));
                        ui.add_space(14.0);
                        if button3d(ui, "계속 학습하기", vec2(300.0, 50.0), PRIMARY, true).clicked() {
                            run.confirm_quit = false;
                        }
                        ui.add_space(6.0);
                        if outline_button(ui, "그만하기", vec2(300.0, 48.0), RED).clicked() {
                            quit = true;
                        }
                    });
                });
            });
            if !quit {
                self.run = Some(run);
            }
            return;
        }

        // ---- footer
        let can_check = match &run.ex().body {
            Body::Choice { .. } => run.st.selected.is_some(),
            Body::Tiles { .. } => !run.st.placed.is_empty(),
            Body::Match { .. } => false,
            Body::Flash { shadow, .. } => *shadow || run.st.flipped,
        };
        let is_match = matches!(run.ex().body, Body::Match { .. });
        let flash: Option<bool> = match &run.ex().body {
            Body::Flash { shadow, .. } => Some(*shadow),
            _ => None,
        };
        let flipped = run.st.flipped;
        let mut do_flip = false;
        let mut do_replay = false;
        let mut do_check = false;
        let mut do_continue = false;
        let mut do_skip = false;
        let (enter, backspace, digit) = ui.input(|i| {
            let mut d = None;
            let keys = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
            for (k, key) in keys.iter().enumerate() {
                if i.key_pressed(*key) {
                    d = Some(k);
                }
            }
            (i.key_pressed(Key::Enter), i.key_pressed(Key::Backspace), d)
        });
        if enter {
            if run.feedback.is_some() {
                do_continue = true;
            } else if can_check {
                do_check = true;
            } else if flash.is_some() {
                do_flip = true;
            }
        }

        let fb = run.feedback;
        let footer_fill = match fb {
            Some(true) => green_soft(),
            Some(false) => red_soft(),
            None => surface(),
        };
        let narrow_fb = fb.is_some() && ui.available_width() < 700.0;
        egui::Panel::bottom("lesson_bottom")
            .exact_size(if narrow_fb { 200.0 } else { 130.0 })
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(footer_fill))
            .show(ui, |ui| {
                let full = ui.max_rect();
                if fb.is_none() {
                    ui.painter().line_segment([full.left_top(), full.right_top()], Stroke::new(2.0, border()));
                }
                let w = 900.0f32.min(full.width() - 60.0);
                // two buttons side by side must fit even on a 320 dp wide phone
                let btn_r = ((w - 12.0) * 0.55).min(190.0);
                let btn_l = ((w - 12.0) * 0.45).min(150.0);
                let row = Rect::from_center_size(full.center(), vec2(w, if narrow_fb { 160.0 } else { 60.0 }));
                ui.scope_builder(egui::UiBuilder::new().max_rect(row), |ui| {
                    ui.horizontal_centered(|ui| match fb {
                        None if flash == Some(true) => {
                            if outline_button(ui, "다시 듣기", vec2(btn_l, 52.0), BLUE).clicked() {
                                do_replay = true;
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if button3d(ui, "따라 했어요", vec2(btn_r, 52.0), PRIMARY, true).clicked() {
                                    do_check = true;
                                }
                            });
                        }
                        None if flash == Some(false) => {
                            if !flipped {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if button3d(ui, "뜻 보기", vec2(w.min(320.0), 52.0), BLUE, true).clicked() {
                                        do_flip = true;
                                    }
                                });
                            } else {
                                if outline_button(ui, "아직 어려워요", vec2(btn_r, 52.0), RED).clicked() {
                                    do_skip = true;
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if button3d(ui, "알아요", vec2(btn_r, 52.0), PRIMARY, true).clicked() {
                                        do_check = true;
                                    }
                                });
                            }
                        }
                        None => {
                            if !is_match {
                                if outline_button(ui, "건너뛰기", vec2(btn_l, 52.0), muted()).clicked() {
                                    do_skip = true;
                                }
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if !is_match && button3d(ui, "확인", vec2(btn_r, 52.0), PRIMARY, can_check).clicked() {
                                    do_check = true;
                                }
                            });
                        }
                        Some(ok) => {
                            let c = if ok { GREEN } else { RED };
                            let title = if ok && is_match && run.st.match_wrong > 0 {
                                format!("완료! 실수 {}번", run.st.match_wrong)
                            } else if ok {
                                ["훌륭해요!", "정답이에요!", "잘했어요!", "완벽해요!"][run.pos % 4].to_string()
                            } else if flash == Some(false) {
                                "복습 노트에 넣을게요".to_string()
                            } else {
                                "정답:".to_string()
                            };
                            let detail: Option<String> = if ok && is_match && run.st.match_wrong > 0 {
                                Some("틀린 짝은 복습 목록에 넣어 둘게요.".to_string())
                            } else if let Some(note) = &run.note {
                                Some(note.clone())
                            } else if !ok && !run.ex().solution.is_empty() {
                                Some(run.ex().solution.clone())
                            } else {
                                None
                            };
                            let paint_icon = |ui: &mut Ui, size: f32| {
                                let (ir, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
                                let p = ui.painter();
                                p.circle_filled(ir.center(), size * 0.47, surface());
                                if ok {
                                    paint_check(p, ir.center(), size * 0.47, c, 6.0);
                                } else {
                                    paint_cross(p, ir.center(), size * 0.47, c, 6.0);
                                }
                            };
                            if narrow_fb {
                                // Phone: the text gets the full width, the button sits below it.
                                ui.vertical(|ui| {
                                    ui.horizontal_top(|ui| {
                                        paint_icon(ui, 48.0);
                                        ui.vertical(|ui| {
                                            ui.set_width(w - 64.0);
                                            ui.label(RichText::new(&title).font(bold(20.0)).color(ink(c, 0.8)));
                                            if let Some(d) = &detail {
                                                ui.add(egui::Label::new(RichText::new(d).font(font(15.0)).color(ink(c, 0.75))).wrap());
                                            }
                                        });
                                    });
                                    ui.add_space(10.0);
                                    if button3d(ui, "계속", vec2(w, 52.0), c, true).clicked() {
                                        do_continue = true;
                                    }
                                });
                            } else {
                                paint_icon(ui, 64.0);
                                ui.add_space(8.0);
                                ui.vertical(|ui| {
                                    ui.add_space(4.0);
                                    ui.label(RichText::new(&title).font(bold(22.0)).color(ink(c, 0.8)));
                                    if let Some(d) = &detail {
                                        ui.label(RichText::new(d).font(font(16.0)).color(ink(c, 0.75)));
                                    }
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if button3d(ui, "계속", vec2(btn_r, 52.0), c, true).clicked() {
                                        do_continue = true;
                                    }
                                });
                            }
                        }
                    });
                });
            });

        // ---- content
        egui::CentralPanel::no_frame().frame(egui::Frame::NONE.fill(surface())).show(ui, |ui| {
            let full = ui.max_rect();
            let w = 640.0f32.min(full.width() - 60.0);
            let area = Rect::from_min_max(Pos2::new(full.center().x - w / 2.0, full.top() + 24.0), Pos2::new(full.center().x + w / 2.0, full.bottom()));
            ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.exercise_ui(ui, &mut run, now, digit, backspace);
                });
            });
        });

        // ---- actions
        if do_flip {
            run.st.flipped = true;
        }
        if do_replay {
            if let Some(t) = run.ex().speak.clone() {
                self.audio.speak(&t, true);
            }
        }
        if do_skip {
            do_check = true;
            run.st.selected = None;
            run.st.placed.clear();
            self.check(&mut run, true);
        } else if do_check {
            self.check(&mut run, false);
        }
        // matching finishes on its own
        if is_match && run.feedback.is_none() && !run.st.matched_left.is_empty() && run.st.matched_left.iter().all(|m| *m) {
            run.feedback = Some(true);
            let words = run.ex().words.clone();
            for (k, w) in words.iter().enumerate() {
                if !run.st.match_bad.contains(&k) {
                    self.p.record(*w, true);
                }
            }
            if run.st.match_wrong == 0 {
                run.combo += 1;
                if !run.queue[run.pos].1 {
                    run.first_right += 1;
                }
            } else {
                run.combo = 0;
                run.mistakes += run.st.match_wrong;
            }
            self.audio.effect(true);
        }
        if do_continue && run.feedback.is_some() {
            run.pos += 1;
            run.st = ExState::default();
            run.feedback = None;
            run.note = None;
            if run.pos >= run.queue.len() {
                finished = true;
            }
        }
        let _ = do_check;
        if quit {
            self.run = None;
        } else if finished {
            self.finish(run, now);
        } else {
            self.run = Some(run);
        }
    }

    fn check(&mut self, run: &mut Run, skipped: bool) {
        let ok = if skipped {
            false
        } else {
            match &run.ex().body {
                Body::Choice { correct, .. } => run.st.selected == Some(*correct),
                Body::Tiles { tiles, accepted, joiner } => {
                    let got: Vec<&str> = run.st.placed.iter().map(|&i| tiles[i].main.as_str()).collect();
                    let got = got.join(joiner);
                    match accepted.iter().position(|a| a.join(joiner) == got) {
                        Some(0) => true,
                        Some(_) => {
                            run.note = Some(format!("이렇게도 말해요: {}", accepted[0].join(joiner)));
                            true
                        }
                        None => false,
                    }
                }
                Body::Match { .. } => true,
                Body::Flash { .. } => true,
            }
        };
        run.feedback = Some(ok);
        // Remember how each practised word went (a skip counts as "not known yet").
        let words = run.ex().words.clone();
        for w in words {
            self.p.record(w, ok);
        }
        if ok {
            run.combo += 1;
            // A right answer on the first try counts toward leaving the notebook.
            // (The retry at the end of a lesson right after a mistake does not.)
            if !run.queue[run.pos].1 {
                run.first_right += 1;
                let ex = run.ex().clone();
                let mut graduated = 0;
                if let Some((u, si)) = ex.sentence {
                    graduated += self.p.note_right(u, si, true) as usize;
                } else {
                    for (u, i) in &ex.words {
                        graduated += self.p.note_right(*u, *i, false) as usize;
                    }
                }
                run.graduated += graduated;
            }
            if let Some(t) = run.ex().solution_speak.clone() {
                self.audio.speak(&t, false);
            }
        } else {
            run.combo = 0;
            run.mistakes += 1;
            // Into the mistake notebook (a skip counts too: it comes back later).
            let ex = run.ex().clone();
            if let Some((u, si)) = ex.sentence {
                self.p.note_wrong(u, si, true);
            } else {
                for (u, i) in &ex.words {
                    self.p.note_wrong(*u, *i, false);
                }
            }
            if !ex.solution.is_empty() && !run.missed.contains(&ex.solution) {
                run.missed.push(ex.solution.clone());
            }
            if !run.queue[run.pos].1 {
                let again = run.queue[run.pos].0.clone();
                run.queue.push((again, true));
            }
        }
        self.audio.effect(ok);
    }

    fn exercise_ui(&mut self, ui: &mut Ui, run: &mut Run, now: f64, digit: Option<usize>, backspace: bool) {
        let w = ui.available_width();
        let ex = run.ex().clone();
        let locked = run.feedback.is_some();
        let retry = run.queue[run.pos].1;

        // auto speak once
        if !run.st.spoke {
            run.st.spoke = true;
            if let Some(t) = &ex.speak {
                self.audio.speak(t, false);
            }
        }

        // badge
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let (label, c) = if retry { ("다시 도전", ORANGE) } else { (ex.badge, Color32::from_rgb(140, 100, 230)) };
            let g = ui.painter().layout_no_wrap(label.into(), bold(14.0), c);
            let (r, _) = ui.allocate_exact_size(g.size() + vec2(20.0, 10.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(8), lighten(c, 0.85));
            ui.painter().galley(r.center() - g.size() / 2.0, g, c);
        });
        ui.add_space(6.0);
        ui.label(RichText::new(&ex.title).font(bold(23.0)));
        ui.add_space(16.0);

        // prompt
        if ex.listen_only {
            ui.horizontal(|ui| {
                if speaker_button(ui, 92.0, BLUE).clicked() {
                    if let Some(t) = &ex.speak {
                        self.audio.speak(t, false);
                    }
                }
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.add_space(20.0);
                    let r = outline_button(ui, "천천히 듣기", vec2(140.0, 44.0), BLUE);
                    if r.clicked() {
                        if let Some(t) = &ex.speak {
                            self.audio.speak(t, true);
                        }
                    }
                });
            });
            if !self.audio.available() {
                ui.label(RichText::new("이 컴퓨터에는 태국어 음성이 없어요.").font(font(13.0)).color(muted()));
            }
            ui.add_space(20.0);
        } else if let Some(pr) = &ex.prompt_thai {
            // On a phone the elephant leaves the bubble too little room, so it steps aside there.
            // A Thai word must never be split in the middle: the font shrinks until it fits on one line.
            let avail = ui.available_width();
            let with_mascot = avail >= 520.0;
            // a long sentence on a phone: the speaker button goes above the text, so the text gets the whole width
            let stacked = !with_mascot && text_size(ui, &pr.th, bold(22.0)).x > avail - 112.0;
            let budget = (avail - if with_mascot { 116.0 } else { 0.0 } - if stacked { 40.0 } else { 112.0 }).max(80.0);
            let mut th_size = 30.0f32;
            while th_size > 15.0 && text_size(ui, &pr.th, bold(th_size)).x > budget {
                th_size -= 1.0;
            }
            ui.horizontal(|ui| {
                // mascot with speech bubble
                if with_mascot {
                    let (mr, _) = ui.allocate_exact_size(vec2(110.0, 120.0), Sense::hover());
                    paint_mascot(ui.painter(), mr.center() + vec2(0.0, 6.0), 36.0, false);
                    ui.add_space(6.0);
                }
                let frame = egui::Frame::NONE
                    .stroke(Stroke::new(2.0, border()))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(Margin::symmetric(16, 12));
                let resp = frame.show(ui, |ui| {
                    let layout = if stacked { Layout::top_down(Align::Min) } else { Layout::left_to_right(Align::Min) };
                    ui.with_layout(layout, |ui| {
                        if speaker_button(ui, 44.0, BLUE).clicked() {
                            if let Some(t) = &ex.speak {
                                self.audio.speak(t, false);
                            }
                        }
                        ui.add_space(6.0);
                        ui.vertical(|ui| {
                            ui.add(egui::Label::new(RichText::new(&pr.th).font(bold(th_size)).color(fg())).wrap_mode(if text_size(ui, &pr.th, bold(th_size)).x <= budget { egui::TextWrapMode::Extend } else { egui::TextWrapMode::Wrap }));
                            if !pr.pron.is_empty() {
                                ui.label(RichText::new(&pr.pron).font(font(15.0)).color(muted()));
                            }
                            if !pr.tones.is_empty() {
                                ui.label(RichText::new(format!("성조  {}", pr.tones)).font(bold(15.0)).color(TONE))
                                    .on_hover_text("→ 평성   ↓ 저성   ↘ 하강   ↑ 고성   ↗ 상승");
                            }
                        });
                    });
                });
                // bubble tail
                if with_mascot {
                    let r = resp.response.rect;
                    let tip = Pos2::new(r.left() - 12.0, r.center().y + 6.0);
                    let p = ui.painter();
                    p.add(egui::Shape::convex_polygon(vec![Pos2::new(r.left() + 1.0, r.center().y - 4.0), tip, Pos2::new(r.left() + 1.0, r.center().y + 14.0)], surface(), Stroke::NONE));
                    p.line_segment([Pos2::new(r.left(), r.center().y - 4.0), tip], Stroke::new(2.0, border()));
                    p.line_segment([tip, Pos2::new(r.left(), r.center().y + 14.0)], Stroke::new(2.0, border()));
                }
            });
            ui.add_space(18.0);
        } else if let Some(ko) = &ex.prompt_ko {
            let avail = ui.available_width();
            let with_mascot = avail >= 520.0;
            let text_w = (avail - if with_mascot { 116.0 } else { 0.0 } - 40.0).max(120.0);
            ui.horizontal(|ui| {
                if with_mascot {
                    let (mr, _) = ui.allocate_exact_size(vec2(110.0, 110.0), Sense::hover());
                    paint_mascot(ui.painter(), mr.center(), 34.0, false);
                    ui.add_space(6.0);
                }
                egui::Frame::NONE
                    .stroke(Stroke::new(2.0, border()))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(Margin::symmetric(18, 14))
                    .show(ui, |ui| {
                        ui.set_max_width(text_w);
                        ui.add(egui::Label::new(RichText::new(ko).font(bold(21.0))).wrap());
                    });
            });
            ui.add_space(18.0);
        }

        match &ex.body {
            Body::Choice { options, correct } => {
                if !locked {
                    if let Some(d) = digit {
                        if d < options.len() {
                            run.st.selected = Some(d);
                            if options[d].thai {
                                self.audio.speak(&options[d].main, false);
                            }
                        }
                    }
                }
                let two_cols = options.iter().all(|o| !o.thai || o.sub.is_empty()) && w > 500.0;
                let card_h = if options.iter().any(|o| o.thai && !o.sub.is_empty()) { 76.0 } else { 62.0 };
                let cols = if two_cols { 2 } else { 1 };
                let cw = (w - 12.0 * (cols as f32 - 1.0)) / cols as f32;
                for chunk in (0..options.len()).collect::<Vec<_>>().chunks(cols) {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        for &i in chunk {
                            let look = if locked {
                                if i == *correct && run.feedback == Some(false) {
                                    Look::Right
                                } else if Some(i) == run.st.selected {
                                    if i == *correct { Look::Right } else { Look::Wrong }
                                } else {
                                    Look::Normal
                                }
                            } else if Some(i) == run.st.selected {
                                Look::Selected
                            } else {
                                Look::Normal
                            };
                            let r = choice_card(ui, i + 1, &options[i], vec2(cw, card_h), look, !locked);
                            if r.clicked() && !locked {
                                run.st.selected = Some(i);
                                if options[i].thai {
                                    self.audio.speak(&options[i].main, false);
                                }
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
            }
            Body::Tiles { tiles, .. } => {
                if !locked {
                    if backspace {
                        run.st.placed.pop();
                    }
                    if let Some(d) = digit {
                        if d < tiles.len() && !run.st.placed.contains(&d) {
                            run.st.placed.push(d);
                            if tiles[d].thai {
                                self.audio.speak(&tiles[d].main, false);
                            }
                        }
                    }
                }
                // answer area with lines
                let line_h = if tiles.iter().any(|t| t.thai) { 76.0 } else { 60.0 };
                let (area, _) = ui.allocate_exact_size(vec2(w, line_h * 2.0 + 10.0), Sense::hover());
                let p = ui.painter();
                for k in 1..=2 {
                    let y = area.top() + line_h * k as f32 + 2.0;
                    p.line_segment([Pos2::new(area.left(), y), Pos2::new(area.right(), y)], Stroke::new(2.0, border()));
                }
                let mut remove: Option<usize> = None;
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area.shrink2(vec2(0.0, 6.0))));
                {
                    let ui = &mut child;
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(8.0, line_h - tiles.iter().map(|t| tile_size(ui, t).y).fold(0.0, f32::max) + 0.0);
                        for (k, &ti) in run.st.placed.iter().enumerate() {
                            let look = match run.feedback {
                                Some(true) => Look::Right,
                                Some(false) => Look::Wrong,
                                None => Look::Normal,
                            };
                            if tile(ui, &tiles[ti], look).clicked() && !locked {
                                remove = Some(k);
                            }
                        }
                    });
                }
                if let Some(k) = remove {
                    run.st.placed.remove(k);
                }
                ui.add_space(28.0);
                // tile bank
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(10.0, 12.0);
                    for (i, t) in tiles.iter().enumerate() {
                        let used = run.st.placed.contains(&i);
                        let look = if used { Look::Ghost } else if locked { Look::Faded } else { Look::Normal };
                        if tile(ui, t, look).clicked() && !used && !locked {
                            run.st.placed.push(i);
                            if t.thai {
                                self.audio.speak(&t.main, false);
                            }
                        }
                    }
                });
                ui.add_space(10.0);
                ui.label(RichText::new("타일을 눌러 문장을 만들고, 다시 누르면 빠져요. (숫자 키, 지우기 키도 쓸 수 있어요)").font(font(12.5)).color(muted()));
            }
            Body::Flash { th, pron, tones, ko, shadow } => {
                let big = if *shadow { 30.0 } else { 46.0 };
                let height = if *shadow { if notify::rec_supported() { 190.0 } else { 300.0 } } else { 280.0 };
                let (rect, resp) = ui.allocate_exact_size(vec2(w, height), if *shadow || locked { Sense::hover() } else { Sense::click() });
                let back = run.st.flipped && !*shadow;
                let p = ui.painter();
                let fill = if back { primary_soft() } else { surface() };
                soft_shadow(p, rect, CornerRadius::same(22), 0.8);
                p.rect_filled(rect, CornerRadius::same(22), fill);
                p.rect_stroke(rect, CornerRadius::same(22), Stroke::new(2.0, if back { PRIMARY } else { border() }), StrokeKind::Inside);
                let cx = rect.center().x;
                if back {
                    p.text(Pos2::new(cx, rect.top() + 44.0), Align2::CENTER_CENTER, th, bold(30.0), muted());
                    p.text(Pos2::new(cx, rect.center().y), Align2::CENTER_CENTER, ko, bold(32.0), ink(PRIMARY, 0.7));
                    p.text(Pos2::new(cx, rect.bottom() - 30.0), Align2::CENTER_CENTER, "눌러서 다시 뒤집기", font(13.0), muted());
                } else {
                    let g = p.layout(th.clone(), bold(big), fg(), rect.width() - 40.0);
                    let top = rect.top() + if *shadow && notify::rec_supported() { 60.0 } else { 70.0 };
                    p.galley(Pos2::new(cx - g.size().x / 2.0, top), g.clone(), fg());
                    let mut y = top + g.size().y + 14.0;
                    paint_pron(p, Pos2::new(cx, y + 8.0), true, pron, tones, 17.0);
                    y += 34.0;
                    if *shadow {
                        let g2 = p.layout(ko.clone(), font(17.0), muted(), rect.width() - 40.0);
                        p.galley(Pos2::new(cx - g2.size().x / 2.0, y + 6.0), g2, muted());
                    } else {
                        p.text(Pos2::new(cx, rect.bottom() - 30.0), Align2::CENTER_CENTER, if locked { "" } else { "카드를 눌러 뜻 보기" }, font(13.0), muted());
                    }
                }
                if resp.on_hover_cursor(CursorIcon::PointingHand).clicked() && !locked && !*shadow {
                    run.st.flipped = !run.st.flipped;
                }
                // speaker on the card
                let sp = Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + if *shadow && notify::rec_supported() { 34.0 } else { 36.0 }), vec2(48.0, 52.0));
                if !back {
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(sp));
                    if speaker_button(&mut child, 44.0, BLUE).clicked() {
                        self.audio.speak(th, false);
                    }
                }
                if *shadow {
                    ui.add_space(if notify::rec_supported() { 8.0 } else { 14.0 });
                    if notify::rec_supported() {
                        // each card starts without a recording
                        if self.rec_key != run.pos {
                            if self.rec != RecUi::Idle {
                                notify::rec_clear();
                            }
                            self.rec = RecUi::Idle;
                            self.rec_mine.clear();
                            self.rec_native.clear();
                            self.rec_key = run.pos;
                        }
                        if let RecUi::Recording(t0) = self.rec {
                            if now - t0 > 12.0 {
                                self.finish_recording(th);
                            }
                        }
                        ui.add_space(2.0);
                        ui.horizontal_wrapped(|ui| match self.rec {
                            RecUi::Idle => {
                                if button3d(ui, "● 녹음하기", vec2(150.0, 46.0), RED, true).clicked() {
                                    match notify::rec_start() {
                                        notify::RecStart::Ok => self.rec = RecUi::Recording(now),
                                        notify::RecStart::NeedPermission => self.toast("마이크 허용 후 다시 눌러 주세요", now),
                                        _ => self.toast("녹음을 시작하지 못했어요", now),
                                    }
                                }
                            }
                            RecUi::Recording(t0) => {
                                if button3d(ui, &format!("■ 멈추기 {:.0}초", now - t0), vec2(170.0, 46.0), RED, true).clicked() {
                                    self.finish_recording(th);
                                }
                            }
                            RecUi::Have => {
                                if outline_button(ui, "▶ 내 소리", vec2(110.0, 46.0), BLUE).clicked() {
                                    notify::rec_play();
                                }
                                if outline_button(ui, "▶ 원어민", vec2(110.0, 46.0), BLUE).clicked() {
                                    self.audio.speak(th, false);
                                }
                                if outline_button(ui, "다시 녹음", vec2(110.0, 46.0), fg()).clicked() {
                                    notify::rec_clear();
                                    self.rec = RecUi::Idle;
                                    self.rec_mine.clear();
                                }
                            }
                        });
                        if self.rec == RecUi::Have {
                            ui.add_space(6.0);
                            paint_pitch_compare(ui, &self.rec_native, &self.rec_mine);
                        } else if matches!(self.rec, RecUi::Idle) {
                            ui.add_space(4.0);
                            ui.label(RichText::new("녹음한 소리의 높낮이를 원어민과 겹쳐 보여 드려요 (채점은 안 해요)").font(font(12.5)).color(muted()));
                        }
                    } else {
                        ui.label(RichText::new("입 모양을 크게 하고, 들은 대로 소리 내어 읽어 보세요. 스스로 체크해요.").font(font(13.5)).color(muted()));
                    }
                }
            }
            Body::Match { left, right, right_to_left } => {
                if run.st.matched_left.is_empty() {
                    run.st.matched_left = vec![false; left.len()];
                    run.st.matched_right = vec![false; right.len()];
                }
                if let Some((_, _, t)) = run.st.wrong_pair {
                    if now - t > 0.6 {
                        run.st.wrong_pair = None;
                    }
                }
                let cw = (w - 16.0) / 2.0;
                let mut click_l = None;
                let mut click_r = None;
                for i in 0..left.len() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 16.0;
                        let wl = run.st.wrong_pair.map(|(l, _, _)| l == i).unwrap_or(false);
                        let look_l = if run.st.matched_left[i] {
                            Look::Faded
                        } else if wl {
                            Look::Wrong
                        } else if run.st.m_left == Some(i) {
                            Look::Selected
                        } else {
                            Look::Normal
                        };
                        if choice_card(ui, i + 1, &left[i], vec2(cw, 72.0), look_l, !run.st.matched_left[i]).clicked() {
                            click_l = Some(i);
                        }
                        let wr = run.st.wrong_pair.map(|(_, r, _)| r == i).unwrap_or(false);
                        let look_r = if run.st.matched_right[i] {
                            Look::Faded
                        } else if wr {
                            Look::Wrong
                        } else if run.st.m_right == Some(i) {
                            Look::Selected
                        } else {
                            Look::Normal
                        };
                        if choice_card(ui, i + 1 + left.len(), &right[i], vec2(cw, 72.0), look_r, !run.st.matched_right[i]).clicked() {
                            click_r = Some(i);
                        }
                    });
                    ui.add_space(4.0);
                }
                if let Some(d) = digit {
                    if d < left.len() {
                        if !run.st.matched_left[d] {
                            click_l = Some(d);
                        }
                    } else if d - left.len() < right.len() && !run.st.matched_right[d - left.len()] {
                        click_r = Some(d - left.len());
                    }
                }
                if let Some(i) = click_l {
                    run.st.m_left = Some(i);
                    self.audio.speak(&left[i].main, false);
                }
                if let Some(i) = click_r {
                    run.st.m_right = Some(i);
                }
                if let (Some(l), Some(r)) = (run.st.m_left, run.st.m_right) {
                    if right_to_left[r] == l {
                        run.st.matched_left[l] = true;
                        run.st.matched_right[r] = true;
                        self.audio.effect(true);
                    } else {
                        run.st.wrong_pair = Some((l, r, now));
                        run.st.match_wrong += 1;
                        for k in [l, right_to_left[r]] {
                            if !run.st.match_bad.contains(&k) {
                                run.st.match_bad.push(k);
                                if let Some(w) = ex.words.get(k) {
                                    self.p.record(*w, false);
                                    self.p.note_wrong(w.0, w.1, false);
                                    let wd = &UNITS[w.0].words[w.1];
                                    let line = format!("{} = {}", wd.th, wd.ko);
                                    if !run.missed.contains(&line) {
                                        run.missed.push(line);
                                    }
                                }
                            }
                        }
                        self.audio.effect(false);
                    }
                    run.st.m_left = None;
                    run.st.m_right = None;
                }
            }
        }
    }

    // ------------------------------------------------------------ summary

    fn summary_screen(&mut self, ui: &mut Ui, now: f64) {
        let Some(s) = self.summary.as_ref() else { return };
        let mut close = false;
        let mut again = false;
        let mut redo = false;
        let notebook_n = self.p.notebook.len();
        egui::CentralPanel::no_frame().frame(egui::Frame::NONE.fill(surface())).show(ui, |ui| {
            let r = ui.max_rect();
            let top = if s.missed.is_empty() { (r.height() - 600.0).max(0.0) / 2.0 } else { 10.0 };
            let sw = (r.width() - 32.0).min(560.0);
            let area = Rect::from_min_max(Pos2::new(r.center().x - sw / 2.0, r.top() + top), Pos2::new(r.center().x + sw / 2.0, r.bottom()));
            ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    let (mr, _) = ui.allocate_exact_size(vec2(220.0, if s.missed.is_empty() { 170.0 } else { 140.0 }), Sense::hover());
                    let p = ui.painter();
                    // confetti
                    for k in 0..14 {
                        let a = k as f32 * 0.9 + now as f32 * 0.6;
                        let rad = 80.0 + (k % 3) as f32 * 12.0;
                        let c = [PRIMARY, ORANGE, BLUE, GOLD, RED][k % 5];
                        let pos = mr.center() + vec2(a.cos() * rad, a.sin() * rad * 0.6);
                        p.rect_filled(Rect::from_center_size(pos, vec2(8.0, 8.0)), CornerRadius::same(2), c);
                    }
                    paint_mascot(p, mr.center(), 56.0, true);
                    let title = if s.practice {
                        if s.mistake_review { "오답 복습 완료!" } else { "연습 완료!" }
                    } else if s.unit_finished.is_some() {
                        "단원 완료! 축하해요"
                    } else if s.review {
                        "복습 완료!"
                    } else {
                        "레슨 완료!"
                    };
                    ui.label(RichText::new(title).font(bold(32.0)).color(GOLD));
                    if let Some(u) = s.unit_finished {
                        ui.label(RichText::new(format!("“{}” 단원을 모두 끝냈어요", UNITS[u].title)).font(font(16.0)).color(muted()));
                    }
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 14.0;
                        let boxes = [
                            ("획득 XP", format!("+{}", s.xp), GOLD),
                            ("정확도", format!("{}%", s.accuracy), PRIMARY),
                            ("걸린 시간", format!("{}:{:02}", s.secs / 60, s.secs % 60), BLUE),
                        ];
                        let bw = ((sw - 28.0) / 3.0).min(160.0);
                        let total = bw * 3.0 + 28.0;
                        ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
                        for (label, value, c) in boxes {
                            let (r, _) = ui.allocate_exact_size(vec2(bw, 100.0), Sense::hover());
                            let p = ui.painter();
                            p.rect_filled(r, CornerRadius::same(14), c);
                            let inner = Rect::from_min_max(r.min + vec2(3.0, 32.0), r.max - vec2(3.0, 3.0));
                            p.rect_filled(inner, CornerRadius::same(12), surface());
                            p.text(Pos2::new(r.center().x, r.top() + 16.0), Align2::CENTER_CENTER, label, bold(14.0), WHITE);
                            p.text(inner.center(), Align2::CENTER_CENTER, value, bold(26.0), ink(c, 0.85));
                        }
                    });
                    ui.add_space(14.0);
                    if let Some(st) = s.streak_up {
                        ui.horizontal(|ui| {
                            let g = ui.painter().layout_no_wrap(format!("{}일 연속 학습 중!", st), bold(18.0), ORANGE);
                            let tw = g.size().x + 34.0;
                            ui.add_space(((ui.available_width() - tw) / 2.0).max(0.0));
                            let (r, _) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::hover());
                            paint_flame(ui.painter(), r.center(), 28.0, ORANGE);
                            let (r, _) = ui.allocate_exact_size(g.size(), Sense::hover());
                            ui.painter().galley(r.min, g, ORANGE);
                        });
                    }
                    if s.graduated > 0 {
                        ui.add_space(8.0);
                        ui.label(RichText::new(format!("오답 노트에서 {}개가 졸업했어요!", s.graduated)).font(bold(17.0)).color(GREEN));
                    }
                    if !s.missed.is_empty() {
                        ui.add_space(14.0);
                        egui::Frame::NONE
                            .fill(red_soft())
                            .corner_radius(CornerRadius::same(14))
                            .inner_margin(Margin::symmetric(18, 14))
                            .show(ui, |ui| {
                                ui.set_width(sw - 36.0);
                                ui.with_layout(Layout::top_down(Align::LEFT), |ui| {
                                    ui.horizontal(|ui| {
                                        let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                                        paint_note(ui.painter(), r.center(), 22.0, RED);
                                        ui.label(RichText::new(format!("틀린 문제 {}개 · 오답 노트에 담았어요", s.missed.len())).font(bold(if sw < 460.0 { 14.0 } else { 16.0 })).color(ink(RED, 0.8)));
                                    });
                                    for line in s.missed.iter().take(6) {
                                        ui.label(RichText::new(format!("•  {}", line)).font(font(15.0)).color(fg()));
                                    }
                                    if s.missed.len() > 6 {
                                        ui.label(RichText::new(format!("그 밖에 {}개 더", s.missed.len() - 6)).font(font(13.0)).color(muted()));
                                    }
                                });
                            });
                    }
                    ui.add_space(20.0);
                    if button3d(ui, "계속", vec2(sw.min(320.0), 54.0), PRIMARY, true).clicked() {
                        close = true;
                    }
                    ui.add_space(6.0);
                    if notebook_n > 0 {
                        if outline_button(ui, &format!("틀린 문제 바로 다시 풀기 ({})", notebook_n.min(12)), vec2(sw.min(320.0), 48.0), RED).clicked() {
                            redo = true;
                        }
                    } else if !s.practice && outline_button(ui, "연습 한 번 더", vec2(sw.min(320.0), 48.0), BLUE).clicked() {
                        again = true;
                    }
                    ui.add_space(20.0);
                });
                });
            });
        });
        if ui.input(|i| i.key_pressed(Key::Enter)) {
            close = true;
        }
        if redo {
            self.start(LessonKind::Mistakes, now);
        } else if again {
            self.start(LessonKind::Practice, now);
        } else if close {
            self.summary = None;
            self.tab = Tab::Learn;
        }
    }
}

#[allow(dead_code)]
fn unused(_: Vec2) {}

#[cfg(test)]
mod backup_tests {
    use super::*;

    #[test]
    fn backup_round_trip_keeps_progress() {
        let mut app_p = Progress::default();
        app_p.done[0] = 3;
        app_p.xp_total = 123;
        app_p.streak = 4;
        app_p.reminder_on = true;
        app_p.note_wrong(0, 2, false);
        let code = format!("{}\n{}", BACKUP_HEADER, ron::to_string(&app_p).unwrap());
        // pasted with surrounding blank lines, as chat apps tend to add
        let back = App::parse_backup(&format!("\n{}\n\n", code)).expect("parse");
        assert_eq!(back.done[0], 3);
        assert_eq!(back.xp_total, 123);
        assert_eq!(back.streak, 4);
        assert!(back.reminder_on);
        assert_eq!(back.notebook.len(), 1);
    }

    #[test]
    fn bad_backup_is_refused() {
        assert!(App::parse_backup("hello").is_err());
        assert!(App::parse_backup("SAWASDEE-BACKUP-1\n(broken").is_err());
    }
}

/// Draws the pitch shape of the native clip (blue) and of the learner's voice (orange) on one small graph.
/// Both are in semitones around each speaker's own middle pitch, stretched to the same width, so the
/// learner compares the up-and-down shape, not the voice height or speed. No verdict is given.
fn paint_pitch_compare(ui: &mut Ui, native: &[f32], mine: &[f32]) {
    let w = ui.available_width().min(560.0);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 56.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(14), surface());
    p.rect_stroke(rect, CornerRadius::same(14), Stroke::new(1.2, border()), StrokeKind::Inside);
    let plot = rect.shrink2(vec2(14.0, 16.0));
    if mine.is_empty() || native.is_empty() {
        let g = p.layout("목소리의 높낮이를 잘 잡지 못했어요. 조금 더 크게, 조용한 곳에서 다시 해 보세요".to_string(), font(12.5), muted(), rect.width() - 28.0);
        p.galley(rect.center() - g.size() / 2.0, g, muted());
        return;
    }
    // The vertical range fits the two curves (at least 4.5 semitones tall), so even a small rise or fall fills the graph.
    let all = native.iter().chain(mine.iter());
    let (lo, hi) = all.fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
    let span = (hi - lo).max(4.5);
    let mid = (hi + lo) / 2.0;
    let line = |curve: &[f32], color: Color32| {
        let pts: Vec<Pos2> = curve
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let x = plot.left() + plot.width() * i as f32 / (curve.len() - 1) as f32;
                let y = plot.center().y - ((v - mid) / span) * plot.height() * 0.92;
                Pos2::new(x, y)
            })
            .collect();
        p.add(egui::Shape::line(pts, Stroke::new(3.0, color)));
    };
    line(native, BLUE);
    line(mine, ORANGE);
    p.text(rect.left_top() + vec2(18.0, 3.0), Align2::LEFT_TOP, "● 원어민", font(11.0), BLUE);
    p.text(rect.left_top() + vec2(84.0, 3.0), Align2::LEFT_TOP, "● 내 소리", font(11.0), ORANGE);
    p.text(rect.right_top() + vec2(-18.0, 3.0), Align2::RIGHT_TOP, "위쪽 = 높은 소리", font(11.0), muted());
}
