# Changelog

중요한 변경을 모두 적는다. 형식은 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)를 따르고, GitHub 릴리스 노트는 PR 라벨로 만들어진다.

## [Unreleased]

### Added

- 사람용 자동 완성과 저장(확장 팝업): 저장된 로그인을 그 사이트의 페이지에 채울 때마다 Touch ID로 확인하고, 페이지에서 친 로그인을 draft로 저장한다. 셸에서 native host를 직접 띄우면 값을 주지 않는다.
- `keyharbor receive`와 MCP `session_wait_site`: 사람이 사이트를 넘길 때까지 기다린다.

## [0.1.1] - 2026-10-30

### Added

- 관리 앱: 항상 위에 뜨고 화면 캡처에서 빠지는 보안 입력 창, 항목 검색·필터(⌘F), 이름 바꾸기·옮기기, 삭제 실행 취소(10초), 명령 팔레트(⌘K), macOS 알림(종류마다 끌 수 있음), 클립보드 지우는 시간 설정.
- `keyharbor vault import-chrome`: Chrome 비밀번호 내보내기(CSV)를 사이트에 묶인 login 항목으로 가져온다.
- `keyharbor vault mv`: 항목 이름 바꾸기·옮기기(그 항목을 가리키는 정책 규칙이 있으면 거절).

### Changed

- 관리 앱의 모든 화면을 디자인 시스템 부품으로 다시 그렸다: 저장 값 대신 사람의 말, 버튼 변형, 탭·제목·팔레트의 아이콘, 스위치와 선택지로 된 설정.
- 항목 삭제는 확인 창 대신 10초 동안 실행 취소할 수 있다.

### Fixed

- `keyharbor --help` 첫 줄이 브라우저 세션만 말하던 것.
- 확인 대화상자가 창 전체를 덮지 않고 칸 하나에 끼어 그려지던 것.
- 홈의 확인 대기 수가 옆 메뉴 배지와 다르던 것.

## [0.1.0] - 2026-10-30

첫 릴리스. macOS 전용이고, 서명·공증하지 않았다.

### Added

- 브라우저 세션(1단계): `create`, `list`, `open`, `stop`, `wait-auth`, `handoff`, `resume`, `recover`, `endpoint`. 세션마다 감독 프로세스가 OS 잠금을 쥐고 Chrome을 `--remote-debugging-pipe`로 실행해 CDP를 소유한다.
- `keyharbor mcp`: 세션 도구, 브라우저 조작 도구 11종(`browser_navigate`·`snapshot`·`screenshot`·`click`·`fill`·`type`·`press`·`tabs`·`new_tab`·`switch_tab`·`wait`), 값 없는 `vault_list`. `--session` 범위 지정, `--allow-cdp` 선택.
- 로컬 암호화 금고와 `keyharbor vault` CLI: `init`, `add`, `edit`, `rm`, `list`, `import-env`, `export`·`import`(age 형식), `rotate-root`, `rotate-domain`.
- 자격 증명·결제·개인정보 세 도메인과 항목 종류 6가지(`login`, `secret`, `env`, `card`, `identity`, `address`).
- XChaCha20-Poly1305 필드 암호화, 도메인 키 envelope, HKDF 키 분리, 항목 메타데이터 HMAC, macOS Keychain 루트 키.
- 중단에 안전한 키 교체와 동시 쓰기 보호.
- 에이전트가 값을 보지 않고 쓰는 길: `{{secret:항목.필드}}` 치환, 응답 마스킹, 권한 정책, 사용 기록, origin 바인딩.
- 사이트별 로그인 상태와 금고 등록 경로: `vault_generate`, `vault_request`·`wait`·`fulfill`, 위임 저장, 인계 중 로그인 포착(기본 꺼짐), draft 확정·폐기·정리.
- grant, 평소 Chrome의 사이트 로그인을 세션으로 넘기는 브라우저 확장과 native host(`keyharbor browser install-host`).
- 개발자 비밀: `keyharbor run`(`--env`, `--env-file`, `--file`, `--template`, 인자 치환, 신호 전달, 출력 마스킹), 경로 이름과 태그, 이전 값과 `vault revert`, 비밀 아닌 필드(`--plain`).
- Claude Code·Codex 플러그인: MCP 서버 등록, 스킬 6개, rules, commands, 훅, `keyharbor status`.
- 관리 앱(`KeyHarbor.app`, DMG): 홈·확인 대기·세션·항목·사용 기록·정책·설정 화면, 보안 입력 폼, 인계 재개, 항목 추가·편집·삭제, 값 보기와 가린 클립보드 복사(설정한 시간 뒤 지움), 세션 열기·중지·복구, 메뉴 막대, macOS 알림, 명령 팔레트(⌘K). CLI는 앱 안 `Contents/Helpers`에 들어 있다.

### Security

- CLI는 평문 비밀을 출력하지 않으며, 비밀 값을 명령행 인자로 받지 않는다.
- 보호를 약하게 하는 변경(정책 allow, 기록 끄기, 포착, origin 추가, 클립보드 시간 늘리기 등)은 사람이 확인한다.
- 관리 앱은 네트워크 listener를 열지 않고, 창의 capability는 `core:default`뿐이다.

[Unreleased]: https://github.com/LeeSwallow/KeyHarbor/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/LeeSwallow/KeyHarbor/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/LeeSwallow/KeyHarbor/releases/tag/v0.1.0

