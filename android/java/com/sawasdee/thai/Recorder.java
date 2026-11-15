package com.sawasdee.thai;

import android.Manifest;
import android.app.Activity;
import android.content.Context;
import android.content.pm.PackageManager;
import android.media.AudioFormat;
import android.media.AudioRecord;
import android.media.MediaPlayer;
import android.media.MediaRecorder;
import android.os.Build;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;

/**
 * Record the learner's own voice (16-bit mono WAV, so the app can draw its pitch curve) and play it back
 * next to the native recording. The file lives in the app's private cache folder, is never sent anywhere,
 * and is replaced by the next recording. Called from Rust through JNI.
 * start() returns 0 = recording, 1 = permission needed (the system asks; tap again after allowing), 2 = error.
 */
public final class Recorder {
    private static final int RATE = 16000;
    private static final int MAX_BYTES = RATE * 2 * 13; // 13 seconds

    private static AudioRecord rec;
    private static Thread worker;
    private static volatile boolean running;
    private static MediaPlayer player;
    private static File file;
    private static boolean has;

    private Recorder() {}

    private static File target(Context c) {
        if (file == null) file = new File(c.getCacheDir(), "my_voice.wav");
        return file;
    }

    public static synchronized int start(final Activity a) {
        if (Build.VERSION.SDK_INT >= 23 && a.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            a.requestPermissions(new String[] {Manifest.permission.RECORD_AUDIO}, 7);
            return 1;
        }
        stopPlay();
        halt();
        try {
            int min = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT);
            if (min <= 0) return 2;
            final int buf = Math.max(min, RATE / 2);
            rec = new AudioRecord(MediaRecorder.AudioSource.MIC, RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, buf);
            if (rec.getState() != AudioRecord.STATE_INITIALIZED) {
                rec.release();
                rec = null;
                return 2;
            }
            final File out = target(a);
            final AudioRecord r = rec;
            has = false;
            running = true;
            r.startRecording();
            worker = new Thread(new Runnable() {
                public void run() {
                    ByteArrayOutputStream bytes = new ByteArrayOutputStream();
                    byte[] chunk = new byte[buf];
                    while (running && bytes.size() < MAX_BYTES) {
                        int n = r.read(chunk, 0, chunk.length);
                        if (n > 0) bytes.write(chunk, 0, n);
                        else if (n < 0) break;
                    }
                    writeWav(out, bytes.toByteArray());
                }
            });
            worker.start();
            return 0;
        } catch (Exception e) {
            halt();
            return 2;
        }
    }

    /** Stops the recording and waits for the file to be written. */
    public static synchronized void stop() {
        Thread w = worker;
        halt();
        has = file != null && file.exists() && file.length() > 44 + RATE / 3 * 2; // at least a third of a second
        if (w != null) {
            // halt() already joined; nothing else to do
        }
    }

    private static void halt() {
        running = false;
        Thread w = worker;
        worker = null;
        if (w != null) {
            try { w.join(2000); } catch (InterruptedException ignored) {}
        }
        if (rec != null) {
            try { rec.stop(); } catch (Exception ignored) {}
            try { rec.release(); } catch (Exception ignored) {}
            rec = null;
        }
    }

    private static void writeWav(File out, byte[] pcm) {
        try {
            ByteBuffer h = ByteBuffer.allocate(44).order(ByteOrder.LITTLE_ENDIAN);
            h.put("RIFF".getBytes()).putInt(36 + pcm.length).put("WAVE".getBytes());
            h.put("fmt ".getBytes()).putInt(16).putShort((short) 1).putShort((short) 1).putInt(RATE).putInt(RATE * 2).putShort((short) 2).putShort((short) 16);
            h.put("data".getBytes()).putInt(pcm.length);
            FileOutputStream f = new FileOutputStream(out);
            f.write(h.array());
            f.write(pcm);
            f.close();
        } catch (Exception ignored) {
        }
    }

    public static synchronized boolean hasRecording() {
        return has && file != null && file.exists();
    }

    /** Full path of the recording, or an empty string. */
    public static synchronized String path() {
        return hasRecording() ? file.getAbsolutePath() : "";
    }

    public static synchronized void play(Activity a) {
        if (!hasRecording()) return;
        stopPlay();
        try {
            player = new MediaPlayer();
            player.setDataSource(target(a).getAbsolutePath());
            player.prepare();
            player.start();
        } catch (Exception e) {
            stopPlay();
        }
    }

    public static synchronized void stopPlay() {
        if (player != null) {
            try { player.release(); } catch (Exception ignored) {}
            player = null;
        }
    }

    public static synchronized void clear() {
        stopPlay();
        halt();
        has = false;
        if (file != null) file.delete();
    }
}
