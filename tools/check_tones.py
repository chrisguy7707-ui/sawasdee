#!/usr/bin/env python3
"""Checks the tone codes in the course against the Thai spelling rules.

For every one-syllable word it works out the tone from the letters
(consonant class, live/dead syllable, vowel length, tone mark) and compares it with the
tone code written in src/data.rs. A difference means either the code is wrong or this
checker does not know the spelling; look at each one by hand.

Run from the app folder after `cargo build`:   python3 tools/check_tones.py
Multi-syllable words are not checked (their syllables are not split here).
Codes: M mid, L low, F falling, H high, R rising.
"""
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
BIN = os.path.join(os.path.dirname(HERE), "target", "debug", "sawasdee")

MID, HIGH = set("กจดตบปอฎฏ"), set("ขฃฉฐถผฝศษสห")
LEAD = set("เแโไใ")
MARKS = {"่": "ek", "้": "tho", "๊": "tri", "๋": "chat"}
SONORANT = set("งนมยวญณรลฬ")
CLUSTER_FIRST, CLUSTER_SECOND = set("กขคตปพผ"), set("รลว")
VOWEL_CHARS = set("ะาำิีึืุูัอ็")


def cls_of(c):
    return "mid" if c in MID else "high" if c in HIGH else "low"


def tone_of(word):
    s = word.replace("เสร็", "เส็")  # สร is read as a plain ส
    # silent letters: a letter followed by the silencer mark
    out = []
    i = 0
    while i < len(s):
        if i + 1 < len(s) and s[i + 1] == "์":
            i += 2
            continue
        out.append(s[i])
        i += 1
    s = "".join(out)
    mark = None
    for ch in list(s):
        if ch in MARKS:
            mark = MARKS[ch]
            s = s.replace(ch, "")
    lead = None
    if s and s[0] in LEAD:
        lead, s = s[0], s[1:]
    if not s:
        return None
    # initial consonant(s) and their class
    if len(s) > 2 and s[0] == "ห" and s[1] in SONORANT:
        cls, s = "high", s[2:]
    elif len(s) > 2 and s[0] == "อ" and s[1] == "ย":
        cls, s = "mid", s[2:]
    elif len(s) > 2 and s[0] in CLUSTER_FIRST and s[1] in CLUSTER_SECOND and (s[2] in VOWEL_CHARS or lead or len(s) == 3):
        cls, s = cls_of(s[0]), s[2:]
    else:
        cls, s = cls_of(s[0]), s[1:]

    # vowel and final consonant
    special_live = False
    length = None
    final = None
    r = s
    if lead in ("ไ", "ใ"):
        special_live = True
    elif lead == "เ":
        if r.startswith("า"):
            if r == "า":
                special_live = True
            elif r == "าะ":
                length = "short"
            else:
                return None
        elif r == "ะ":
            length = "short"
        elif r.startswith("็"):
            length, final = "short", r[1:] or None
        elif r.startswith("ียะ") or r.startswith("ือะ") or r.startswith("อะ"):
            length = "short"
        elif r.startswith("ีย"):
            length, final = "long", r[2:] or None
        elif r.startswith("ือ"):
            length, final = "long", r[2:] or None
        elif r.startswith("อ"):
            length, final = "long", r[1:] or None
        elif r.startswith("ิ"):
            length, final = "long", r[1:] or None
        else:
            length, final = "long", r or None
    elif lead in ("แ", "โ"):
        if r == "ะ":
            length = "short"
        elif r.startswith("็"):
            length, final = "short", r[1:] or None
        else:
            length, final = "long", r or None
    else:
        if r.startswith("ัว"):
            length, final = ("short", None) if r == "ัวะ" else ("long", r[2:] or None)
        elif r.startswith("ว") and not r.startswith("วะ"):
            length, final = "long", r[1:] or None  # the "ua" vowel written with a lone ว (สวย, ควร)
        elif r.startswith("ะ"):
            length = "short"
        elif r.startswith("ำ"):
            special_live = True
        elif r.startswith("า"):
            length, final = "long", r[1:] or None
        elif r.startswith("ิ") or r.startswith("ึ") or r.startswith("ุ") or r.startswith("ั"):
            length, final = "short", r[1:] or None
        elif r.startswith("ี") or r.startswith("ู"):
            length, final = "long", r[1:] or None
        elif r.startswith("ื"):
            rest = r[1:]
            length, final = "long", (rest[1:] if rest.startswith("อ") else rest) or None
        elif r.startswith("อ") and len(r) <= 2:
            length, final = "long", r[1:] or None
        elif len(r) == 1 and r not in VOWEL_CHARS:
            length, final = "short", r  # inherent vowel + final
        else:
            return None
    if final is not None and (len(final) != 1 or final in VOWEL_CHARS):
        return None

    if special_live:
        live = True
    elif final:
        live = final in SONORANT
    else:
        live = length == "long"

    if mark == "ek":
        return {"mid": "L", "high": "L", "low": "F"}[cls]
    if mark == "tho":
        return {"mid": "F", "high": "F", "low": "H"}[cls]
    if mark == "tri":
        return "H"
    if mark == "chat":
        return "R"
    if live:
        return {"mid": "M", "high": "R", "low": "M"}[cls]
    if cls == "low":
        return "H" if length == "short" else "F"
    return "L"


# Hangul initial sound each Thai initial letter should have in this course's spelling
INITIAL = {}
for letters, jamo in [("ก", "ㄲ"), ("ขฃคฅฆ", "ㅋ"), ("จ", "ㅉ"), ("ฉชฌ", "ㅊ"), ("ซศษส", "ㅆ"), ("งญยวอ", "ㅇ"), ("ฎด", "ㄷ"), ("ฏต", "ㄸ"),
                      ("ฐฑฒถทธ", "ㅌ"), ("ณน", "ㄴ"), ("บ", "ㅂ"), ("ป", "ㅃ"), ("ผพภฝฟ", "ㅍ"), ("ม", "ㅁ"), ("รลฬ", "ㄹ"), ("หฮ", "ㅎ")]:
    for ch in letters:
        INITIAL[ch] = jamo
CHO = "ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ"


def hangul_initial(pron):
    c = ord(pron[0]) - 0xAC00
    return CHO[c // 588] if 0 <= c < 11172 else None


def first_letter(word):
    s = word.lstrip("เแโไใ")
    had_lead = len(s) < len(word)
    if s[:1] == "ห" and len(s) > 1 and s[1] in SONORANT and (len(s) > 2 or had_lead):
        return s[1]
    if len(s) > 2 and s[0] == "อ" and s[1] == "ย":
        return "ย"
    return s[0] if s else None


def check_pron_initials(rows):
    bad = []
    for r in rows:
        if r[0] not in ("단어", "보조 단어") or not r[5]:
            continue
        letter = first_letter(r[3])
        want = INITIAL.get(letter)
        got = hangul_initial(r[5])
        if want and got and want != got:
            bad.append((r[1], r[3], r[5], want, got))
    return bad


# Spellings whose real tone does not follow the rule; the expected tones come from the dictionary
IRREGULAR = {"อร่อย": "LL", "ก็": "F"}


def load_splits():
    splits = {}
    path = os.path.join(HERE, "syllable_splits.txt")
    if os.path.exists(path):
        for line in open(path, encoding="utf-8"):
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                word, pieces = [x.strip() for x in line.split("=", 1)]
                splits[word] = pieces.split("-")
    return splits


def check_multi(rows):
    splits = load_splits()
    bad, missing, ok = [], [], 0
    for r in rows:
        if r[0] not in ("단어", "보조 단어"):
            continue
        parts = r[6].split()
        if not parts:
            continue
        th, code = r[3], parts[0]
        if len(code) < 2:
            continue
        pieces = splits.get(th)
        if not pieces:
            missing.append(th)
            continue
        got = "".join(tone_of(p) or "?" for p in pieces)
        if th in IRREGULAR:
            got = IRREGULAR[th]
        if len(pieces) != len(code):
            bad.append((r[1], th, r[5], code, f"split has {len(pieces)} syllables"))
        elif got != code:
            bad.append((r[1], th, r[5], code, got))
        else:
            ok += 1
    return ok, bad, missing


def main():
    if not os.path.exists(BIN):
        sys.exit("Run `cargo build` first.")
    rows = [l.split("\t") for l in subprocess.check_output([BIN, "--dump-content"], text=True).splitlines()]
    checked = skipped = 0
    bad = []
    for r in rows:
        if r[0] not in ("단어", "보조 단어"):
            continue
        parts = r[6].split()
        if not parts:
            continue  # words without a tone code (names)
        th, code = r[3], parts[0]
        if len(code) != 1:
            continue
        got = tone_of(th)
        if got is None:
            skipped += 1
            continue
        checked += 1
        if got != code:
            bad.append((r[1], th, r[5], code, got))
    pb = check_pron_initials(rows)
    if pb:
        print(f"{len(pb)} Hangul spellings start with an unexpected sound:")
        for unit, th, pron, want, got in pb:
            print(f"  {th:<10} {pron:<8} expected {want}  found {got}   [{unit}]")
    ok, mbad, missing = check_multi(rows)
    print(f"one-syllable: checked {checked}, skipped {skipped} (spelling not recognised)")
    print(f"multi-syllable: {ok} match")
    if missing:
        print(f"no syllable split for: {' '.join(missing)}")
    failed = False
    if bad:
        failed = True
        print(f"{len(bad)} one-syllable words differ (written code vs rule):")
        for unit, th, pron, code, got in bad:
            print(f"  {th:<10} {pron:<8} written {code}  rule {got}   [{unit}]")
    if mbad:
        failed = True
        print(f"{len(mbad)} multi-syllable words differ (written code vs rule):")
        for unit, th, pron, code, got in mbad:
            print(f"  {th:<12} {pron:<8} written {code}  rule {got}   [{unit}]")
    if pb:
        failed = True
    print("all tone codes match" if not (bad or mbad) else "tone codes differ: see above")
    if failed or missing:
        sys.exit(1)


main()
