# 사왓디 (Sawasdee) — 한국인을 위한 태국어

태국 글자 읽기부터 여행·생활 회화까지, 한국어 설명으로 배우는 태국어 학습 앱입니다. Rust + egui로 만들었고 안드로이드(APK)·아이폰·데스크톱에서 같은 코드로 돌아갑니다.

> **시험 단계입니다.** 음성은 AI 합성(Google Cloud Chirp 3 HD)이고, 단어·문장·한글 발음·성조는 태국어 원어민의 검수를 아직 받지 않았습니다. 틀린 곳을 발견하면 이슈로 알려 주세요.

## 안드로이드 시험 설치
[릴리스 페이지](https://github.com/chrisguy7707-ui/sawasdee/releases/latest)에서 `Sawasdee.apk`를 내려받아 설치하세요(안드로이드 8.0 이상, "출처 불명 앱 설치" 허용 필요). 자세한 시험 안내와 설문은 [docs/BETA.md](docs/BETA.md).

## 들어 있는 것
- 34개 단원, 단어 510개, 문장 370여 개, 짧은 대화 연습
- 알파벳 탭: 자음 44자·모음 23개·받침·성조 규칙·읽기 퀴즈
- 하루 한 번 학습 알림(켜는 경우에만 알림 권한 요청), 학습 기록은 기기 안에만 저장
- 광고·로그인·통신 없음

## 빌드
| 대상 | 방법 |
|---|---|
| 데스크톱 | `cargo run` |
| 안드로이드 | `sh tools/build_android.sh` (APK와 구글 플레이용 AAB 생성, 서명 키는 직접 준비: [docs/SIGNING.md](docs/SIGNING.md)) |
| 아이폰 | [ios/README.md](ios/README.md) (Xcode, 실기기는 애플 개발자 서명 필요) |

검사: `cargo build && ./target/debug/sawasdee --selftest`, `python3 tools/check_tones.py`(성조·발음 규칙 검증)

## 문서
[약점 보완 계획](docs/IMPROVEMENT_PLAN.md) · [커리큘럼](docs/CURRICULUM.md) · [개인정보 처리방침 초안](docs/store/PRIVACY_POLICY.md) · [출시 체크리스트](docs/store/LAUNCH_CHECKLIST.md) · [오픈소스 라이선스 고지](assets/legal/licenses.txt)

## 라이선스
아직 정하지 않았습니다. 정해지기 전까지 코드와 콘텐츠의 모든 권리는 저작자에게 있습니다.
