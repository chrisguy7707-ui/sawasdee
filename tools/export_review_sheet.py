#!/usr/bin/env python3
"""Makes the files to send to a native Thai speaker for review.

Run from the app folder (after `cargo build`):
    python3 tools/export_review_sheet.py

Writes
    docs/review/sawasdee-review.csv        every word, sentence and letter with empty review columns
                                           (UTF-8 with BOM: opens correctly in Excel and Google Sheets)
    docs/review/audio/                     the voice clips, named by number and Thai text (git-ignored)
    docs/review/sawasdee-review-audio.zip  the same clips as one zip (git-ignored)
The audio needs audio_work/ (made by tools/make_audio_google.py).
"""
import csv
import os
import shutil
import subprocess
import sys
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
APP = os.path.dirname(HERE)
BIN = os.path.join(APP, "target", "debug", "sawasdee")
OUT = os.path.join(APP, "docs", "review")
WORK = os.path.join(APP, "audio_work")

# Items the author is least sure about (see docs/REVIEW_NOTES.md). Matched on the Thai text without spaces.
SHAKY_SENTENCES = {
    "ขอหมอด้วยครับ", "นี่ฉุกเฉิน", "ขอไกด์หนึ่งคน", "ไปเกาะด้วยเรือ", "ตรงนี้มีเน็ตไหม",
    "เบอร์โทรอะไรครับ", "ผมสั่งกาแฟ", "ผมชวนคุณกินข้าว", "ผมรักคุณ", "ผมคิดถึงคุณ",
    # added when the course grew to 15 words per unit
    "ผมสงสารเขา", "ผมเป็นห่วงคุณ", "ผมผิดหวังครับ", "ผมอยากสมัครงาน", "ผมแชร์ภาพ", "ติดตามผมด้วย", "เหตุผลคืออะไร",
    "ผมมีผื่น", "ผมอาเจียน", "ขอตั๋วหนึ่งใบครับ", "แถวนี้มีร้านอาหารไหม", "ทำยังไงครับ", "ผมหลงทางครับ",
}
SHAKY_WORDS = {
    "เช็คอิน", "เช็คเอาท์", "ไวไฟ", "แบตเตอรี่", "ชาร์จ", "โพสต์", "ไกด์", "แอป", "ไลน์", "ซิม", "เน็ต", "เมนู",
    "กิโล", "ผ้าเช็ดตัว", "พระราชวัง", "ร้านขายยา", "ต่อราคา", "ทิวทัศน์", "ท้องเสีย", "เป็นหวัด",
    # added when the course grew to 15 words per unit (loanwords and compounds are the least certain)
    "ฟรี", "วิดีโอ", "ลิงก์", "แชร์", "ลิฟต์", "ทัวร์", "โซฟา", "อีเมล", "วัคซีน", "ดอลลาร์", "สปา", "ตรวจ", "เหตุผล",
    "สงสาร", "อาศัย", "ภูมิใจ", "รหัส", "ปลอดภัย", "ตักบาตร", "ผู้โดยสาร", "แผงลอย", "ของฝาก", "ยังไง", "ราตรีสวัสดิ์",
    "แปรงฟัน", "ซักผ้า", "พัดลม", "สระว่ายน้ำ", "รถไฟฟ้า", "นักท่องเที่ยว", "น้ำผลไม้", "ส้มตำ", "เที่ยวบิน", "ผ่อนคลาย",
}
SHAKY_LETTERS = {"ฏ", "ฑ", "ฌ", "ฒ"}

HEADER = [
    "구분 (Type)", "단원 (Unit)", "번호 (No.)", "태국어 · 남성형 (Thai, male form)", "태국어 · 여성형 (Thai, female form)",
    "한글 발음 (Hangul pronunciation)", "성조 (Tones)", "한국어 뜻 (Korean meaning)",
    "작성자가 특히 확신 낮음 (Author unsure)", "맞음? O/X (Correct?)", "수정 제안 (Correction)", "메모 (Notes)",
]


def speech_id(text):
    h = 0xCBF29CE484222325
    for b in text.encode("utf-8"):
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def main():
    if not os.path.exists(BIN):
        sys.exit("Run `cargo build` first.")
    rows = [l.split("\t") for l in subprocess.check_output([BIN, "--dump-content"], text=True).splitlines() if l.strip()]
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "sawasdee-review.csv"), "w", newline="", encoding="utf-8-sig") as f:
        w = csv.writer(f)
        w.writerow(HEADER)
        for r in rows:
            r = (r + [""] * 9)[:9]
            kind, thai = r[0], r[3].replace(" ", "")
            shaky = (kind == "문장" and thai in SHAKY_SENTENCES) or (kind == "단어" and thai in SHAKY_WORDS) or (kind == "자음" and r[2] in SHAKY_LETTERS)
            w.writerow(r[:8] + ["예 (yes)" if shaky else "", "", "", ""])
    print(f"{len(rows)} rows -> docs/review/sawasdee-review.csv")

    # voice clips named by Thai text, for listening checks
    if not os.path.isdir(WORK):
        print("audio_work/ not found: skipping audio samples")
        return
    clips = {}
    for name in os.listdir(WORK):
        if name.endswith(".mp3"):
            clips[name.split("-")[0]] = os.path.join(WORK, name)
    audio_dir = os.path.join(OUT, "audio")
    shutil.rmtree(audio_dir, ignore_errors=True)
    os.makedirs(audio_dir)
    n = 0
    seen = set()
    for r in rows:
        for text in {r[3], r[4] if len(r) > 4 else ""}:
            text = text.strip()
            if not text or text in seen:
                continue
            src = clips.get(f"{speech_id(text):016x}")
            if src:
                seen.add(text)
                n += 1
                safe = text.replace("/", "_")[:40]
                shutil.copyfile(src, os.path.join(audio_dir, f"{n:04d}_{safe}.mp3"))
    zpath = os.path.join(OUT, "sawasdee-review-audio.zip")
    with zipfile.ZipFile(zpath, "w", zipfile.ZIP_STORED) as z:
        for name in sorted(os.listdir(audio_dir)):
            z.write(os.path.join(audio_dir, name), name)
    print(f"{n} audio clips -> docs/review/audio/ and sawasdee-review-audio.zip ({os.path.getsize(zpath) / 1e6:.1f} MB)")


main()
