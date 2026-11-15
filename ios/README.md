# 사왓디 iPhone 앱

앱 전체는 Rust(`../src`)로 만들어져 있고, 이 폴더는 그것을 iPhone 앱으로 감싸는 Xcode 프로젝트입니다.

## 필요한 것
- Xcode (iOS 시뮬레이터 포함), Rust(`rustup`), `xcodegen` (`brew install xcodegen`)
- Rust iOS 대상: `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`

## 시뮬레이터에서 실행
```sh
cd ios
xcodegen generate            # project.yml → Sawasdee.xcodeproj (설정을 바꿨을 때만)
xcodebuild -project Sawasdee.xcodeproj -scheme Sawasdee -sdk iphonesimulator \
  -configuration Release -derivedDataPath build CODE_SIGNING_ALLOWED=NO build
xcrun simctl boot "iPhone 18 Pro"        # 이미 켜져 있으면 생략
xcrun simctl install booted build/Build/Products/Release-iphonesimulator/Sawasdee.app
xcrun simctl launch booted com.sawasdee.thai
```
또는 `Sawasdee.xcodeproj`를 Xcode로 열어 ▶ 를 누르면 됩니다(빌드 전에 Rust 라이브러리를 자동으로 만듭니다: `build_rust.sh`).

## 실제 아이폰 / App Store
Xcode의 Signing & Capabilities에서 Apple Developer 계정(Team)을 고르면 됩니다. 무료 계정은 기기에 7일간 설치,
App Store 배포는 유료 계정(연 $99)이 필요합니다. 코드 서명 없이 기기용으로 컴파일되는 것까지는 확인했습니다.

## 알아 둘 것 (iOS 27 대응)
- iOS 27은 UIScene 방식이 필수인데 Rust 창 라이브러리(winit 0.30)는 옛 방식입니다. 그래서 `Sawasdee/main.m`이
  장면(Scene) 대리자를 두고, 만들어지는 창을 그 장면에 붙입니다. `Info.plist`의 `UIApplicationSceneManifest`가 짝입니다.
- 같은 파일에서 winit에는 안전 영역을 0으로 알려 줍니다(안 그러면 그림이 세로로 늘어나고 아래가 잘림).
  노치·홈 바 여백은 앱이 `sawasdee_safe_insets()`로 직접 읽어 비웁니다(`src/app.rs`의 `ios_top`·`ios_bottom`).
- 화면 그리기는 Metal(wgpu)입니다. 시뮬레이터 그래픽이 허용하는 한도에 맞춰 요청합니다(`src/lib.rs`).
- 시작에 문제가 생기면 앱 안 `Documents/sawasdee.log`에 오류가 남습니다(`xcrun simctl get_app_container booted com.sawasdee.thai data`).
- iPhone에서는 "앱 종료" 버튼을 숨깁니다(애플 심사 기준).
