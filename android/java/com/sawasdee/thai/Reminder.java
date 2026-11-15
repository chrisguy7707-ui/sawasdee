package com.sawasdee.thai;

import android.Manifest;
import android.app.Activity;
import android.app.AlarmManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.os.Build;

import java.util.Calendar;

/**
 * Daily study reminder. The Rust app calls these static methods through JNI.
 * It books the next 7 days as separate alarms (so a day already studied can be skipped, and a
 * person who stops opening the app gets at most 7 reminders, not an endless stream).
 */
public final class Reminder {
    static final String PREFS = "sawasdee_reminder";
    static final int DAYS = 7;
    static final int FIRST_CODE = 100;

    private Reminder() {}

    /** minutes: minutes after midnight. bodies: messages separated by newlines (one is picked per day). */
    public static void schedule(Context ctx, int minutes, String title, String bodies, boolean skipToday) {
        cancelAlarms(ctx);
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit()
                .putBoolean("on", true).putInt("minutes", minutes)
                .putString("title", title).putString("bodies", bodies).apply();
        AlarmManager am = (AlarmManager) ctx.getSystemService(Context.ALARM_SERVICE);
        if (am == null) return;
        long now = System.currentTimeMillis();
        int dayOfYear = Calendar.getInstance().get(Calendar.DAY_OF_YEAR);
        for (int d = 0; d < DAYS; d++) {
            Calendar t = Calendar.getInstance();
            t.add(Calendar.DAY_OF_YEAR, d);
            t.set(Calendar.HOUR_OF_DAY, minutes / 60);
            t.set(Calendar.MINUTE, minutes % 60);
            t.set(Calendar.SECOND, 0);
            t.set(Calendar.MILLISECOND, 0);
            if (t.getTimeInMillis() <= now + 30000) continue;
            if (d == 0 && skipToday) continue;
            Intent i = new Intent(ctx, ReminderReceiver.class);
            i.putExtra("title", title);
            i.putExtra("body", pick(bodies, dayOfYear + d));
            PendingIntent pi = PendingIntent.getBroadcast(ctx, FIRST_CODE + d, i,
                    PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
            // Not exact on purpose: no special permission is needed, a few minutes of drift is fine.
            am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, t.getTimeInMillis(), pi);
        }
    }

    public static void cancel(Context ctx) {
        cancelAlarms(ctx);
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean("on", false).apply();
    }

    static void cancelAlarms(Context ctx) {
        AlarmManager am = (AlarmManager) ctx.getSystemService(Context.ALARM_SERVICE);
        if (am == null) return;
        for (int d = 0; d < DAYS; d++) {
            Intent i = new Intent(ctx, ReminderReceiver.class);
            PendingIntent pi = PendingIntent.getBroadcast(ctx, FIRST_CODE + d, i,
                    PendingIntent.FLAG_NO_CREATE | PendingIntent.FLAG_IMMUTABLE);
            if (pi != null) {
                am.cancel(pi);
                pi.cancel();
            }
        }
    }

    /** Shows a notification right away (the "알림 시험" button). */
    public static void test(Context ctx, String title, String body) {
        Intent i = new Intent(ctx, ReminderReceiver.class);
        i.putExtra("title", title);
        i.putExtra("body", body);
        ctx.sendBroadcast(i);
    }

    public static boolean hasPermission(Context ctx) {
        if (Build.VERSION.SDK_INT < 33) return true;
        return ctx.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED;
    }

    /** Android 13+ asks the person once; older versions need nothing. */
    public static void requestPermission(Activity a) {
        if (Build.VERSION.SDK_INT >= 33 && !hasPermission(a)) {
            a.requestPermissions(new String[] {Manifest.permission.POST_NOTIFICATIONS}, 7001);
        }
    }

    static String pick(String bodies, int index) {
        String[] a = bodies.split("\n");
        return a[Math.floorMod(index, a.length)];
    }

    /** Re-books the alarms from the saved settings (after a reboot or an app update). */
    static void restore(Context ctx) {
        SharedPreferences p = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        if (p.getBoolean("on", false)) {
            schedule(ctx, p.getInt("minutes", 20 * 60), p.getString("title", "사왓디"),
                    p.getString("bodies", "오늘의 태국어를 해 볼까요?"), false);
        }
    }
}
