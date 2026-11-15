// The whole app is written in Rust (src/lib.rs). This file only hands control to it,
// and gives it the "scene" that iOS 27 requires every app to have.
//
// Why: iOS 27 refuses to launch apps that do not use UIKit's scene life cycle, but the
// Rust window library (winit 0.30) still creates its window the old way, without a scene.
// So (1) Info.plist names SawasdeeSceneDelegate, which remembers the scene iOS connects,
// and (2) every UIWindow created afterwards is created inside that scene (see +load below).
#import <UIKit/UIKit.h>
#import <objc/runtime.h>

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
