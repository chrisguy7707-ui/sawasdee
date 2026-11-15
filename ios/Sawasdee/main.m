// The whole app is written in Rust (src/lib.rs). This file only hands control to it,
// and gives it the "scene" that iOS 27 requires every app to have.
//
// Why: iOS 27 refuses to launch apps that do not use UIKit's scene life cycle, but the
// Rust window library (winit 0.30) still creates its window the old way, without a scene.
// So (1) Info.plist names SawasdeeSceneDelegate, which remembers the scene iOS connects,
// and (2) every UIWindow created afterwards is created inside that scene (see +load below).
#import <UIKit/UIKit.h>
#import <objc/runtime.h>
#import <UserNotifications/UserNotifications.h>

extern void sawasdee_ios_main(void);

static UIWindowScene *gScene = nil;
static BOOL gCreatingInScene = NO;

@interface SawasdeeSceneDelegate : UIResponder <UIWindowSceneDelegate>
@property(nonatomic, strong) UIWindow *window;
@end

@implementation SawasdeeSceneDelegate
- (void)scene:(UIScene *)scene willConnectToSession:(UISceneSession *)session options:(UISceneConnectionOptions *)options {
    if ([scene isKindOfClass:[UIWindowScene class]]) {
        gScene = (UIWindowScene *)scene;
    }
}
@end

@interface UIWindow (SawasdeeScene)
- (UIEdgeInsets)sawasdee_safeAreaInsets;
@end

@implementation UIWindow (SawasdeeScene)
+ (void)load {
    Method a = class_getInstanceMethod([UIWindow class], @selector(initWithFrame:));
    Method b = class_getInstanceMethod([UIWindow class], @selector(sawasdee_initWithFrame:));
    method_exchangeImplementations(a, b);

    // winit sizes the drawing surface from the window's safe area, which leaves the picture
    // too small for the screen (stretched, with the bottom cut off). The app handles the
    // notch and home bar itself (see sawasdee_safe_insets), so winit is told there are none.
    Method c = class_getInstanceMethod([UIWindow class], @selector(safeAreaInsets));
    Method d = class_getInstanceMethod([UIWindow class], @selector(sawasdee_safeAreaInsets));
    method_exchangeImplementations(c, d);
}

// After the exchange, `safeAreaInsets` runs this: no insets.
// The original (real) values are reached through `sawasdee_safeAreaInsets`.
- (UIEdgeInsets)sawasdee_safeAreaInsets {
    return UIEdgeInsetsZero;
}

// After the exchange, this is what `initWithFrame:` runs.
- (instancetype)sawasdee_initWithFrame:(CGRect)frame {
    if (gScene != nil && !gCreatingInScene) {
        gCreatingInScene = YES;
        UIWindow *w = [self initWithWindowScene:gScene];
        gCreatingInScene = NO;
        w.frame = frame;
        return w;
    }
    return [self sawasdee_initWithFrame:frame];  // the original implementation
}
@end

// Space the app must keep clear (notch / Dynamic Island on top, home bar at the bottom), in points.
void sawasdee_safe_insets(float *top, float *bottom) {
    *top = 0;
    *bottom = 0;
    if (gScene != nil) {
        UIWindow *w = gScene.windows.firstObject;
        if (w != nil) {
            UIEdgeInsets i = [w sawasdee_safeAreaInsets];  // the real values
            *top = (float)i.top;
            *bottom = (float)i.bottom;
        }
    }
}

int main(int argc, char *argv[]) {
    @autoreleasepool {
        sawasdee_ios_main();  // starts the UIKit run loop and never returns
    }
    return 0;
}


// ---- Daily study reminder (called from src/notify.rs) ----
// Seven one-time notifications for the next 7 days, so a day already studied can be skipped
// and a person who stops opening the app gets at most 7 reminders.
static NSString *const kReminderPrefix = @"sawasdee.reminder.";

void sawasdee_notify_request(void) {
    [[UNUserNotificationCenter currentNotificationCenter]
        requestAuthorizationWithOptions:(UNAuthorizationOptionAlert | UNAuthorizationOptionSound)
                      completionHandler:^(BOOL granted, NSError *error) {}];
}

void sawasdee_notify_cancel(void) {
    NSMutableArray *ids = [NSMutableArray array];
    for (int i = 0; i < 8; i++) [ids addObject:[NSString stringWithFormat:@"%@%d", kReminderPrefix, i]];
    [[UNUserNotificationCenter currentNotificationCenter] removePendingNotificationRequestsWithIdentifiers:ids];
}

void sawasdee_notify_schedule(int minutes, const char *title, const char *bodies, int skip_today) {
    sawasdee_notify_cancel();
    NSString *t = [NSString stringWithUTF8String:title];
    NSArray<NSString *> *msgs = [[NSString stringWithUTF8String:bodies] componentsSeparatedByString:@"\n"];
    NSCalendar *cal = [NSCalendar currentCalendar];
    NSDate *now = [NSDate date];
    for (int i = 0; i < 7; i++) {
        NSDate *day = [cal dateByAddingUnit:NSCalendarUnitDay value:i toDate:now options:0];
        NSDateComponents *c = [cal components:(NSCalendarUnitYear | NSCalendarUnitMonth | NSCalendarUnitDay) fromDate:day];
        c.hour = minutes / 60;
        c.minute = minutes % 60;
        NSDate *when = [cal dateFromComponents:c];
        if ([when timeIntervalSinceNow] <= 30) continue;              // time already passed today
        if (i == 0 && skip_today) continue;                           // already studied today
        UNMutableNotificationContent *content = [UNMutableNotificationContent new];
        content.title = t;
        content.body = msgs[i % msgs.count];
        content.sound = [UNNotificationSound defaultSound];
        UNCalendarNotificationTrigger *trig = [UNCalendarNotificationTrigger triggerWithDateMatchingComponents:c repeats:NO];
        UNNotificationRequest *req = [UNNotificationRequest requestWithIdentifier:[NSString stringWithFormat:@"%@%d", kReminderPrefix, i]
                                                                          content:content trigger:trig];
        [[UNUserNotificationCenter currentNotificationCenter] addNotificationRequest:req withCompletionHandler:nil];
    }
}

void sawasdee_notify_test(const char *title, const char *body) {
    UNMutableNotificationContent *content = [UNMutableNotificationContent new];
    content.title = [NSString stringWithUTF8String:title];
    content.body = [NSString stringWithUTF8String:body];
    content.sound = [UNNotificationSound defaultSound];
    UNTimeIntervalNotificationTrigger *trig = [UNTimeIntervalNotificationTrigger triggerWithTimeInterval:3 repeats:NO];
    [[UNUserNotificationCenter currentNotificationCenter]
        addNotificationRequest:[UNNotificationRequest requestWithIdentifier:@"sawasdee.test" content:content trigger:trig]
         withCompletionHandler:nil];
}

// While the app is open, still show the notification (iOS hides it by default).
@interface SawasdeeNotifyDelegate : NSObject <UNUserNotificationCenterDelegate>
@end
@implementation SawasdeeNotifyDelegate
- (void)userNotificationCenter:(UNUserNotificationCenter *)center willPresentNotification:(UNNotification *)n
         withCompletionHandler:(void (^)(UNNotificationPresentationOptions))handler {
    handler(UNNotificationPresentationOptionBanner | UNNotificationPresentationOptionList | UNNotificationPresentationOptionSound);
}
@end

__attribute__((constructor)) static void sawasdee_notify_setup(void) {
    static SawasdeeNotifyDelegate *d; d = [SawasdeeNotifyDelegate new];
    [UNUserNotificationCenter currentNotificationCenter].delegate = d;
}

// ---- Clipboard (learning-record backup code) ----
void sawasdee_clip_set(const char *text) {
    [UIPasteboard generalPasteboard].string = [NSString stringWithUTF8String:text];
}

// Returns a malloc'ed UTF-8 copy of the clipboard text, or NULL. The caller frees it.
char *sawasdee_clip_get(void) {
    NSString *s = [UIPasteboard generalPasteboard].string;
    return s ? strdup([s UTF8String]) : NULL;
}
