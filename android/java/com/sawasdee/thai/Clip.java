package com.sawasdee.thai;

import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;

/** Copy to / read from the clipboard, used for the learning-record backup code. Called from Rust through JNI. */
public final class Clip {
    private Clip() {}

    public static void set(Context ctx, String text) {
        ClipboardManager cm = (ClipboardManager) ctx.getSystemService(Context.CLIPBOARD_SERVICE);
        if (cm != null) cm.setPrimaryClip(ClipData.newPlainText("sawasdee", text));
    }

    /** The clipboard text, or an empty string. */
    public static String get(Context ctx) {
        ClipboardManager cm = (ClipboardManager) ctx.getSystemService(Context.CLIPBOARD_SERVICE);
        if (cm == null || !cm.hasPrimaryClip()) return "";
        ClipData d = cm.getPrimaryClip();
        if (d == null || d.getItemCount() == 0) return "";
        CharSequence t = d.getItemAt(0).coerceToText(ctx);
        return t == null ? "" : t.toString();
    }
}
