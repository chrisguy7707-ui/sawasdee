#!/usr/bin/env python3
"""Makes the app's Thai voice pack with Google Cloud Text-to-Speech (Chirp 3: HD, th-TH).

Replaces tools/make_audio.py, which recorded the macOS "Kanya" voice. Recordings of
macOS system voices must not be shipped in a commercial app; Google allows using the
audio files it generates in your own apps (per its docs; read the Cloud terms yourself).

Setup (once):
  1. Create a Google Cloud project, enable "Cloud Text-to-Speech API", create an API key.
  2. export GOOGLE_TTS_API_KEY="..."        (never put the key in a file in this folder)

Run from the app folder:
  cargo build                               (makes target/debug/sawasdee, used to list the phrases)
  python3 tools/make_audio_google.py --dry-run     (no key needed: counts phrases and characters)
  python3 tools/make_audio_google.py --listen 5    (makes 5 clips into audio_work/ so you can listen first)
  python3 tools/make_audio_google.py               (makes everything and writes assets/audio/thai_voice.pack)

Clips are cached in audio_work/ (git-ignored), so a stopped run continues where it
stopped and a re-run costs nothing for phrases that did not change.

Optional environment variables:
  GOOGLE_TTS_FEMALE   voice for ค่ะ / ฉัน phrases and neutral phrases  (default th-TH-Chirp3-HD-Aoede)
  GOOGLE_TTS_MALE     voice for ครับ / ผม phrases                       (default th-TH-Chirp3-HD-Charon)
  GOOGLE_TTS_RATE     speaking rate, e.g. 0.85 for slower learner speech (not sent when unset;
                      check that the chosen voice type accepts it)

Pack format (same as before): u32 count, then per clip: u64 id, u32 length, mp3 bytes (little endian).
"""
import json
import os
import struct
import subprocess
import sys
import time
import urllib.error
import urllib.request
import base64

HERE = os.path.dirname(os.path.abspath(__file__))
APP = os.path.dirname(HERE)
BIN = os.path.join(APP, "target", "debug", "sawasdee")
OUT = os.environ.get("PACK_OUT", os.path.join(APP, "assets", "audio", "thai_voice.pack"))
WORK = os.environ.get("AUDIO_WORK", os.path.join(APP, "audio_work"))
ENDPOINT = os.environ.get("GOOGLE_TTS_ENDPOINT", "https://texttospeech.googleapis.com/v1/text:synthesize")

FEMALE = os.environ.get("GOOGLE_TTS_FEMALE", "th-TH-Chirp3-HD-Aoede")
MALE = os.environ.get("GOOGLE_TTS_MALE", "th-TH-Chirp3-HD-Charon")
RATE = os.environ.get("GOOGLE_TTS_RATE")

FEMALE_MARKS = ("ค่ะ", "คะ", "ฉัน", "ดิฉัน")
MALE_MARKS = ("ครับ", "ผม")
MIN_GAP = float(os.environ.get("GOOGLE_TTS_GAP", "0.40"))  # seconds between requests (Chirp 3 allows 200/minute)


def speech_id(text: str) -> int:
    h = 0xCBF29CE484222325
    for b in text.encode("utf-8"):
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def voice_for(text: str) -> str:
    if any(m in text for m in FEMALE_MARKS):
        return FEMALE
    if any(m in text for m in MALE_MARKS):
        return MALE
    return FEMALE


def phrases() -> list:
    if not os.path.exists(BIN):
        sys.exit(f"{BIN} not found. Run `cargo build` in the app folder first.")
    out = subprocess.check_output([BIN, "--list-speech"], text=True)
    seen, texts = set(), []
    for t in out.splitlines():
        t = t.strip()
        if t and t not in seen:
            seen.add(t)
            texts.append(t)
    return texts


def synthesize(text: str, voice: str, key: str) -> bytes:
    audio = {"audioEncoding": "MP3"}
    if RATE:
        audio["speakingRate"] = float(RATE)
    body = json.dumps({
        "input": {"text": text},
        "voice": {"languageCode": "th-TH", "name": voice},
        "audioConfig": audio,
    }).encode("utf-8")
    req = urllib.request.Request(
        ENDPOINT, data=body,
        headers={"Content-Type": "application/json", "X-Goog-Api-Key": key},
    )
    for attempt in range(6):
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                return base64.b64decode(json.load(r)["audioContent"])
        except urllib.error.HTTPError as e:
            detail = e.read().decode("utf-8", "replace")[:300]
            if e.code in (429, 500, 502, 503, 504) and attempt < 5:
                time.sleep(2 ** attempt)
                continue
            sys.exit(f"Google TTS error {e.code} for {text!r} ({voice}): {detail}")
        except (urllib.error.URLError, TimeoutError) as e:
            if attempt < 5:
                time.sleep(2 ** attempt)
                continue
            sys.exit(f"Network error for {text!r}: {e}")


def clip_path(text: str, voice: str) -> str:
    rate = RATE or "default"
    return os.path.join(WORK, f"{speech_id(text):016x}-{voice}-{rate}.mp3")


def main():
    args = sys.argv[1:]
    dry = "--dry-run" in args
    listen = int(args[args.index("--listen") + 1]) if "--listen" in args else None
    texts = phrases()
    nf = sum(1 for t in texts if voice_for(t) == FEMALE)
    chars = sum(len(t) for t in texts)
    print(f"{len(texts)} phrases, {chars} characters; {nf} with {FEMALE}, {len(texts) - nf} with {MALE}")
    if dry:
        return
    key = os.environ.get("GOOGLE_TTS_API_KEY")
    if not key:
        sys.exit("Set GOOGLE_TTS_API_KEY first (see the top of this file).")
    os.makedirs(WORK, exist_ok=True)

    todo = texts[:listen] if listen else texts
    made = 0
    for i, t in enumerate(todo):
        v = voice_for(t)
        p = clip_path(t, v)
        if not os.path.exists(p):
            data = synthesize(t, v, key)
            with open(p + ".tmp", "wb") as f:
                f.write(data)
            os.replace(p + ".tmp", p)
            made += 1
            time.sleep(MIN_GAP)
        if i % 50 == 0:
            print(f"{i}/{len(todo)}", file=sys.stderr)
    print(f"{len(todo)} clips ready ({made} new) in {WORK}")
    if listen:
        print("Listen test only: the pack was not written. Open the .mp3 files and check tones and speed.")
        return

    with open(OUT, "wb") as f:
        f.write(struct.pack("<I", len(texts)))
        for t in texts:
            data = open(clip_path(t, voice_for(t)), "rb").read()
            f.write(struct.pack("<QI", speech_id(t), len(data)))
            f.write(data)
    print(f"{len(texts)} clips, {os.path.getsize(OUT) / 1e6:.1f} MB -> {OUT}")


main()
