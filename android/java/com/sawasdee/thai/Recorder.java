package com.sawasdee.thai;

import android.Manifest;
import android.app.Activity;
import android.content.Context;
import android.content.pm.PackageManager;
import android.media.MediaPlayer;
import android.media.MediaRecorder;
import android.os.Build;

import java.io.File;

/**
 * Record the learner's own voice and play it back next to the native recording.
 * The file lives in the app's private cache folder, is never sent anywhere, and is replaced by the next
 * recording. Called from Rust through JNI. start() returns 0 = recording, 1 = permission needed (the
 * system asks; tap again after allowing), 2 = error.
 */
public final class Recorder {
    private static MediaRecorder rec;
    private static MediaPlayer player;
    private static File file;
    private static boolean has;

    private Recorder() {}

    private static File target(Context c) {
        if (file == null) file = new File(c.getCacheDir(), "my_voice.m4a");
        return file;
    }

    public static synchronized int start(Activity a) {
        if (Build.VERSION.SDK_INT >= 23 && a.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            a.requestPermissions(new String[] {Manifest.permission.RECORD_AUDIO}, 7);
            return 1;
        }
        stopPlay();
        release();
        try {
            rec = Build.VERSION.SDK_INT >= 31 ? new MediaRecorder(a) : new MediaRecorder();
            rec.setAudioSource(MediaRecorder.AudioSource.MIC);
            rec.setOutputFormat(MediaRecorder.OutputFormat.MPEG_4);
            rec.setAudioEncoder(MediaRecorder.AudioEncoder.AAC);
            rec.setAudioSamplingRate(44100);
            rec.setAudioEncodingBitRate(96000);
            rec.setOutputFile(target(a).getAbsolutePath());
            rec.prepare();
            rec.start();
            has = false;
            return 0;
        } catch (Exception e) {
            release();
            return 2;
        }
    }

    public static synchronized void stop() {
        if (rec == null) return;
        try {
            rec.stop();
            has = true;
        } catch (Exception e) {
            has = false; // stopped too quickly: nothing recorded
        }
        release();
    }

    private static void release() {
        if (rec != null) {
            try { rec.release(); } catch (Exception ignored) {}
            rec = null;
        }
    }

    public static synchronized boolean hasRecording() {
        return has && file != null && file.exists();
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
        release();
        has = false;
        if (file != null) file.delete();
    }
}
