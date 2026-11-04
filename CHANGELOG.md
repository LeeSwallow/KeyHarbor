# Changelog

중요한 변경을 모두 적는다. 형식은 [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)를 따르고, GitHub 릴리스 노트는 PR 라벨로 만들어진다.

## [Unreleased]

## [0.3.0] - 2026-11-04

### Added

- `keyharbor run`의 통로 ④ 실행 중 로컬 조회: 자식에게만 알려 주는 일회용 Unix socket과 토큰(`KEYHARBOR_LOOKUP_SOCKET`·`KEYHARBOR_LOOKUP_TOKEN`). 범위는 `run`에 준 항목이고 `--allow <패턴>`으로 넓힌다. 요청마다 같은 guard 검사와 사용 기록을 거친다. Python keyring 백엔드는 `integrations/python-keyring/`(배포하지 않음). (#146)
- 관리 앱: 앱 열기·값 보기·편집·정책 변경 전에 인증 포트(`Authenticator`)를 거친다. 지금 방식은 "인증 없음" 하나이고 설정 화면의 표시가 이 포트에서 온다. (#151)
- 관리 앱: 정책 규칙에 `browser` 자원을 고를 수 있다. 사용 기록에 "세션 지움"·"파일 올림" 이름이 생겼다. (#173)

### Changed

- 반복 거부(초당 상한으로 묶인 거부)도 "거부됨" 알림이 간다. 설정 화면의 ego lite 확인은 설정 `ego_probe_seconds`만큼 기다린다. 세션의 실패 이유는 복구가 필요한 세션에만 보인다. (#181)
- 확장의 항목 이름 미리 검사가 CLI와 같은 규칙을 쓴다(`contracts/item-names.json`). 빈 경로 조각(`a//b`)을 거부하고 결합 문자를 받는다. (#168)
- 내부 구조를 계층·책임별 모듈로 나눴다(domain, core, guard, broker, supervisor, mcp, cli, design-check, 관리 앱, 확장). 명령·도구 이름, 출력, 저장 형식은 그대로다. (#159–#186)

### Fixed

- 세션 시작 중 설정·저장소 읽기가 실패하면 `starting`에 남지 않고 `failed`와 사유가 남는다. 로그인 포착 실패도 값 없이 사용 기록에 남는다. (#176)
- 관리 앱: `HOME`이 없으면 상대 경로에 금고를 만들지 않고 오류로 끝난다. 시작할 때 인증이 거절되면 끝나지 않고 메뉴 막대에 남는다. (#173)
- 확장 팝업: 넘기기 요청이 실패해도 버튼이 막히지 않고, 화면이 loading에 멈추지 않는다. 백그라운드 응답을 검사한다. (#158)
- 응답 마스커를 만들지 못하면 panic으로 세션을 망가뜨리지 않고 응답을 보내지 않는다. (#165)
- Ctrl-C에 `keyharbor run`이 바로 죽지 않고 자식이 끝난 뒤 임시 파일과 socket을 지운다. (#146)

### Security

- `browser_screenshot`이 찍는 동안 KeyHarbor가 값을 넣은 입력칸을 원(●)으로 그린다. 가리지 못하면 찍지 않는다. (#144)
- 확장 자동 완성 답의 비밀번호가 지워지지 않는 사본으로 남지 않는다. 제어 요청의 Debug 출력에 localStorage 값이 나오지 않는다. (#184)
- age 가져오기·내보내기에서 도중에 끝나도 평문 필드가 메모리에서 지워진다. (#174)
- 관리 앱 항목 편집이 비밀 값을 지워지지 않는 문자열로 복제하지 않는다. (#173)
- 개발 도구 `vitest`·`web-ext`를 critical 권고 이후 버전으로 올렸다(배포 번들에는 들어가지 않음). (#187)

## [0.2.0] - 2026-11-02

### Added

- ego lite 브라우저: `keyharbor open --browser auto|chrome|ego-lite|<경로>`(MCP `keyharbor mcp --browser`도)와 설정 `browser`. 자동이면 ego lite가 답할 때 ego lite, 아니면 Chrome으로 연다. ego lite 세션은 다른 프로그램도 같은 창을 조작할 수 있어 "보호 낮음"으로 표시한다(ADR-0018). `--headless`, 설정 `ego_probe_seconds`·`ego_poll_ms`.
- `keyharbor browser install-host --browser ego-lite`: ego lite에서도 확장이 KeyHarbor에 연결된다.
- `keyharbor rm <세션>… [--force]`(`ls` 별칭)와 세션이 시작하지 못한 이유 표시.
- MCP `browser_select`(드롭다운 고르기)와 `browser_upload`(설정 `upload_dirs` 안의 파일만, `upload_max_files`개까지). 업로드는 사용 기록에 남는다.
- `keyharbor mcp --non-interactive`: 사람을 기다리는 대신 `needs_user`를 돌려준다.
- identity 항목에 지원서용 필드(영문·한자 이름, 성별, 국적, 비상 연락처, 병역·장애·보훈)와 직접 이름 붙인 필드.
- 관리 앱: 세션의 브라우저와 보호 표시, 세션 지우기(10초 실행 취소), [Chrome으로]·[ego lite로] 열기, 신원 입력 묶음, 설정의 "브라우저"·"파일 올리기".
- 브라우저 확장 스토어 패키지(`npm run package`)와 Chrome 웹 스토어·Firefox AMO 게시 workflow.
- 사람용 자동 완성과 저장(확장 팝업): 저장된 로그인을 그 사이트의 페이지에 채울 때마다 Touch ID로 확인하고, 페이지에서 친 로그인을 draft로 저장한다. 셸에서 native host를 직접 띄우면 값을 주지 않는다.
- `keyharbor receive`와 MCP `session_wait_site`: 사람이 사이트를 넘길 때까지 기다린다.
- 관리 앱의 세션 카드에 평소 Chrome의 탭을 끌어다 놓아 그 사이트의 로그인을 넘긴다. 넘기기 전에 브라우저 알림으로 한 번 더 묻는다(확장에 `notifications` 권한).

### Fixed

- Chrome을 `/Applications`에서만 찾던 것: `~/Applications`, Chromium, PATH의 `google-chrome`도 찾는다.

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

[Unreleased]: https://github.com/LeeSwallow/KeyHarbor/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/LeeSwallow/KeyHarbor/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/LeeSwallow/KeyHarbor/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/LeeSwallow/KeyHarbor/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/LeeSwallow/KeyHarbor/releases/tag/v0.1.0

