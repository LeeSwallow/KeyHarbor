# 5단계 관리 앱(Tauri) 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans. Steps use `- [ ]`.

**Goal:** docs/design/09의 관리 앱을 Tauri v2로 만든다: 금고·세션·정책·사용 기록·설정 화면, 보안 입력 창(`vault_request` 응답), 메뉴 막대 아이콘, 인증 trait(`NoneAuth`만), 값 보기·복사(30초 클립보드 지우기).

**Architecture:** `rebuild/crates/app`(Tauri v2, `tauri` crate)이 `keyharbor-core`를 직접 링크한다. Keychain 접근은 같은 프로세스에서 하되, ADR/02 §2.10의 "단일 바이너리" 원칙 대신 **앱 바이너리도 Keychain ACL에 한 번 등록되는 것**을 받아들인다(J-22 수정 — npm·sidecar 빌드 복잡도를 줄이기 위해. 결정 기록에 남김). 프런트엔드는 **npm 없이** `crates/app/ui/`의 정적 HTML·CSS·ES 모듈 JS(`window.__TAURI__` 전역, `withGlobalTauri: true`). Lucide 아이콘은 필요한 SVG만 `ui/icons/`에 복사(ISC 라이선스 고지 포함).

## Global Constraints
- 1·3·4단계 제약 유지. npm/npx 금지. `cargo install tauri-cli --locked --version ^2` 허용.
- 평문은 "값 보기"·"복사" 동작 때만 Tauri 명령 `reveal_field(item, field)`로 받아 화면에 표시하고, 가리면 프런트엔드 상태에서 지운다. 다른 명령의 응답에 값 없음.
- 인증: `trait Authenticator { fn id(&self)->&'static str; fn challenge(&self, scope: AuthScope)->Result<()> }`, `enum AuthScope { OpenApp, Reveal, Edit, Export, PolicyChange }`, `NoneAuth`만 구현. 설정 화면은 09 §9.4.7 문구 그대로, 다른 방식은 "준비 중"으로 비활성.
- 클립보드: `NSPasteboard`에 `org.nspasteboard.ConcealedType` 표시, 30초 뒤 내용이 같으면 지움(objc2 또는 `arboard` + concealed type; 구현자가 가장 단순한 방법 선택).
- 메뉴 막대: Tauri tray, 아이콘 `docs/design/assets/menubar-icon.svg`를 템플릿 PNG로 변환해 사용, 메뉴 09 §9.4.6.
- 색·토큰: 09 §9.6.4·§9.7. 다크 모드 대응.

## Review Focus
1. 값 보기 후 다른 항목으로 이동하면 이전 값이 DOM에 남지 않는다.
2. 보안 입력 창에서 거절하면 `vault_wait`가 `declined`를 받는다.
3. 금고 키 없음 상태에서 앱이 새 키를 만들지 않고 안내 화면을 보인다(09 §9.10).
4. 변조된 항목은 "변조 의심"으로 표시되고 입력에 쓰이지 않는다.
5. 앱이 네트워크 listener를 열지 않는다(`lsof -iTCP -sTCP:LISTEN -p <pid>`로 확인).

## Tasks
1. 앱 골격: `crates/app`(tauri.conf.json: identifier `io.github.leeswallow.keyharbor.app`, productName KeyHarbor, frontendDist `ui`, withGlobalTauri), 아이콘(`cargo tauri icon docs/design/assets/app-icon.svg` — SVG 미지원이면 qlmanage로 1024 PNG 변환 후), `cargo tauri build --debug` 성공 → 커밋 `feat(app): Tauri shell for the admin app`
2. Tauri 명령 계층(`commands.rs`, 단위 테스트 가능하게 core 호출만): list_items, item_detail(값 없음), reveal_field, add_item, edit_item, remove_item, generate_password, list_sessions, open/stop/recover session, list_pending(인계·요청·draft), resume_handoff, fulfill_request/decline_request, confirm/discard draft, policy CRUD, audit list, settings get/set, export/import. 테스트(`cargo test -p keyharbor-app`) → 커밋 `feat(app): command layer over keyharbor-core`
3. UI: 09 §9.3 사이드바, §9.4 각 화면, 명령 팔레트(⌘K), 단축키 표(§9.8). 정적 JS. 실행 확인: `cargo tauri dev` 대신 debug 빌드 실행 후 `screencapture -l <windowid>`가 권한상 불가하면 headless Chrome으로 `ui/index.html`을 모의 `__TAURI__`(테스트용 `ui/dev-mock.js`, 릴리스 번들 제외)와 함께 렌더링해 스크린샷 → 커밋 `feat(app): vault, sessions, policy, audit and settings screens`
4. 보안 입력 창·메뉴 막대·알림·클립보드 → 커밋 `feat(app): secure input window, menu bar and concealed clipboard`
5. Review Focus 테스트(명령 계층 + 모의 UI) → 커밋 `test(app): reveal hygiene, decline path, missing key, tamper, no listener`
6. CI에 앱 빌드 job(macOS, `cargo tauri build --debug --no-bundle`), GoReleaser/릴리스에 `.dmg` 추가는 문서에만(릴리스는 사용자 결정), README·09·05·ROADMAP·.sticky → 커밋 `docs: phase 5 admin app`
