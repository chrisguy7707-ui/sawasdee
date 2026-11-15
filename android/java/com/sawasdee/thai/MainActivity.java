package com.sawasdee.thai;

import android.app.Activity;
import android.app.NativeActivity;
import android.os.Build;
import android.os.Bundle;
import android.view.KeyEvent;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

/**
 * The app's activity. Since Android 16 the system no longer hands the Back button to a native app as a
 * key press (it just closes the app), so the Back gesture is caught here and the Rust app asks for it
 * through consumeBack() and decides what Back does (leave a lesson, go to the first tab, or exit).
 */
public class MainActivity extends NativeActivity {
    private static volatile boolean back;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        if (Build.VERSION.SDK_INT >= 33) {
            getOnBackInvokedDispatcher().registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, new OnBackInvokedCallback() {
                @Override
                public void onBackInvoked() {
                    back = true;
                }
            });
        }
    }

    @Override
    public boolean onKeyDown(int keyCode, KeyEvent event) {
        if (keyCode == KeyEvent.KEYCODE_BACK) {
            back = true;
            return true;
        }
        return super.onKeyDown(keyCode, event);
    }

    /** True once per Back press. */
    public static boolean consumeBack() {
        boolean b = back;
        back = false;
        return b;
    }

    /** Closes the app (Back pressed on the first screen). */
    public static void exitApp(final Activity a) {
        a.runOnUiThread(new Runnable() {
            public void run() {
                a.finish();
            }
        });
    }
}
