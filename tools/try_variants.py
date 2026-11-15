#!/usr/bin/env python3
"""Makes a few alternative recordings of ONE Thai word so you can listen and pick the best.

  export GOOGLE_TTS_API_KEY="..."            (type it only in your own terminal)
  python3 tools/try_variants.py ชื่อ

Files go to audio_work/try/ (git-ignored): 1.mp3, 2.mp3, ... and a list of what each one is.
Costs a few dozen characters of Google Chirp 3 HD speech (a fraction of a cent).
"""
import base64
import json
import os
import sys
import urllib.request

APP = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(APP, "audio_work", "try")
KEY = os.environ.get("GOOGLE_TTS_API_KEY")
F, M = "th-TH-Chirp3-HD-Aoede", "th-TH-Chirp3-HD-Charon"


def synth(text, voice, rate=None):
    audio = {"audioEncoding": "MP3"}
    if rate:
        audio["speakingRate"] = rate
    body = json.dumps({"input": {"text": text}, "voice": {"languageCode": "th-TH", "name": voice}, "audioConfig": audio}).encode()
    req = urllib.request.Request("https://texttospeech.googleapis.com/v1/text:synthesize", data=body,
                                 headers={"Content-Type": "application/json", "X-Goog-Api-Key": KEY})
    with urllib.request.urlopen(req, timeout=60) as r:
        return base64.b64decode(json.load(r)["audioContent"])


def main():
    if not KEY or len(sys.argv) < 2:
        sys.exit("usage: GOOGLE_TTS_API_KEY=... python3 tools/try_variants.py <thai word>")
    w = sys.argv[1]
    variants = [
        (w, F, None, "word alone, female voice (current)"),
        (w, M, None, "word alone, male voice"),
        (w, F, 0.85, "word alone, female voice, slower"),
        (w + " " + w, F, None, "word said twice, female voice"),
        ("คำว่า " + w, F, None, "'the word' + word (cut the start yourself if you like it)"),
    ]
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "list.txt"), "w") as lst:
        for i, (t, v, r, desc) in enumerate(variants, 1):
            open(os.path.join(OUT, f"{i}.mp3"), "wb").write(synth(t, v, r))
            lst.write(f"{i}.mp3  {desc}\n")
            print(f"{i}.mp3  {desc}")
    print("Listen to the files in", OUT)


main()
