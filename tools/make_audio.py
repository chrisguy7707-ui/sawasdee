#!/usr/bin/env python3
"""Records every Thai word and sentence of the app with the computer's Thai voice
and packs the recordings into assets/audio/thai_voice.pack (embedded in the app).

Run on a Mac:  python3 tools/make_audio.py
Pack format: u32 count, then per clip: u64 id, u32 length, mp3 bytes (little endian).
"""
import os, struct, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
APP = os.path.dirname(HERE)
BIN = os.path.join(APP, "target", "debug", "sawasdee")
OUT = os.path.join(APP, "assets", "audio", "thai_voice.pack")


def speech_id(text: str) -> int:
    h = 0xCBF29CE484222325
    for b in text.encode("utf-8"):
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def main():
    texts = [t for t in subprocess.check_output([BIN, "--list-speech"], text=True).splitlines() if t.strip()]
    clips = []
    with tempfile.TemporaryDirectory() as tmp:
        for i, t in enumerate(texts):
            aiff = os.path.join(tmp, "a.aiff")
            mp3 = os.path.join(tmp, "a.mp3")
            subprocess.run(["say", "-v", "Kanya", "-r", "150", "-o", aiff, t], check=True)
            subprocess.run(
                ["ffmpeg", "-y", "-loglevel", "error", "-i", aiff, "-ac", "1", "-ar", "22050", "-codec:a", "libmp3lame", "-b:a", "40k", mp3],
                check=True,
            )
            clips.append((speech_id(t), open(mp3, "rb").read()))
            if i % 50 == 0:
                print(f"{i}/{len(texts)}", file=sys.stderr)
    with open(OUT, "wb") as f:
        f.write(struct.pack("<I", len(clips)))
        for cid, data in clips:
            f.write(struct.pack("<QI", cid, len(data)))
            f.write(data)
    print(f"{len(clips)} clips, {os.path.getsize(OUT) / 1e6:.1f} MB -> {OUT}")


main()
