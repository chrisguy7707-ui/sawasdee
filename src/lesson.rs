//! Builds the exercises of a lesson from the course content.

use crate::data::{tone_marks, tone_of, Sentence, EXTRA_TOKENS, TONES, UNITS};

/// Small deterministic random generator (xorshift).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

// ---------------------------------------------------------------- words, tones, speech form

/// Switch the polite ending and "I" to the learner's chosen speech form.
pub fn adapt(tok: &'static str, female: bool) -> &'static str {
    match (tok, female) {
        ("ครับ", true) => "ค่ะ",
        ("ผม", true) => "ฉัน",
        ("ค่ะ", false) => "ครับ",
        ("ฉัน", false) => "ผม",
        _ => tok,
    }
}

/// The words of the other speech form, never offered as extra tiles.
fn other_form(female: bool) -> [&'static str; 2] {
    if female {
        ["ครับ", "ผม"]
    } else {
        ["ค่ะ", "ฉัน"]
    }
}

/// (Hangul pronunciation, tone letters) of a Thai word or sentence word.
pub fn token_info(tok: &str) -> (&'static str, &'static str) {
    for u in UNITS {
        for w in u.words {
            if w.th == tok {
                return (w.pron, w.tone);
            }
        }
    }
    for (th, pron, tone) in EXTRA_TOKENS {
        if *th == tok {
            return (pron, tone);
        }
    }
    ("", "")
}

pub fn sentence_tokens(s: &Sentence, female: bool) -> Vec<&'static str> {
    s.th.iter().map(|t| adapt(t, female)).collect()
}

pub fn sentence_thai(s: &Sentence, female: bool) -> String {
    sentence_tokens(s, female).concat()
}

pub fn sentence_pron(s: &Sentence, female: bool) -> String {
    sentence_tokens(s, female).iter().map(|t| token_info(t).0).collect::<Vec<_>>().join(" ")
}

pub fn sentence_tones(s: &Sentence, female: bool) -> String {
    sentence_tokens(s, female)
        .iter()
        .map(|t| tone_marks(token_info(t).1))
        .filter(|m| !m.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Thai sentence words that are also course words: (unit, word).
fn sentence_words(s: &Sentence, female: bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for t in sentence_tokens(s, female) {
        'find: for (u, unit) in UNITS.iter().enumerate() {
            for (i, w) in unit.words.iter().enumerate() {
                if w.th == t {
                    if !out.contains(&(u, i)) {
                        out.push((u, i));
                    }
                    break 'find;
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------- exercise model

/// One answer card or word tile.
#[derive(Clone, Debug)]
pub struct Card {
    pub main: String,
    pub sub: String,
    /// Tone arrows shown next to the pronunciation.
    pub tones: String,
    pub thai: bool,
}

impl Card {
    pub fn thai(th: &str, pron: &str, tones: &str) -> Self {
        Card { main: th.into(), sub: pron.into(), tones: tones.into(), thai: true }
    }
    pub fn thai_word(u: usize, i: usize) -> Self {
        let w = &UNITS[u].words[i];
        Card::thai(w.th, w.pron, &tone_marks(w.tone))
    }
    pub fn token(t: &str) -> Self {
        let (pron, tone) = token_info(t);
        Card::thai(t, pron, &tone_marks(tone))
    }
    pub fn ko(text: &str) -> Self {
        Card { main: text.into(), sub: String::new(), tones: String::new(), thai: false }
    }
}

#[derive(Clone, Debug)]
pub enum Body {
    /// Pick one of the options.
    Choice { options: Vec<Card>, correct: usize },
    /// Arrange tiles into the answer. `accepted` lists every answer counted as right
    /// (the first is the main one).
    Tiles { tiles: Vec<Card>, accepted: Vec<Vec<String>>, joiner: &'static str },
    /// A study card: the learner reads/listens, then says whether they know it.
    /// `shadow`: listen and repeat aloud (no right or wrong), otherwise a flip card.
    Flash { th: String, pron: String, tones: String, ko: String, shadow: bool },
    /// Match Thai words with their Korean meanings.
    Match { left: Vec<Card>, right: Vec<Card>, right_to_left: Vec<usize> },
}

#[derive(Clone, Debug)]
pub struct Prompt {
    pub th: String,
    pub pron: String,
    pub tones: String,
}

#[derive(Clone, Debug)]
pub struct Exercise {
    pub badge: &'static str,
    pub title: String,
    /// Big Thai text shown in the prompt.
    pub prompt_thai: Option<Prompt>,
    /// Korean sentence or word shown in the prompt.
    pub prompt_ko: Option<String>,
    /// Text read aloud by the speaker button.
    pub speak: Option<String>,
    /// Only the speaker is shown (listening exercise).
    pub listen_only: bool,
    pub body: Body,
    /// The right answer, shown when the learner gets it wrong.
    pub solution: String,
    /// Thai text of the answer, read aloud after checking.
    pub solution_speak: Option<String>,
    /// Course words practised by this exercise: (unit, word). For matching, in left-card order.
    pub words: Vec<(usize, usize)>,
    /// The course sentence this exercise is about: (unit, sentence).
    pub sentence: Option<(usize, usize)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Meaning,
    Thai,
    Listen,
    Tone,
    Match,
    Build,
    ToKorean,
    /// Flip card for a new word.
    Flash,
    /// Listen to a sentence and repeat it aloud.
    Shadow,
    /// Fill in the missing word of a sentence.
    Fill,
    /// Build a word letter by letter.
    Spell,
}

/// Which lesson to build.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LessonKind {
    Normal { unit: usize, lesson: usize },
    /// Practice with words already learned, weakest first.
    Practice,
    /// Review of questions answered wrongly before (the mistake notebook).
    Mistakes,
    /// Alphabet tab: reading drill for the Thai consonant names.
    Letters,
    /// Alphabet tab: which sound does this vowel make.
    Vowels,
    /// Alphabet tab: build a word letter by letter (words the learner already met).
    Spelling,
    /// Alphabet tab: is this consonant a mid, high or low class letter.
    ConsonantClass,
    /// Alphabet tab: which tone does this rule give.
    ToneRules,
    /// Alphabet tab: which sound does this letter make at the end of a syllable.
    Finals,
    /// Alphabet tab: read a Thai word without any help.
    Reading,
    /// Alphabet tab: read a whole sentence and pick its meaning.
    SentenceReading,
    /// Alphabet tab: read a sign.
    Signs,
    /// Alphabet tab: Thai digits.
    Digits,
}

/// What the generator needs to know about the learner.
pub struct Ctx<'a> {
    pub tts: bool,
    pub female: bool,
    /// Words to practise, most needed first (practice only).
    pub focus: &'a [(usize, usize)],
    /// Mistake notebook entries, most needed first: (unit, word or sentence index, is_sentence).
    pub mistakes: &'a [(usize, usize, bool)],
}

fn pick_options(rng: &mut Rng, u: usize, correct: usize, make: impl Fn(usize) -> Card) -> (Vec<Card>, usize) {
    let unit = &UNITS[u];
    let mut others: Vec<usize> = (0..unit.words.len()).filter(|&i| i != correct).collect();
    rng.shuffle(&mut others);
    let mut idx = vec![correct];
    idx.extend(others.into_iter().take(3));
    rng.shuffle(&mut idx);
    let pos = idx.iter().position(|&i| i == correct).unwrap_or(0);
    (idx.into_iter().map(make).collect(), pos)
}

/// Split a Thai word into written letters: a base letter with the marks that sit on it.
pub fn clusters(word: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for ch in word.chars() {
        let u = ch as u32;
        let mark = u == 0x0E31 || (0x0E34..=0x0E3A).contains(&u) || (0x0E47..=0x0E4E).contains(&u);
        if mark && !out.is_empty() {
            out.last_mut().unwrap().push(ch);
        } else {
            out.push(ch.to_string());
        }
    }
    out
}

fn tone_card(code: char) -> Card {
    let t = tone_of(code).expect("tone");
    Card { main: format!("{}  {}", t.mark, t.name), sub: t.how.into(), tones: String::new(), thai: false }
}

/// Every answer counted as right for a Thai sentence: the polite ending and "I" may be left out.
fn thai_accepted(tokens: &[&'static str]) -> Vec<Vec<String>> {
    let base: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
    let mut out = vec![base.clone()];
    let polite = |t: &str| t == "ครับ" || t == "ค่ะ";
    let pronoun = |t: &str| t == "ผม" || t == "ฉัน";
    let mut variants = Vec::new();
    if base.len() > 2 && base.last().map(|t| polite(t)).unwrap_or(false) {
        variants.push(base[..base.len() - 1].to_vec());
    }
    if base.len() > 2 && pronoun(&base[0]) {
        variants.push(base[1..].to_vec());
        if polite(base.last().unwrap()) && base.len() > 3 {
            variants.push(base[1..base.len() - 1].to_vec());
        }
    }
    for v in variants {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

/// Every answer counted as right for a Korean sentence: listed word orders, and "저는" may be left out.
fn korean_accepted(s: &Sentence) -> Vec<Vec<String>> {
    let mut forms: Vec<Vec<String>> = vec![s.ko.iter().map(|t| t.to_string()).collect()];
    for a in s.alts {
        forms.push(a.iter().map(|t| t.to_string()).collect());
    }
    let mut out = forms.clone();
    for f in &forms {
        if f.len() > 2 && f[0] == "저는" {
            let v = f[1..].to_vec();
            if !out.contains(&v) {
                out.push(v);
            }
        }
    }
    out
}

fn make(rng: &mut Rng, kind: Kind, u: usize, word: usize, sent: usize, pool: &[(usize, usize)], ctx: &Ctx) -> Exercise {
    let unit = &UNITS[u];
    let female = ctx.female;
    let mut kind = if kind == Kind::Listen && !ctx.tts { Kind::Meaning } else { kind };
    if kind == Kind::Tone && unit.words[word].tone.chars().count() != 1 {
        kind = Kind::Meaning;
    }
    if kind == Kind::Spell {
        let n = clusters(unit.words[word].th).len();
        if !(2..=7).contains(&n) {
            kind = Kind::Thai;
        }
    }
    if kind == Kind::Fill && unit.sentences[sent].th.len() < 3 {
        kind = Kind::Build;
    }
    match kind {
        Kind::Meaning => {
            let wd = &unit.words[word];
            let (options, correct) = pick_options(rng, u, word, |i| Card::ko(unit.words[i].ko));
            Exercise {
                badge: "새 단어",
                title: "이 단어의 뜻은 무엇일까요?".into(),
                prompt_thai: Some(Prompt { th: wd.th.into(), pron: wd.pron.into(), tones: tone_marks(wd.tone) }),
                prompt_ko: None,
                speak: Some(wd.th.into()),
                listen_only: false,
                body: Body::Choice { options, correct },
                solution: format!("{} = {}", wd.th, wd.ko),
                solution_speak: None,
                words: vec![(u, word)],
                sentence: None,
            }
        }
        Kind::Thai => {
            let wd = &unit.words[word];
            let (options, correct) = pick_options(rng, u, word, |i| Card::thai_word(u, i));
            Exercise {
                badge: "단어",
                title: format!("“{}”은(는) 태국어로?", wd.ko),
                prompt_thai: None,
                prompt_ko: None,
                speak: None,
                listen_only: false,
                body: Body::Choice { options, correct },
                solution: format!("{} ({} {})", wd.th, wd.pron, tone_marks(wd.tone)),
                solution_speak: Some(wd.th.into()),
                words: vec![(u, word)],
                sentence: None,
            }
        }
        Kind::Listen => {
            let wd = &unit.words[word];
            let (options, correct) = pick_options(rng, u, word, |i| Card::thai(unit.words[i].th, "", ""));
            Exercise {
                badge: "듣기",
                title: "들리는 단어를 고르세요".into(),
                prompt_thai: None,
                prompt_ko: None,
                speak: Some(wd.th.into()),
                listen_only: true,
                body: Body::Choice { options, correct },
                solution: format!("{} ({} {}) — {}", wd.th, wd.pron, tone_marks(wd.tone), wd.ko),
                solution_speak: None,
                words: vec![(u, word)],
                sentence: None,
            }
        }
        Kind::Flash => {
            let wd = &unit.words[word];
            Exercise {
                badge: "단어 카드",
                title: "카드를 눌러 뜻을 확인하세요".into(),
                prompt_thai: None,
                prompt_ko: None,
                speak: Some(wd.th.into()),
                listen_only: false,
                body: Body::Flash { th: wd.th.into(), pron: wd.pron.into(), tones: tone_marks(wd.tone), ko: wd.ko.into(), shadow: false },
                solution: format!("{} = {}", wd.th, wd.ko),
                solution_speak: None,
                words: vec![(u, word)],
                sentence: None,
            }
        }
        Kind::Shadow => {
            let s = &unit.sentences[sent];
            Exercise {
                badge: "따라 말하기",
                title: "듣고 소리 내어 따라 해 보세요".into(),
                prompt_thai: None,
                prompt_ko: None,
                speak: Some(sentence_thai(s, female)),
                listen_only: false,
                body: Body::Flash {
                    th: sentence_thai(s, female),
                    pron: sentence_pron(s, female),
                    tones: sentence_tones(s, female),
                    ko: s.korean(),
                    shadow: true,
                },
                solution: format!("{} ({})", sentence_thai(s, female), sentence_pron(s, female)),
                solution_speak: None,
                words: sentence_words(s, female),
                sentence: Some((u, sent)),
            }
        }
        Kind::Fill => {
            let s = &unit.sentences[sent];
            let tokens = sentence_tokens(s, female);
            let candidates: Vec<usize> = (0..tokens.len()).filter(|&i| tokens[i] != "ครับ" && tokens[i] != "ค่ะ").collect();
            let blank = candidates[rng.below(candidates.len())];
            let correct_tok = tokens[blank];
            let shown: Vec<String> = tokens.iter().enumerate().map(|(i, t)| if i == blank { "＿＿＿".to_string() } else { t.to_string() }).collect();
            let avoid = other_form(female);
            let mut pool_t: Vec<&'static str> = unit
                .words
                .iter()
                .map(|w| w.th)
                .filter(|t| !tokens.contains(t) && !avoid.contains(t) && *t != "ครับ" && *t != "ค่ะ")
                .collect();
            rng.shuffle(&mut pool_t);
            let mut opts: Vec<&'static str> = vec![correct_tok];
            opts.extend(pool_t.into_iter().take(3));
            rng.shuffle(&mut opts);
            let correct = opts.iter().position(|t| *t == correct_tok).unwrap_or(0);
            Exercise {
                badge: "빈칸 채우기",
                title: "빈칸에 들어갈 말은?".into(),
                prompt_thai: Some(Prompt { th: shown.concat(), pron: s.korean(), tones: String::new() }),
                prompt_ko: None,
                speak: Some(sentence_thai(s, female)),
                listen_only: false,
                body: Body::Choice { options: opts.into_iter().map(Card::token).collect(), correct },
                solution: format!("{} ({})", sentence_thai(s, female), sentence_pron(s, female)),
                solution_speak: Some(sentence_thai(s, female)),
                words: sentence_words(s, female),
                sentence: Some((u, sent)),
            }
        }
        Kind::Spell => {
            let wd = &unit.words[word];
            let letters = clusters(wd.th);
            let mut tiles: Vec<Card> = letters.iter().map(|l| Card::thai(l, "", "")).collect();
            rng.shuffle(&mut tiles);
            Exercise {
                badge: "글자 맞추기",
                title: "글자를 순서대로 눌러 단어를 만드세요".into(),
                prompt_thai: None,
                prompt_ko: Some(format!("{}  ({})", wd.ko, wd.pron)),
                speak: Some(wd.th.into()),
                listen_only: false,
                body: Body::Tiles { tiles, accepted: vec![letters], joiner: "" },
                solution: format!("{} ({} {})", wd.th, wd.pron, tone_marks(wd.tone)),
                solution_speak: Some(wd.th.into()),
                words: vec![(u, word)],
                sentence: None,
            }
        }
        Kind::Tone => {
            let wd = &unit.words[word];
            let code = wd.tone.chars().next().unwrap_or('M');
            let mut codes: Vec<char> = TONES.iter().map(|t| t.code).filter(|c| *c != code).collect();
            rng.shuffle(&mut codes);
            codes.truncate(3);
            codes.push(code);
            rng.shuffle(&mut codes);
            let correct = codes.iter().position(|c| *c == code).unwrap_or(0);
            let t = tone_of(code).expect("tone");
            Exercise {
                badge: "성조",
                title: "이 단어는 어떤 성조일까요?".into(),
                prompt_thai: Some(Prompt { th: wd.th.into(), pron: wd.pron.into(), tones: String::new() }),
                prompt_ko: None,
                speak: Some(wd.th.into()),
                listen_only: false,
                body: Body::Choice { options: codes.into_iter().map(tone_card).collect(), correct },
                solution: format!("{} ({}) = {} {} · {}", wd.th, wd.ko, t.mark, t.name, t.how),
                solution_speak: None,
                words: vec![(u, word)],
                sentence: None,
            }
        }
        Kind::Match => {
            let mut list: Vec<(usize, usize)> = pool.to_vec();
            rng.shuffle(&mut list);
            list.truncate(5);
            // Fill up from the unit if the pool is small.
            let mut extra: Vec<usize> = (0..unit.words.len()).collect();
            rng.shuffle(&mut extra);
            for i in extra {
                if list.len() >= 5 {
                    break;
                }
                if !list.contains(&(u, i)) {
                    list.push((u, i));
                }
            }
            let left: Vec<Card> = list.iter().map(|&(a, b)| Card::thai_word(a, b)).collect();
            let mut order: Vec<usize> = (0..list.len()).collect();
            rng.shuffle(&mut order);
            let right: Vec<Card> = order.iter().map(|&k| Card::ko(UNITS[list[k].0].words[list[k].1].ko)).collect();
            Exercise {
                badge: "짝 맞추기",
                title: "짝을 맞춰 보세요".into(),
                prompt_thai: None,
                prompt_ko: None,
                speak: None,
                listen_only: false,
                body: Body::Match { left, right, right_to_left: order },
                solution: String::new(),
                solution_speak: None,
                words: list,
                sentence: None,
            }
        }
        Kind::Build => {
            let s = &unit.sentences[sent];
            let tokens = sentence_tokens(s, female);
            let avoid = other_form(female);
            // Extra tiles: words of this unit that are not in the answer and not the other speech form.
            let mut pool_t: Vec<&'static str> = unit
                .words
                .iter()
                .map(|w| w.th)
                .filter(|t| !tokens.contains(t) && !avoid.contains(t) && *t != "ครับ" && *t != "ค่ะ")
                .collect();
            rng.shuffle(&mut pool_t);
            let extra = if tokens.len() <= 2 { 3 } else { 2 };
            let mut tiles: Vec<Card> = tokens.iter().map(|t| Card::token(t)).collect();
            for t in pool_t.into_iter().take(extra) {
                tiles.push(Card::token(t));
            }
            rng.shuffle(&mut tiles);
            Exercise {
                badge: "문장 만들기",
                title: "태국어로 옮겨 보세요".into(),
                prompt_thai: None,
                prompt_ko: Some(s.korean()),
                speak: None,
                listen_only: false,
                body: Body::Tiles { tiles, accepted: thai_accepted(&tokens), joiner: "" },
                solution: format!("{} ({})", sentence_thai(s, female), sentence_pron(s, female)),
                solution_speak: Some(sentence_thai(s, female)),
                words: sentence_words(s, female),
                sentence: Some((u, sent)),
            }
        }
        Kind::ToKorean => {
            let s = &unit.sentences[sent];
            let accepted = korean_accepted(s);
            let mut pool_t: Vec<&str> = Vec::new();
            for (i, other) in unit.sentences.iter().enumerate() {
                if i != sent {
                    for t in other.ko {
                        if !s.ko.contains(t) && !pool_t.contains(t) {
                            pool_t.push(t);
                        }
                    }
                }
            }
            rng.shuffle(&mut pool_t);
            let mut tiles: Vec<Card> = s.ko.iter().map(|t| Card::ko(t)).collect();
            for t in pool_t.into_iter().take(3) {
                tiles.push(Card::ko(t));
            }
            rng.shuffle(&mut tiles);
            Exercise {
                badge: "문장 해석",
                title: "이 문장을 한국어로 옮겨 보세요".into(),
                prompt_thai: Some(Prompt { th: sentence_thai(s, female), pron: sentence_pron(s, female), tones: sentence_tones(s, female) }),
                prompt_ko: None,
                speak: Some(sentence_thai(s, female)),
                listen_only: false,
                body: Body::Tiles { tiles, accepted, joiner: " " },
                solution: s.korean(),
                solution_speak: None,
                words: sentence_words(s, female),
                sentence: Some((u, sent)),
            }
        }
    }
}

/// Build the exercises for a lesson.
pub fn build(kind: LessonKind, rng: &mut Rng, ctx: &Ctx) -> Vec<Exercise> {
    let mut out = Vec::new();
    match kind {
        LessonKind::Normal { unit, lesson } => {
            let u = &UNITS[unit];
            let nw = u.words.len();
            let ns = u.sentences.len();
            use Kind::*;
            let (words, sents, plan): (Vec<usize>, Vec<usize>, Vec<Kind>) = match lesson {
                0 => (
                    (0..5.min(nw)).collect(),
                    vec![0, 1.min(ns - 1)],
                    vec![Flash, Flash, Flash, Flash, Flash, Meaning, Meaning, Thai, Listen, Tone, Thai, Match, Fill, Build],
                ),
                1 => (
                    (5.min(nw)..nw).collect(),
                    vec![1.min(ns - 1), 2.min(ns - 1)],
                    vec![Flash, Flash, Flash, Flash, Flash, Meaning, Thai, Listen, Tone, Meaning, Match, Fill, Build, Shadow, ToKorean],
                ),
                2 => ((0..nw).collect(), (0..ns.min(4)).collect(), vec![Thai, Listen, Meaning, Build, Tone, Fill, ToKorean, Shadow, Match, Listen]),
                3 => ((0..nw).collect(), (0..ns).collect(), vec![Build, Shadow, ToKorean, Fill, Listen, Meaning, Build, Match, Fill, Thai, Shadow]),
                _ => (
                    (0..nw).collect(),
                    (0..ns).collect(),
                    vec![Meaning, Fill, Listen, ToKorean, Tone, Thai, Match, Build, Shadow, Listen, ToKorean, Tone, Fill, Build, Meaning],
                ),
            };
            let pool: Vec<(usize, usize)> = words.iter().map(|&i| (unit, i)).collect();
            // Tone questions use one-syllable words from this lesson (or the unit).
            let mut tone_words: Vec<usize> = words.iter().copied().filter(|&i| u.words[i].tone.chars().count() == 1).collect();
            if tone_words.is_empty() {
                tone_words = (0..nw).filter(|&i| u.words[i].tone.chars().count() == 1).collect();
            }
            rng.shuffle(&mut tone_words);
            let mut wq = words.clone();
            rng.shuffle(&mut wq);
            let mut sq = sents.clone();
            rng.shuffle(&mut sq);
            let (mut wi, mut si, mut ti, mut fi) = (0usize, 0usize, 0usize, 0usize);
            for k in plan {
                let mut word = wq[wi % wq.len()];
                let sent = sq[si % sq.len()];
                match k {
                    Build | ToKorean | Shadow | Fill => si += 1,
                    Flash => {
                        word = words[fi % words.len()];
                        fi += 1;
                    }
                    Spell => wi += 1,
                    Match => {}
                    Tone => {
                        if !tone_words.is_empty() {
                            word = tone_words[ti % tone_words.len()];
                        }
                        ti += 1;
                    }
                    _ => wi += 1,
                }
                // Korean tile exercise needs at least two Korean tiles.
                let k = if k == ToKorean && u.sentences[sent].ko.len() < 2 { Build } else { k };
                out.push(make(rng, k, unit, word, sent, &pool, ctx));
            }
        }
        LessonKind::Letters => {
            // Every letter still in use (the two obsolete ones are left out).
            let pool: Vec<&crate::data::Consonant> = crate::data::all_consonants().filter(|c| !crate::data::is_obsolete(c.letter)).collect();
            let mut idx: Vec<usize> = (0..pool.len()).collect();
            rng.shuffle(&mut idx);
            for &i in idx.iter().take(10) {
                let c = pool[i];
                let mut others: Vec<usize> = (0..pool.len()).filter(|&j| j != i && pool[j].name_pron != c.name_pron).collect();
                rng.shuffle(&mut others);
                let mut opts: Vec<usize> = vec![i];
                opts.extend(others.into_iter().take(3));
                rng.shuffle(&mut opts);
                let correct = opts.iter().position(|&j| j == i).unwrap_or(0);
                out.push(Exercise {
                    badge: "글자 읽기",
                    title: "이 글자의 이름은?".into(),
                    prompt_thai: Some(Prompt { th: c.letter.to_string(), pron: String::new(), tones: String::new() }),
                    prompt_ko: None,
                    speak: Some(c.name.to_string()),
                    listen_only: false,
                    body: Body::Choice {
                        options: opts.iter().map(|&j| Card::ko(&format!("{}  ({})", pool[j].name_pron, pool[j].sound))).collect(),
                        correct,
                    },
                    solution: format!("{} = {}  ({}) · {}", c.letter, c.name_pron, c.sound, c.meaning),
                    solution_speak: Some(c.name.to_string()),
                    words: vec![],
                    sentence: None,
                });
            }
        }
        LessonKind::Vowels => {
            let n = crate::data::VOWELS.len();
            let mut idx: Vec<usize> = (0..n).collect();
            rng.shuffle(&mut idx);
            for &i in idx.iter().take(10) {
                let v = &crate::data::VOWELS[i];
                let mut others: Vec<usize> = (0..n).filter(|&j| j != i && crate::data::VOWELS[j].sound != v.sound).collect();
                rng.shuffle(&mut others);
                let mut opts: Vec<usize> = vec![i];
                opts.extend(others.into_iter().take(3));
                rng.shuffle(&mut opts);
                let correct = opts.iter().position(|&j| j == i).unwrap_or(0);
                out.push(Exercise {
                    badge: "모음",
                    title: "이 모음은 어떤 소리일까요?".into(),
                    prompt_thai: Some(Prompt { th: v.form.to_string(), pron: String::new(), tones: String::new() }),
                    prompt_ko: None,
                    speak: Some(v.example.to_string()),
                    listen_only: false,
                    body: Body::Choice {
                        options: opts.iter().map(|&j| Card::ko(crate::data::VOWELS[j].sound)).collect(),
                        correct,
                    },
                    solution: format!("{} = {} · 예: {} ({})", v.form, v.sound, v.example, v.example_pron),
                    solution_speak: Some(v.example.to_string()),
                    words: vec![],
                    sentence: None,
                });
            }
        }
        LessonKind::Spelling => {
            // Words the learner has met (weakest first); at the start, words of the first units.
            let mut pool: Vec<(usize, usize)> = ctx.focus.to_vec();
            if pool.len() < 10 {
                for u in 0..UNITS.len().min(3) {
                    for i in 0..UNITS[u].words.len() {
                        if !pool.contains(&(u, i)) {
                            pool.push((u, i));
                        }
                    }
                }
            }
            // Only words that split into 2 to 7 letter groups make a good puzzle.
            pool.retain(|&(u, i)| (2..=7).contains(&clusters(UNITS[u].words[i].th).len()));
            let mut uniq: Vec<(usize, usize)> = Vec::new();
            for &(u, i) in &pool {
                if !uniq.iter().any(|&(a, b)| UNITS[a].words[b].th == UNITS[u].words[i].th) {
                    uniq.push((u, i));
                }
            }
            rng.shuffle(&mut uniq);
            for &(u, i) in uniq.iter().take(10) {
                out.push(make(rng, Kind::Spell, u, i, 0, &[], ctx));
            }
        }
        LessonKind::ConsonantClass => {
            let pool: Vec<&crate::data::Consonant> = crate::data::all_consonants().filter(|c| !crate::data::is_obsolete(c.letter)).collect();
            let mut idx: Vec<usize> = (0..pool.len()).collect();
            rng.shuffle(&mut idx);
            for &i in idx.iter().take(10) {
                let c = pool[i];
                out.push(Exercise {
                    badge: "자음 계급",
                    title: "이 자음은 어떤 계급일까요?".into(),
                    prompt_thai: Some(Prompt { th: c.letter.to_string(), pron: c.name_pron.to_string(), tones: String::new() }),
                    prompt_ko: None,
                    speak: Some(c.name.to_string()),
                    listen_only: false,
                    body: Body::Choice { options: (0..3u8).map(|k| Card::ko(crate::data::class_name(k))).collect(), correct: c.class as usize },
                    solution: format!("{} = {} ({})", c.letter, crate::data::class_name(c.class), c.name_pron),
                    solution_speak: Some(c.name.to_string()),
                    words: vec![],
                    sentence: None,
                });
            }
        }
        LessonKind::ToneRules => {
            let mut idx: Vec<usize> = (0..crate::data::TONE_RULES.len()).collect();
            rng.shuffle(&mut idx);
            for &i in idx.iter().take(10) {
                let r = &crate::data::TONE_RULES[i];
                let correct = TONES.iter().position(|t| t.code == r.tone).unwrap_or(0);
                out.push(Exercise {
                    badge: "성조 규칙",
                    title: "이 조건이면 성조는?".into(),
                    prompt_thai: None,
                    prompt_ko: Some(format!("{} + {}", crate::data::class_name(r.class), r.when)),
                    speak: None,
                    listen_only: false,
                    body: Body::Choice { options: TONES.iter().map(|t| Card::ko(&format!("{} {}", t.mark, t.name))).collect(), correct },
                    solution: format!("{} {} · 예: {} ({})", tone_of(r.tone).map(|t| t.mark).unwrap_or(""), tone_of(r.tone).map(|t| t.name).unwrap_or(""), r.example, r.example_pron),
                    solution_speak: Some(r.example.to_string()),
                    words: vec![],
                    sentence: None,
                });
            }
        }
        LessonKind::Finals => {
            let mut letters: Vec<&'static str> = crate::data::FINALS.iter().flat_map(|f| f.letters.split_whitespace()).collect();
            letters.retain(|l| !crate::data::is_obsolete(l));
            rng.shuffle(&mut letters);
            for &l in letters.iter().take(10) {
                let Some(fi) = crate::data::final_index(l) else { continue };
                let f = &crate::data::FINALS[fi];
                let name = crate::data::all_consonants().find(|c| c.letter == l);
                let mut others: Vec<usize> = (0..crate::data::FINALS.len()).filter(|&j| j != fi).collect();
                rng.shuffle(&mut others);
                let mut opts: Vec<usize> = vec![fi];
                opts.extend(others.into_iter().take(3));
                rng.shuffle(&mut opts);
                let correct = opts.iter().position(|&j| j == fi).unwrap_or(0);
                out.push(Exercise {
                    badge: "받침",
                    title: "이 글자가 받침이면 어떤 소리일까요?".into(),
                    prompt_thai: Some(Prompt { th: l.to_string(), pron: name.map(|c| c.name_pron.to_string()).unwrap_or_default(), tones: String::new() }),
                    prompt_ko: None,
                    speak: name.map(|c| c.name.to_string()),
                    listen_only: false,
                    body: Body::Choice { options: opts.iter().map(|&j| Card::ko(crate::data::FINALS[j].sound)).collect(), correct },
                    solution: format!("{} → {} 받침 · 예: {} ({})", l, f.sound, f.example, f.example_pron),
                    solution_speak: Some(f.example.to_string()),
                    words: vec![],
                    sentence: None,
                });
            }
        }
        LessonKind::Reading => {
            // Words the learner has met (weakest first); at the start, words of the first units.
            let mut pool: Vec<(usize, usize)> = ctx.focus.to_vec();
            if pool.len() < 10 {
                for u in 0..UNITS.len().min(4) {
                    for i in 0..UNITS[u].words.len() {
                        if !pool.contains(&(u, i)) {
                            pool.push((u, i));
                        }
                    }
                }
            }
            let mut uniq: Vec<(usize, usize)> = Vec::new();
            for &(u, i) in &pool {
                let w = &UNITS[u].words[i];
                // Words with only a one-letter-wide sound make dull questions.
                if w.th.chars().count() >= 2 && !uniq.iter().any(|&(a, b)| UNITS[a].words[b].th == w.th) {
                    uniq.push((u, i));
                }
            }
            rng.shuffle(&mut uniq);
            let all: Vec<(usize, usize)> = (0..UNITS.len()).flat_map(|u| (0..UNITS[u].words.len()).map(move |i| (u, i))).collect();
            for &(u, i) in uniq.iter().take(10) {
                let w = &UNITS[u].words[i];
                let len = w.pron.chars().count();
                // Wrong answers: other words that sound about as long, so the length gives nothing away.
                let mut near: Vec<usize> = (0..all.len())
                    .filter(|&k| {
                        let o = &UNITS[all[k].0].words[all[k].1];
                        o.pron != w.pron && o.pron.chars().count().abs_diff(len) <= 1
                    })
                    .collect();
                rng.shuffle(&mut near);
                let mut prons: Vec<&str> = vec![w.pron];
                for k in near {
                    let o = &UNITS[all[k].0].words[all[k].1];
                    if !prons.contains(&o.pron) {
                        prons.push(o.pron);
                    }
                    if prons.len() == 4 {
                        break;
                    }
                }
                rng.shuffle(&mut prons);
                let correct = prons.iter().position(|&p| p == w.pron).unwrap_or(0);
                out.push(Exercise {
                    badge: "읽기",
                    title: "이 단어는 어떻게 읽을까요?".into(),
                    prompt_thai: Some(Prompt { th: w.th.to_string(), pron: String::new(), tones: String::new() }),
                    prompt_ko: None,
                    speak: None,
                    listen_only: false,
                    body: Body::Choice { options: prons.iter().map(|p| Card::ko(p)).collect(), correct },
                    solution: format!("{} = {} · {}", w.th, w.pron, w.ko),
                    solution_speak: Some(w.th.to_string()),
                    words: vec![(u, i)],
                    sentence: None,
                });
            }
        }
        LessonKind::Digits => {
            let n = crate::data::DIGITS.len();
            let mut idx: Vec<usize> = (0..n).collect();
            rng.shuffle(&mut idx);
            for &i in idx.iter().take(10) {
                let d = &crate::data::DIGITS[i];
                let mut others: Vec<usize> = (0..n).filter(|&j| j != i).collect();
                rng.shuffle(&mut others);
                let mut opts: Vec<usize> = vec![i];
                opts.extend(others.into_iter().take(3));
                rng.shuffle(&mut opts);
                let correct = opts.iter().position(|&j| j == i).unwrap_or(0);
                out.push(Exercise {
                    badge: "태국 숫자",
                    title: "이 숫자는 얼마일까요?".into(),
                    prompt_thai: Some(Prompt { th: d.glyph.to_string(), pron: String::new(), tones: String::new() }),
                    prompt_ko: None,
                    speak: None,
                    listen_only: false,
                    body: Body::Choice { options: opts.iter().map(|&j| Card::ko(crate::data::DIGITS[j].arabic)).collect(), correct },
                    solution: format!("{} = {} · {} ({})", d.glyph, d.arabic, d.name, d.pron),
                    solution_speak: Some(d.name.to_string()),
                    words: vec![],
                    sentence: None,
                });
            }
        }
        LessonKind::Signs => {
            let n = crate::data::SIGNS.len();
            let mut idx: Vec<usize> = (0..n).collect();
            rng.shuffle(&mut idx);
            for &i in idx.iter().take(10) {
                let sg = &crate::data::SIGNS[i];
                let mut others: Vec<usize> = (0..n).filter(|&j| j != i && crate::data::SIGNS[j].ko != sg.ko).collect();
                rng.shuffle(&mut others);
                let mut opts: Vec<usize> = vec![i];
                opts.extend(others.into_iter().take(3));
                rng.shuffle(&mut opts);
                let correct = opts.iter().position(|&j| j == i).unwrap_or(0);
                out.push(Exercise {
                    badge: "표지판",
                    title: "이 표지판은 무슨 뜻일까요?".into(),
                    prompt_thai: Some(Prompt { th: sg.th.to_string(), pron: String::new(), tones: String::new() }),
                    prompt_ko: None,
                    speak: None,
                    listen_only: false,
                    body: Body::Choice { options: opts.iter().map(|&j| Card::ko(crate::data::SIGNS[j].ko)).collect(), correct },
                    solution: format!("{} ({}) = {}", sg.th, sg.pron, sg.ko),
                    solution_speak: Some(sg.th.to_string()),
                    words: vec![],
                    sentence: None,
                });
            }
        }
        LessonKind::SentenceReading => {
            // Sentences from the units the learner has reached; at the start, the first units.
            let mut units: Vec<usize> = ctx.focus.iter().map(|&(u, _)| u).collect();
            units.sort_unstable();
            units.dedup();
            for u in 0..UNITS.len().min(4) {
                if !units.contains(&u) {
                    units.push(u);
                }
            }
            let mut pool: Vec<(usize, usize)> = Vec::new();
            for &u in &units {
                for i in 0..UNITS[u].sentences.len() {
                    // Skip one-word sentences and sentences with a Korean meaning shared by another.
                    if UNITS[u].sentences[i].th.len() >= 2 {
                        pool.push((u, i));
                    }
                }
            }
            rng.shuffle(&mut pool);
            let meanings: Vec<String> = pool.iter().map(|&(u, i)| UNITS[u].sentences[i].korean()).collect();
            for (k, &(u, i)) in pool.iter().take(10).enumerate() {
                let sent = &UNITS[u].sentences[i];
                let right = meanings[k].clone();
                let mut wrong: Vec<String> = Vec::new();
                for m in meanings.iter().skip(10).chain(meanings.iter().take(10)) {
                    if *m != right && !wrong.contains(m) {
                        wrong.push(m.clone());
                    }
                    if wrong.len() == 3 {
                        break;
                    }
                }
                if wrong.len() < 3 {
                    continue;
                }
                let mut opts: Vec<String> = vec![right.clone()];
                opts.extend(wrong);
                rng.shuffle(&mut opts);
                let correct = opts.iter().position(|o| *o == right).unwrap_or(0);
                out.push(Exercise {
                    badge: "문장 읽기",
                    title: "이 문장은 무슨 뜻일까요?".into(),
                    prompt_thai: Some(Prompt { th: sentence_thai(sent, ctx.female), pron: String::new(), tones: String::new() }),
                    prompt_ko: None,
                    speak: None,
                    listen_only: false,
                    body: Body::Choice { options: opts.iter().map(|o| Card::ko(o)).collect(), correct },
                    solution: format!("{} = {} · {}", sentence_thai(sent, ctx.female), sentence_pron(sent, ctx.female), right),
                    solution_speak: Some(sentence_thai(sent, ctx.female)),
                    words: vec![],
                    sentence: Some((u, i)),
                });
            }
        }
        LessonKind::Mistakes => {
            let cycle = [Kind::Thai, Kind::Meaning, Kind::Listen, Kind::Tone];
            let (mut wc, mut sc) = (0usize, 0usize);
            for &(u, item, is_sentence) in ctx.mistakes.iter().take(12) {
                if is_sentence {
                    let s = &UNITS[u].sentences[item];
                    let k = if sc % 2 == 1 && s.ko.len() >= 2 { Kind::ToKorean } else { Kind::Build };
                    sc += 1;
                    out.push(make(rng, k, u, 0, item, &[], ctx));
                } else {
                    let k = cycle[wc % cycle.len()];
                    wc += 1;
                    out.push(make(rng, k, u, item, 0, &[], ctx));
                }
            }
            rng.shuffle(&mut out);
        }
        LessonKind::Practice => {
            let focus: Vec<(usize, usize)> = if ctx.focus.is_empty() { (0..5).map(|i| (0, i)).collect() } else { ctx.focus.to_vec() };
            let take = focus.len().min(7);
            let cycle = [Kind::Meaning, Kind::Listen, Kind::Thai, Kind::Tone, Kind::Meaning, Kind::Thai, Kind::Listen];
            for (k, &(u, i)) in focus.iter().take(take).enumerate() {
                let mut kind = cycle[k % cycle.len()];
                if kind == Kind::Tone && UNITS[u].words[i].tone.chars().count() != 1 {
                    kind = Kind::Thai;
                }
                out.push(make(rng, kind, u, i, 0, &focus, ctx));
            }
            // Matching with the weakest words.
            let (u0, i0) = focus[0];
            let weakest: Vec<(usize, usize)> = focus.iter().copied().take(5).collect();
            out.push(make(rng, Kind::Match, u0, i0, 0, &weakest, ctx));
            // One sentence using a weak word if possible.
            let unit = &UNITS[u0];
            let w0 = unit.words[i0].th;
            let sent = unit.sentences.iter().position(|s| sentence_tokens(s, ctx.female).contains(&w0)).unwrap_or_else(|| rng.below(unit.sentences.len()));
            out.push(make(rng, Kind::Build, u0, i0, sent, &focus, ctx));
            // Mix the order a little but keep matching near the end.
            let n = out.len();
            rng.shuffle(&mut out[..n - 2]);
        }
    }
    out
}

/// Checks that every lesson can be answered. Used by `--selftest`.
pub fn selftest(check_audio: bool) -> Result<String, String> {
    let mut n = 0;
    for (u, unit) in UNITS.iter().enumerate() {
        for w in unit.words {
            if w.tone.is_empty() {
                return Err(format!("no tone for {}", w.th));
            }
        }
        for s in unit.sentences {
            for female in [false, true] {
                for t in sentence_tokens(s, female) {
                    if token_info(t).0.is_empty() {
                        return Err(format!("no pronunciation for {t}"));
                    }
                }
            }
        }
        for lesson in 0..crate::data::LESSONS_PER_UNIT {
            for female in [false, true] {
                for tts in [false, true] {
                    for seed in 1..30u64 {
                        let mut rng = Rng::new(seed);
                        let ctx = Ctx { tts, female, focus: &[], mistakes: &[] };
                        for ex in build(LessonKind::Normal { unit: u, lesson }, &mut rng, &ctx) {
                            check_ex(&ex, female)?;
                            n += 1;
                        }
                    }
                }
            }
        }
    }
    // practice
    let focus: Vec<(usize, usize)> = vec![(2, 3), (0, 6), (4, 1), (5, 9), (1, 0), (3, 2)];
    for seed in 1..50u64 {
        let mut rng = Rng::new(seed);
        for ex in build(LessonKind::Practice, &mut rng, &Ctx { tts: true, female: seed % 2 == 0, focus: &focus, mistakes: &[] }) {
            check_ex(&ex, seed % 2 == 0)?;
            n += 1;
        }
        for kind in [LessonKind::Letters, LessonKind::Vowels, LessonKind::Spelling, LessonKind::ConsonantClass, LessonKind::ToneRules, LessonKind::Finals, LessonKind::Reading, LessonKind::SentenceReading, LessonKind::Signs, LessonKind::Digits] {
            for focus_set in [&[][..], &focus[..]] {
                let es = build(kind, &mut rng, &Ctx { tts: true, female: false, focus: focus_set, mistakes: &[] });
                if es.len() < 8 {
                    return Err(format!("{kind:?} lesson too short: {}", es.len()));
                }
                for e in es {
                    check_ex(&e, false)?;
                    n += 1;
                }
            }
        }
        let mistakes: Vec<(usize, usize, bool)> = vec![(0, 2, false), (3, 1, true), (6, 4, true), (11, 9, false), (9, 3, true), (1, 0, false)];
        let ex = build(LessonKind::Mistakes, &mut rng, &Ctx { tts: seed % 3 == 0, female: seed % 2 == 0, focus: &[], mistakes: &mistakes });
        if ex.len() != mistakes.len() {
            return Err("mistake review lost questions".into());
        }
        for e in ex {
            check_ex(&e, seed % 2 == 0)?;
            n += 1;
        }
    }
    let mut missing = 0usize;
    let mut first_missing = String::new();
    for t in all_speech() {
        if !crate::audio::clip_decodes(&t) {
            missing += 1;
            if first_missing.is_empty() {
                first_missing = t;
            }
        }
    }
    if check_audio && missing > 0 {
        return Err(format!("no playable recording for {missing} phrases, e.g. {first_missing} (run tools/make_audio_google.py)"));
    }
    Ok(format!("{n} exercises checked, {} recordings present, {missing} missing", all_speech().len() - missing))
}

fn check_ex(ex: &Exercise, female: bool) -> Result<(), String> {
    match &ex.body {
        Body::Choice { options, correct } => {
            if *correct >= options.len() || options.len() < 2 {
                return Err(format!("bad choice in {}", ex.title));
            }
        }
        Body::Tiles { tiles, accepted, .. } => {
            let main = &accepted[0];
            let mut have: Vec<&str> = tiles.iter().map(|t| t.main.as_str()).collect();
            for t in main {
                match have.iter().position(|h| h == t) {
                    Some(p) => {
                        have.remove(p);
                    }
                    None => return Err(format!("tile {t} missing in {}", ex.solution)),
                }
            }
            for t in other_form(female) {
                if tiles.iter().any(|c| c.main == t) {
                    return Err(format!("other speech form {t} offered in {}", ex.solution));
                }
            }
        }
        Body::Flash { th, ko, .. } => {
            if th.is_empty() || ko.is_empty() {
                return Err("empty card".into());
            }
        }
        Body::Match { left, right, right_to_left } => {
            if left.len() != right.len() || right_to_left.len() != right.len() || left.len() != ex.words.len() {
                return Err("bad match".into());
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- speech list

/// Stable id of a spoken text (FNV-1a, 64 bit). Used to find its recording in the voice pack.
pub fn speech_id(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Every Thai text the app can read aloud (both speech forms), without duplicates.
pub fn all_speech() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |t: String| {
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    };
    for unit in UNITS {
        for w in unit.words {
            add(w.th.to_string());
        }
        for s in unit.sentences {
            for female in [false, true] {
                add(sentence_thai(s, female));
                for t in sentence_tokens(s, female) {
                    add(t.to_string());
                }
            }
        }
    }
    for (t, _, _) in EXTRA_TOKENS {
        add(t.to_string());
    }
    for c in crate::data::all_consonants() {
        add(c.name.to_string());
    }
    for r in crate::data::TONE_RULES {
        add(r.example.to_string());
    }
    for d in crate::data::DIGITS {
        add(d.name.to_string());
    }
    for sg in crate::data::SIGNS {
        add(sg.th.to_string());
    }
    for sp in crate::data::SPECIALS {
        for (th, _, _) in sp.examples {
            add(th.to_string());
        }
    }
    for f in crate::data::FINALS {
        add(f.example.to_string());
    }
    for v in crate::data::VOWELS {
        add(v.example.to_string());
    }
    for t in TONES {
        add(t.example.to_string());
    }
    out
}

/// All course content as tab-separated lines, for the native-speaker review sheet
/// (`sawasdee --dump-content`, see tools/export_review_sheet.py).
/// Columns: kind, unit, number, Thai (male form), Thai (female form), Hangul pronunciation, tones, Korean meaning, note.
pub fn dump_content() -> String {
    use crate::data::{class_name, CONSONANTS, CONSONANTS_MORE, FINALS, TONES, TONE_RULES, VOWELS};
    let mut out = String::new();
    let mut line = |cols: [&str; 9]| {
        out.push_str(&cols.join("\t"));
        out.push('\n');
    };
    for (u, unit) in UNITS.iter().enumerate() {
        let unit_name = format!("{}. {}", u + 1, unit.title);
        for (i, w) in unit.words.iter().enumerate() {
            line(["단어", &unit_name, &(i + 1).to_string(), w.th, "", w.pron, &format!("{} {}", w.tone, tone_marks(w.tone)), w.ko, ""]);
        }
        for (i, s) in unit.sentences.iter().enumerate() {
            let tones = sentence_tones(s, false);
            line(["문장", &unit_name, &(i + 1).to_string(), &sentence_thai(s, false), &sentence_thai(s, true), &sentence_pron(s, false), &tones, &s.korean(), ""]);
        }
    }
    for c in CONSONANTS.iter().chain(CONSONANTS_MORE.iter()) {
        line(["자음", "알파벳", c.letter, c.name, "", c.name_pron, class_name(c.class), &format!("{} · {}", c.sound, c.meaning), ""]);
    }
    for v in VOWELS {
        line(["모음", "알파벳", v.form, v.example, "", v.example_pron, "", v.sound, ""]);
    }
    for f in FINALS {
        line(["받침", "알파벳", f.sound, f.example, "", f.example_pron, "", &format!("글자: {} · {}", f.letters, f.example_ko), ""]);
    }
    for t in TONES {
        line(["성조", "알파벳", &t.code.to_string(), t.example, "", t.example_pron, t.mark, &format!("{} · {} · {}", t.name, t.how, t.example_ko), ""]);
    }
    for d in crate::data::DIGITS {
        line(["숫자", "알파벳", d.glyph, d.name, "", d.pron, "", d.ko, ""]);
    }
    for sg in crate::data::SIGNS {
        line(["표지판", "알파벳", "", sg.th, "", sg.pron, "", sg.ko, ""]);
    }
    for sp in crate::data::SPECIALS {
        for (th, pr, ko) in sp.examples {
            line(["특수 표기", sp.title, "", th, "", pr, "", ko, ""]);
        }
    }
    for r in TONE_RULES {
        line(["성조 규칙", "알파벳", class_name(r.class), r.example, "", r.example_pron, &r.tone.to_string(), &format!("{} · {}", r.when, r.example_ko), ""]);
    }
    out
}
