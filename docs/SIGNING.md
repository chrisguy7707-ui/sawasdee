# 안드로이드 서명 키 관리

앱 업데이트는 **처음 올린 것과 같은 서명 키**로 서명해야 설치된 앱 위에 덮어쓸 수 있다. 키를 잃어버리면 같은 앱으로 업데이트를 올릴 수 없다.

## 어디에 있나
| 파일 | 내용 | 저장소에 올리나 |
|---|---|---|
| `android/release.keystore` | 서명 키 | 아니오 (git에서 제외) |
| `android/keystore.password` | 키 파일 비밀번호(한 줄) | 아니오 (git에서 제외) |
| `android/release.keystore.bak` | 키 파일 사본 | 아니오 (git에서 제외) |

`tools/build_android.sh`가 두 파일을 읽어 서명한다. 비밀번호는 `Cargo.toml`에 없다.

## 꼭 할 일
1. `release.keystore`와 `keystore.password`를 **저장소 밖의 안전한 곳 두 군데 이상**(예: 비밀번호 관리자, 암호화된 외장 디스크)에 백업.
2. 두 파일을 메신저·이메일·공개 저장소·AI 채팅에 올리지 않기.
3. Google Play에 올릴 때는 "Play 앱 서명"을 켜면 업로드 키를 바꿀 수 있어 위험이 줄어든다(스토어 설정 단계에서 결정).

## 인증서 지문 (확인용)
SHA-256: `151d860253a9ad57cf4ff417e9c50e27645d0b21ee00030ddff6bedad3f49a3f`
(`apksigner verify --print-certs dist/Sawasdee.apk`로 같은 값이 나와야 같은 키로 서명된 것)

## 비밀번호를 바꾸려면
```sh
keytool -storepasswd -keystore android/release.keystore
```
바꾼 뒤 `android/keystore.password`도 같은 값으로 고친다.
