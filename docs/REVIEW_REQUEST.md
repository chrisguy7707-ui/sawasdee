# 원어민 검수 의뢰 방법

앱의 모든 태국어(단어 525, 문장 387, 보조 단어 63, 글자·모음·받침·성조 규칙·표지판·숫자, 표 1,110줄)를 태국어 원어민(또는 태국어 교사)이 검토하도록 맡기는 방법이다.

## 보낼 파일
1. `docs/review/sawasdee-review.csv` — 검수 표(엑셀·구글 시트에서 열림). 한글이 깨지면 UTF-8로 열기.
2. `docs/review/sawasdee-review-audio.zip` — 음성 샘플 1,286개(약 8.5MB)(번호_태국어.mp3). 선택 사항: 발음을 같이 확인할 때.
   두 파일은 `python3 tools/export_review_sheet.py`로 다시 만들 수 있다(내용을 고친 뒤 최신본 전달).

## 분량이 많을 때의 우선순위 (제안)
표가 1,100줄이라 한 번에 맡기기 부담스러우면 이 순서로 나눠 맡긴다.
1. **"작성자가 특히 확신 낮음 = 예" 82줄** (외래어·합성어·성조를 분석기가 읽지 못한 단어 등. 가장 틀릴 가능성이 큼)
2. **단원 1~10**(기초·초급): 학습자가 가장 먼저, 가장 많이 보는 내용
3. 나머지 단원 11~35, 그리고 글자·성조 규칙 줄
4. (선택) 음성 확인
대화 연습 17쌍(`DIALOGUES`)은 표에 있는 기존 문장을 짝지은 것이다. 검수자에게 "표의 어느 문장 다음에 어느 문장이 자연스러운 대답인지"를 따로 확인받으려면 `src/data.rs`의 `DIALOGUES`를 같이 보여 준다.

## 검수자가 확인할 것 (표의 오른쪽 세 칸을 채움)
- **태국어가 자연스럽고 맞는가** (특히 "작성자가 특히 확신 낮음 = 예" 표시된 행, 82개)
- **성조**: 단어마다 적힌 성조(화살표)가 맞는가
- **한글 발음 표기**: 한국어 화자가 따라 읽기에 무리 없는 근사치인가(한글로는 성조·장단음이 안 드러나는 한계는 감안)
- **한국어 뜻**이 맞는가
- (선택) 음성이 자연스러운가, 성조가 틀린 클립이 있는가
- 칸 채우는 법: "맞음?" 칸에 O 또는 X, X이면 "수정 제안"에 올바른 표현, 필요하면 "메모"

## 의뢰 문구 예시 (한국어)
> 안녕하세요. 한국인이 태국어를 배우는 앱을 만들고 있습니다. 앱에 들어갈 태국어 단어·문장 약 1,100줄을 태국어 원어민이 확인해 주실 분을 찾고 있습니다. 첨부한 표(엑셀)의 각 줄에서 ① 태국어가 자연스럽고 맞는지 ② 성조 표시가 맞는지 ③ 한글 발음 표기가 적절한지 ④ 한국어 뜻이 맞는지를 확인하고, 틀린 곳은 올바른 표현을 적어 주시면 됩니다. 필요하면 음성 샘플(mp3)도 함께 들어 보시면 됩니다. 예상 작업 시간은 [   ]시간이고 사례는 [   ]입니다.

## Request text (English, for a Thai speaker)
> Hello! I'm building a Thai-learning app for Korean speakers. I need a native Thai speaker to review about 1,100 rows of Thai vocabulary, sentences and letter data in the attached spreadsheet. For each row please check: (1) is the Thai correct and natural, (2) are the tone marks right, (3) is the Hangul pronunciation a reasonable approximation for Korean learners, and (4) is the Korean meaning right. Mark O or X in the "Correct?" column and write the right version in "Correction". The rows marked "Author unsure" matter most. The audio samples (AI-generated voice) are optional to check. Estimated time: [   ] hours. Payment: [   ].

## 검수자를 찾는 곳 (직접 알아봐야 함)
- 프리랜서·과외 중개 서비스(언어 교환·튜터링 플랫폼, 재능 마켓)
- 한국에 있는 태국 유학생·이주민 커뮤니티, 대학 태국어과 교수·강사
- 태국 현지 어학원 또는 번역가
검수 경험이 있는 사람이 한 명, 가능하면 다른 한 명이 교차 확인하면 좋다. 사례 금액과 소요 시간은 시장 가격을 알아보고 정한다(이 문서는 금액을 정하지 않는다).

## 받은 뒤
1. 수정 제안이 든 CSV를 이 저장소에 `docs/review/returned/`로 두고 알려 주면, 해당 행을 `src/data.rs`에 반영하고 바뀐 문구의 음성을 다시 만든다(`tools/make_audio_google.py`).
2. 반영한 뒤에는 `./target/debug/sawasdee --selftest`로 검사하고, 검수 일자·검수자를 `docs/REVIEW_NOTES.md`에 적는다.
3. 앱 소개·스토어 설명에는 검수를 마친 범위만 "원어민 감수"라고 쓴다.
