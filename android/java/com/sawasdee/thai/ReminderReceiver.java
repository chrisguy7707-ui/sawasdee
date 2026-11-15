package com.sawasdee.thai;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

/** Shows the reminder notification when an alarm fires. Tapping it opens the app. */
public final class ReminderReceiver extends BroadcastReceiver {
    static final String CHANNEL = "reminder";

    @Override
    public void onReceive(Context c, Intent intent) {
        String title = intent.getStringExtra("title");
        String body = intent.getStringExtra("body");
        if (title == null) title = "사왓디";
        if (body == null) body = "오늘의 태국어를 해 볼까요?";
        NotificationManager nm = (NotificationManager) c.getSystemService(Context.NOTIFICATION_SERVICE);
        if (nm == null) return;
        NotificationChannel ch = new NotificationChannel(CHANNEL, "학습 알림", NotificationManager.IMPORTANCE_DEFAULT);
        ch.setDescription("하루 한 번 학습 시간을 알려 줘요");
        nm.createNotificationChannel(ch);
        Intent launch = c.getPackageManager().getLaunchIntentForPackage(c.getPackageName());
        PendingIntent open = PendingIntent.getActivity(c, 0, launch,
                PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        int small = c.getResources().getIdentifier("ic_stat", "drawable", c.getPackageName());
        if (small == 0) small = c.getApplicationInfo().icon;
        Notification n = new Notification.Builder(c, CHANNEL)
                .setSmallIcon(small)
                .setContentTitle(title)
                .setContentText(body)
                .setContentIntent(open)
                .setAutoCancel(true)
                .build();
        nm.notify(1001, n);
    }
}
