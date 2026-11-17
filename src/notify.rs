//! Daily study reminder, one small interface over the phone's own notification system.
//! Also the clipboard (for the learning-record backup code), which needs the same native hookup.
//!
//! * iPhone: UserNotifications, called through ios/Sawasdee/main.m.
//! * Android: `Reminder.java` (alarms) and `ReminderReceiver.java`, called through JNI.
//! * Computers: nothing (`supported()` is false and the other functions do nothing).
//!
//! The next 7 days are booked at once, so a day that is already studied can be skipped and a
//! person who stops opening the app gets at most 7 reminders.

/// Title and messages of the reminder (one message is used per day, in turn).
pub const TITLE: &str = "사왓디";
pub const MESSAGES: &[&str] = &[
    "สวัสดี! 오늘의 태국어 10분, 시작해 볼까요?",
    "레슨 하나만 해도 충분해요. 오늘도 한 걸음!",
    "복습할 단어가 기다리고 있어요.",
    "틀렸던 문제를 다시 풀어 볼까요?",
    "연속 학습을 이어 가 볼까요?",
    "알파벳 퀴즈로 가볍게 시작해 보세요.",
];

/// Minutes after midnight shown as text, e.g. 1230 -> "오후 8:30".
pub fn time_text(minutes: u32) -> String {
    let (h, m) = (minutes / 60 % 24, minutes % 60);
    let (ap, h12) = if h < 12 { ("오전", if h == 0 { 12 } else { h }) } else { ("오후", if h == 12 { 12 } else { h - 12 }) };
    format!("{ap} {h12}:{m:02}")
}

pub fn supported() -> bool {
    cfg!(any(target_os = "android", target_os = "ios"))
}

/// Asks the person for permission to show notifications (the system asks once).
pub fn request_permission() {
    imp::request_permission();
}

/// Books the reminders for the next 7 days at `minutes` after midnight.
/// `skip_today`: the person already studied today.
pub fn schedule(minutes: u32, skip_today: bool) {
    imp::schedule(minutes, TITLE, &MESSAGES.join("\n"), skip_today);
}

pub fn cancel() {
    imp::cancel();
}

/// Shows one notification right away, to check that notifications work.
pub fn test() {
    imp::test(TITLE, MESSAGES[0]);
}

/// Copies text to the phone's clipboard. Returns false where there is no native clipboard (computers).
pub fn clip_set(text: &str) -> bool {
    imp::clip_set(text)
}

/// The phone's clipboard text, if any (computers: None).
pub fn clip_get() -> Option<String> {
    imp::clip_get()
}

/// Result of asking to start recording the learner's voice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RecStart {
    Ok,
    /// The system is asking for microphone permission; the learner taps record again after allowing.
    NeedPermission,
    Unsupported,
    Error,
}

/// Recording the learner's voice works on phones only.
pub fn rec_supported() -> bool {
    supported()
}
pub fn rec_start() -> RecStart {
    imp::rec_start()
}
pub fn rec_stop() {
    imp::rec_stop();
}
pub fn rec_play() {
    imp::rec_play();
}
pub fn rec_has() -> bool {
    imp::rec_has()
}
pub fn rec_clear() {
    imp::rec_clear();
}
/// Path of the learner's recording (16-bit mono WAV), if there is one.
pub fn rec_path() -> Option<String> {
    imp::rec_path()
}

/// The phone's text size setting as a factor (1.0 = normal; computers: 1.0).
pub fn font_scale() -> f32 {
    imp::font_scale().clamp(1.0, 2.0)
}

/// A demo screen to open, given when the app is launched for store pictures
/// (Android: intent extra `sawasdee_open`; iPhone: launch argument `--open <screen>`). None normally.
pub fn launch_open() -> Option<String> {
    imp::launch_open().filter(|s| !s.is_empty())
}

/// True once each time the Back button/gesture was used (Android only).
pub fn back_pressed() -> bool {
    imp::back_pressed()
}
/// Closes the app (Android only; Back on the first screen).
pub fn exit_app() {
    imp::exit_app();
}

// ------------------------------------------------------------ paid unlock (Google Play Billing)
/// True only in the Google Play build (`--features play`), where units after the free ones need a purchase.
pub fn paywall_enabled() -> bool {
    cfg!(all(target_os = "android", feature = "play"))
}
/// Connects to Google Play and checks what the account owns (no sign-in or server of our own).
pub fn billing_init() {
    imp::billing_init();
}
/// "구매 복원": asks Google Play again which purchases the account owns.
pub fn billing_refresh() {
    imp::billing_refresh();
}
/// Opens Google Play's purchase sheet.
pub fn billing_buy() {
    imp::billing_buy();
}
/// 0 = not ready, 1 = can buy, 2 = owned, 3 = payment pending.
pub fn billing_state() -> i32 {
    imp::billing_state()
}
/// Price in the person's currency, "" until Google Play has answered.
pub fn billing_price() -> String {
    imp::billing_price()
}
/// A message to show once (purchase finished or failed), "" if none.
pub fn billing_message() -> String {
    imp::billing_message()
}

// ------------------------------------------------------------------------- iPhone
#[cfg(target_os = "ios")]
mod imp {
    use std::ffi::CString;
    use std::os::raw::c_char;

    extern "C" {
        fn sawasdee_notify_request();
        fn sawasdee_notify_schedule(minutes: i32, title: *const c_char, bodies: *const c_char, skip_today: i32);
        fn sawasdee_notify_cancel();
        fn sawasdee_notify_test(title: *const c_char, body: *const c_char);
        fn sawasdee_clip_set(text: *const c_char);
        fn sawasdee_clip_get() -> *mut c_char;
        fn free(p: *mut std::os::raw::c_void);
        fn sawasdee_rec_start() -> i32;
        fn sawasdee_rec_stop();
        fn sawasdee_rec_play();
        fn sawasdee_rec_has() -> i32;
        fn sawasdee_rec_clear();
        fn sawasdee_rec_path() -> *mut c_char;
        fn sawasdee_font_scale() -> f32;
    }

    pub fn rec_path() -> Option<String> {
        // SAFETY: main.m returns a malloc'ed C string (or null), freed here.
        unsafe {
            let p = sawasdee_rec_path();
            if p.is_null() {
                return None;
            }
            let s = std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned();
            free(p as *mut std::os::raw::c_void);
            Some(s)
        }
    }

    pub fn launch_open() -> Option<String> {
        let args: Vec<String> = std::env::args().collect();
        args.iter().position(|a| a == "--open").and_then(|i| args.get(i + 1).cloned())
    }
    pub fn font_scale() -> f32 {
        // SAFETY: plain function from main.m.
        unsafe { sawasdee_font_scale() }
    }

    pub fn billing_init() {}
    pub fn billing_refresh() {}
    pub fn billing_buy() {}
    pub fn billing_state() -> i32 {
        0
    }
    pub fn billing_price() -> String {
        String::new()
    }
    pub fn billing_message() -> String {
        String::new()
    }
    pub fn back_pressed() -> bool {
        false
    }
    pub fn exit_app() {}

    pub fn rec_start() -> super::RecStart {
        // SAFETY: plain functions from ios/Sawasdee/main.m.
        match unsafe { sawasdee_rec_start() } {
            0 => super::RecStart::Ok,
            1 => super::RecStart::NeedPermission,
            _ => super::RecStart::Error,
        }
    }
    pub fn rec_stop() {
        unsafe { sawasdee_rec_stop() }
    }
    pub fn rec_play() {
        unsafe { sawasdee_rec_play() }
    }
    pub fn rec_has() -> bool {
        unsafe { sawasdee_rec_has() != 0 }
    }
    pub fn rec_clear() {
        unsafe { sawasdee_rec_clear() }
    }

    pub fn clip_set(text: &str) -> bool {
        let t = CString::new(text.replace('\0', "")).unwrap_or_default();
        // SAFETY: NUL-terminated string that lives until the call returns.
        unsafe { sawasdee_clip_set(t.as_ptr()) };
        true
    }
    pub fn clip_get() -> Option<String> {
        // SAFETY: main.m returns a malloc'ed C string (or null), freed here.
        unsafe {
            let p = sawasdee_clip_get();
            if p.is_null() {
                return None;
            }
            let s = std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned();
            free(p as *mut std::os::raw::c_void);
            Some(s)
        }
    }

    pub fn request_permission() {
        // SAFETY: plain function from ios/Sawasdee/main.m.
        unsafe { sawasdee_notify_request() }
    }
    pub fn schedule(minutes: u32, title: &str, bodies: &str, skip_today: bool) {
        let (t, b) = (CString::new(title).unwrap_or_default(), CString::new(bodies).unwrap_or_default());
        // SAFETY: both strings are NUL-terminated and live until the call returns.
        unsafe { sawasdee_notify_schedule(minutes as i32, t.as_ptr(), b.as_ptr(), skip_today as i32) }
    }
    pub fn cancel() {
        // SAFETY: plain function from main.m.
        unsafe { sawasdee_notify_cancel() }
    }
    pub fn test(title: &str, body: &str) {
        let (t, b) = (CString::new(title).unwrap_or_default(), CString::new(body).unwrap_or_default());
        // SAFETY: as above.
        unsafe { sawasdee_notify_test(t.as_ptr(), b.as_ptr()) }
    }
}

// ------------------------------------------------------------------------ Android
#[cfg(target_os = "android")]
mod imp {
    use jni::objects::{JClass, JObject, JValue};
    use jni::{JNIEnv, JavaVM};
    use std::os::raw::c_void;
    use std::sync::OnceLock;

    static CTX: OnceLock<(usize, usize)> = OnceLock::new();

    /// Called once from `android_main` with the Java VM and the activity.
    pub fn init(vm: *mut c_void, activity: *mut c_void) {
        let _ = CTX.set((vm as usize, activity as usize));
    }

    fn with_env<R>(f: impl FnOnce(&mut JNIEnv, &JObject, &JClass) -> jni::errors::Result<R>) -> Option<R> {
        with_class("com.sawasdee.thai.Reminder", f)
    }

    fn with_class<R>(class: &str, f: impl FnOnce(&mut JNIEnv, &JObject, &JClass) -> jni::errors::Result<R>) -> Option<R> {
        let &(vm, act) = CTX.get()?;
        // SAFETY: the pointers come from the running activity and stay valid for the whole process.
        let vm = unsafe { JavaVM::from_raw(vm as *mut _) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        let activity = unsafe { JObject::from_raw(act as *mut _) };
        // A native thread only sees system classes; our Reminder class is found through the activity's class loader.
        let result = (|| {
            let loader = env.call_method(&activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?.l()?;
            let name = env.new_string(class)?;
            let cls = env.call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&name)])?.l()?;
            let cls = JClass::from(cls);
            f(&mut env, &activity, &cls)
        })();
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        result.ok()
    }

    pub fn launch_open() -> Option<String> {
        with_class("com.sawasdee.thai.MainActivity", |env, act, cls| {
            let o = env.call_static_method(cls, "openArg", "(Landroid/app/Activity;)Ljava/lang/String;", &[JValue::Object(act)])?.l()?;
            let s: String = env.get_string(&jni::objects::JString::from(o))?.into();
            Ok(s)
        })
    }
    pub fn font_scale() -> f32 {
        with_class("com.sawasdee.thai.MainActivity", |env, act, cls| Ok(env.call_static_method(cls, "fontScale", "(Landroid/app/Activity;)F", &[JValue::Object(act)])?.f()?)).unwrap_or(1.0)
    }

    pub fn billing_init() {
        with_class("com.sawasdee.thai.Billing", |env, act, cls| {
            env.call_static_method(cls, "init", "(Landroid/app/Activity;)V", &[JValue::Object(act)])?;
            Ok(())
        });
    }
    pub fn billing_refresh() {
        with_class("com.sawasdee.thai.Billing", |env, act, cls| {
            env.call_static_method(cls, "refresh", "(Landroid/app/Activity;)V", &[JValue::Object(act)])?;
            Ok(())
        });
    }
    pub fn billing_buy() {
        with_class("com.sawasdee.thai.Billing", |env, act, cls| {
            env.call_static_method(cls, "buy", "(Landroid/app/Activity;)V", &[JValue::Object(act)])?;
            Ok(())
        });
    }
    pub fn billing_state() -> i32 {
        with_class("com.sawasdee.thai.Billing", |env, _act, cls| Ok(env.call_static_method(cls, "state", "()I", &[])?.i()?)).unwrap_or(0)
    }
    fn billing_text(name: &str) -> String {
        with_class("com.sawasdee.thai.Billing", |env, _act, cls| {
            let o = env.call_static_method(cls, name, "()Ljava/lang/String;", &[])?.l()?;
            let s: String = env.get_string(&jni::objects::JString::from(o))?.into();
            Ok(s)
        })
        .unwrap_or_default()
    }
    pub fn billing_price() -> String {
        billing_text("price")
    }
    pub fn billing_message() -> String {
        billing_text("takeMessage")
    }
    pub fn back_pressed() -> bool {
        with_class("com.sawasdee.thai.MainActivity", |env, _act, cls| Ok(env.call_static_method(cls, "consumeBack", "()Z", &[])?.z()?)).unwrap_or(false)
    }
    pub fn exit_app() {
        with_class("com.sawasdee.thai.MainActivity", |env, act, cls| {
            env.call_static_method(cls, "exitApp", "(Landroid/app/Activity;)V", &[JValue::Object(act)])?;
            Ok(())
        });
    }

    pub fn rec_start() -> super::RecStart {
        match with_class("com.sawasdee.thai.Recorder", |env, act, cls| {
            Ok(env.call_static_method(cls, "start", "(Landroid/app/Activity;)I", &[JValue::Object(act)])?.i()?)
        }) {
            Some(0) => super::RecStart::Ok,
            Some(1) => super::RecStart::NeedPermission,
            _ => super::RecStart::Error,
        }
    }
    pub fn rec_stop() {
        with_class("com.sawasdee.thai.Recorder", |env, _act, cls| {
            env.call_static_method(cls, "stop", "()V", &[])?;
            Ok(())
        });
    }
    pub fn rec_play() {
        with_class("com.sawasdee.thai.Recorder", |env, act, cls| {
            env.call_static_method(cls, "play", "(Landroid/app/Activity;)V", &[JValue::Object(act)])?;
            Ok(())
        });
    }
    pub fn rec_has() -> bool {
        with_class("com.sawasdee.thai.Recorder", |env, _act, cls| Ok(env.call_static_method(cls, "hasRecording", "()Z", &[])?.z()?)).unwrap_or(false)
    }
    pub fn rec_path() -> Option<String> {
        with_class("com.sawasdee.thai.Recorder", |env, _act, cls| {
            let o = env.call_static_method(cls, "path", "()Ljava/lang/String;", &[])?.l()?;
            let s: String = env.get_string(&jni::objects::JString::from(o))?.into();
            Ok(s)
        })
        .filter(|s| !s.is_empty())
    }
    pub fn rec_clear() {
        with_class("com.sawasdee.thai.Recorder", |env, _act, cls| {
            env.call_static_method(cls, "clear", "()V", &[])?;
            Ok(())
        });
    }

    pub fn clip_set(text: &str) -> bool {
        with_class("com.sawasdee.thai.Clip", |env, act, cls| {
            let t = env.new_string(text)?;
            env.call_static_method(cls, "set", "(Landroid/content/Context;Ljava/lang/String;)V", &[JValue::Object(act), JValue::Object(&t)])?;
            Ok(())
        })
        .is_some()
    }
    pub fn clip_get() -> Option<String> {
        with_class("com.sawasdee.thai.Clip", |env, act, cls| {
            let o = env.call_static_method(cls, "get", "(Landroid/content/Context;)Ljava/lang/String;", &[JValue::Object(act)])?.l()?;
            let s: String = env.get_string(&jni::objects::JString::from(o))?.into();
            Ok(s)
        })
    }

    pub fn request_permission() {
        with_env(|env, act, cls| {
            env.call_static_method(cls, "requestPermission", "(Landroid/app/Activity;)V", &[JValue::Object(act)])?;
            Ok(())
        });
    }
    pub fn schedule(minutes: u32, title: &str, bodies: &str, skip_today: bool) {
        with_env(|env, act, cls| {
            let (t, b) = (env.new_string(title)?, env.new_string(bodies)?);
            env.call_static_method(
                cls,
                "schedule",
                "(Landroid/content/Context;ILjava/lang/String;Ljava/lang/String;Z)V",
                &[JValue::Object(act), JValue::Int(minutes as i32), JValue::Object(&t), JValue::Object(&b), JValue::Bool(skip_today as u8)],
            )?;
            Ok(())
        });
    }
    pub fn cancel() {
        with_env(|env, act, cls| {
            env.call_static_method(cls, "cancel", "(Landroid/content/Context;)V", &[JValue::Object(act)])?;
            Ok(())
        });
    }
    pub fn test(title: &str, body: &str) {
        with_env(|env, act, cls| {
            let (t, b) = (env.new_string(title)?, env.new_string(body)?);
            env.call_static_method(
                cls,
                "test",
                "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)V",
                &[JValue::Object(act), JValue::Object(&t), JValue::Object(&b)],
            )?;
            Ok(())
        });
    }
}

#[cfg(target_os = "android")]
pub use imp::init as init_android;

// ---------------------------------------------------------------------- computers
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod imp {
    pub fn launch_open() -> Option<String> {
        None
    }
    pub fn font_scale() -> f32 {
        1.0
    }

    pub fn billing_init() {}
    pub fn billing_refresh() {}
    pub fn billing_buy() {}
    pub fn billing_state() -> i32 {
        0
    }
    pub fn billing_price() -> String {
        String::new()
    }
    pub fn billing_message() -> String {
        String::new()
    }
    pub fn back_pressed() -> bool {
        false
    }
    pub fn exit_app() {}
    pub fn rec_start() -> super::RecStart {
        super::RecStart::Unsupported
    }
    pub fn rec_stop() {}
    pub fn rec_play() {}
    pub fn rec_has() -> bool {
        false
    }
    pub fn rec_clear() {}
    pub fn rec_path() -> Option<String> {
        None
    }
    pub fn clip_set(_text: &str) -> bool {
        false
    }
    pub fn clip_get() -> Option<String> {
        None
    }
    pub fn request_permission() {}
    pub fn schedule(_minutes: u32, _title: &str, _bodies: &str, _skip_today: bool) {}
    pub fn cancel() {}
    pub fn test(_title: &str, _body: &str) {}
}
