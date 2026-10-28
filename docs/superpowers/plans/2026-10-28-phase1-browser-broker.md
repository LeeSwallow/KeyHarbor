# 1단계 브라우저 브로커 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (or subagent-driven-development) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `rebuild/` 워크스페이스에 세션 수명 관리(프로토타입 이식)와 **CDP를 소유하는 감독 프로세스**, 브라우저 조작 기능, 그리고 이를 제공하는 CLI·MCP 서버를 만든다.

**Architecture:** 새 crate `keyharbor-broker`가 감독 프로세스를 맡는다. 감독 프로세스는 Chrome을 전용 프로필과 `--remote-debugging-pipe`로 띄우고, CDP 파이프를 직접 쥐고, 세션별 Unix socket으로 명령을 받는다. 새 crate `keyharbor-mcp`는 rmcp stdio 서버로 세션 도구와 `browser_*` 도구를 제공하며, 브라우저 명령은 socket을 거쳐 감독자에게 보낸다. 세션 상태는 기존 `keyharbor-sqlite`에 generation CAS로 기록한다. 기준 설계: docs/design/02 §2.3–§2.6·§2.9, ADR-0004.

**Tech Stack:** Rust 2024, tokio(rt-multi-thread, process, net, io-util, time, sync, macros), serde/serde_json, rmcp 3.8(server, macros, transport-io), base64(스크린샷), 기존 workspace crate.

**참고 코드(이식 원본):** 저장소 루트 프로토타입 `src/lib.rs`(Store: sessions/handoffs/locks/recover), `src/main.rs`(supervise, open/stop/wait-auth/handoff/resume/recover/endpoint), `src/mcp.rs`(rmcp 도구 정의), `tests/session_lifecycle.rs`·`tests/mcp_stdio.rs`·`tests/mcp_scope.rs`(가짜 브라우저 스크립트 방식).

## Global Constraints

- PATH: `export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$HOME/.cargo/bin:$PATH"`. 작업 디렉터리 `rebuild/`. 허용 명령: cargo build/test/fmt/clippy/run/check/tree/deny. npm 금지. 실제 키체인 쓰기 금지(테스트는 `KEYHARBOR_TEST_KEYSTORE_DIR`).
- 데이터 폴더 구조(`<data>` = `--data-dir`/`KEYHARBOR_HOME`/`~/Library/Application Support/KeyHarbor`): `profiles/<session-id>/`(Chrome user-data-dir, 0700), `run/<session-id>.lock`(감독자 OS 잠금), `run/<session-id>.sock`(제어 socket, 0600), `run/mcp-<session-id>.lock`(범위 지정 MCP 서버 잠금), `run/shots/`(스크린샷).
- 상태 전이는 `keyharbor_domain::State::permits`를 그대로 따른다. `recovery_required`는 저장하지 않는 표시값이다: 활성 상태인데 `run/<id>.lock`을 아무도 잡고 있지 않으면 그렇게 표시한다(02 §2.6).
- 감독자는 Chrome을 `--user-data-dir=<profile> --remote-debugging-pipe --no-first-run --no-default-browser-check [--headless=new] [<url>]`로 실행한다. `cdp: true` 세션만 `--remote-debugging-port=0`을 추가하고 `endpoint`를 노출한다.
- CDP 파이프 프로토콜: 자식의 fd 3이 KeyHarbor→Chrome, fd 4가 Chrome→KeyHarbor. 각 메시지는 JSON + `\0`. 페이지 조작은 `Target.attachToTarget {flatten:true}`의 `sessionId`를 붙인다.
- 제어 socket 프로토콜: 한 줄 JSON 요청 `{"op":"...", ...}` → 한 줄 JSON 응답 `{"ok":true,"result":...}` 또는 `{"ok":false,"error":"..."}`. 감독자는 요청을 **한 번에 하나씩** 실행한다(02 §2.6.1).
- 1단계에서 `fill`·`type`은 받은 텍스트를 그대로 입력한다. placeholder 치환과 응답 마스킹은 3단계의 일이다. 그때 끼워 넣을 수 있게, 입력 텍스트와 응답이 지나가는 지점을 `ops.rs` 한 곳으로 모은다.
- 오류 메시지에 입력 텍스트를 넣지 않는다(3단계에서 비밀이 지나가게 된다).
- 기존 테스트 66개와 `scripts/test-scripts.sh` 등은 계속 통과해야 한다.
- 실제 Chrome 테스트: `KEYHARBOR_TEST_CHROME`(실행 파일 경로)이 있을 때만 실행하고, 없으면 테스트 안에서 바로 return(통과로 처리하되 `eprintln!("skipped: …")`). 로컬 확인용 경로는 `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`. 테스트는 `--headless=new`로 띄운다(환경 변수 `KEYHARBOR_BROWSER_HEADLESS=1`이면 감독자가 이 플래그를 붙인다).

## Review Focus

1. **감독자가 죽은 뒤의 세션**: 목록에 `recovery_required`로 보이고, `open`은 거부되고, `recover --confirm-browser-closed`가 있어야만 `stopped`로 돌아가야 한다. → Task 2 테스트.
2. **같은 세션을 두 번 `open`**: 둘째는 즉시 거부되고 첫째 브라우저는 영향받지 않아야 한다. → Task 2.
3. **브라우저 창을 사용자가 직접 닫음**: 감독자가 `stopped`를 기록하고 socket과 잠금을 정리해야 한다. → Task 3(가짜 브라우저 종료로 재현).
4. **존재하지 않는 ref/selector 클릭, 탐색 실패**: 감독자는 살아 있고 오류 응답만 돌려줘야 한다. → Task 4.
5. **범위 지정 MCP 서버(`--session`)**: 다른 세션 조작과 새 세션 생성이 거부돼야 한다. → Task 6.

---

## Task 1: 세션·인계 저장소 이식 (`keyharbor-sqlite`, `keyharbor-domain`)

**Files:** Modify `crates/sqlite/src/lib.rs`(+ 새 `crates/sqlite/src/session_repo.rs` 권장), `crates/domain/src/lib.rs`.

**Produces:**
- `SessionRow { id: SessionId, name: SessionName, state: State, generation: Option<Generation>, cdp: bool }`
- `SqliteStore::{create(name)→Session, list()→Vec<Session>, get(id)→Option<Session>, delete(id)→bool (stopped만), reserve(id, cdp: bool)→Option<Generation>, transition(id, gen, from, to)→bool, generation(id)→Option<Generation>, cdp_enabled(id)→bool}` (create/list/reserve/transition은 기존 것을 유지·확장)
- 인계: `request_handoff(id, gen, origin: Option<&str>)→RequestId`(running→awaiting_user, 이전 대기 요청은 `superseded`), `pending_handoff(id)→Option<(RequestId, Option<String>)>`, `resume_handoff(id, gen, request_id)→Result<()>`(awaiting_user→running, request_id·generation 일치 필수), `handoff_status(id, request_id)→Option<HandoffStatus>` where `enum HandoffStatus { Pending, Resumed, Superseded, Cancelled }`
- `force_stopped(id)→bool` (recover용: 활성 상태 → stopped, generation 제거)
- `RequestId`(UUID) 타입은 domain에.

- [ ] 실패 테스트: 프로토타입 `tests/session_lifecycle.rs`와 `src/lib.rs` 테스트 중 저장소 수준 시나리오(중복 이름, reserve 경쟁 8스레드, 오래된 generation 거부, 인계 요청·대체·재개, 잘못된 request_id 거부)를 단위 테스트로 옮긴다.
- [ ] 구현, `cargo test -p keyharbor-sqlite` 통과, 커밋 `feat(sqlite): sessions with handoff requests and recovery`

## Task 2: 감독자와 세션 CLI (`keyharbor-broker`, CLI)

**Files:** Create `crates/broker/{Cargo.toml,src/lib.rs,src/supervisor.rs,src/paths.rs}`; Modify `crates/cli/src/main.rs`(+ `crates/cli/src/session_cmd.rs`), `crates/cli/tests/session_cli.rs`(신규).

**Produces:**
- `paths::{profile_dir(data,&SessionId), lock_path, socket_path, shots_dir}`
- `supervisor::run(data: &Path, id: SessionId, generation: Generation, browser: &Path, url: Option<&str>, cdp: bool) -> Result<()>`: 잠금 획득(실패 시 오류) → Chrome 실행(fd 3/4 파이프 연결) → `starting→running` 기록(실패 시 자식 종료) → socket 서버 → 자식 종료 대기 → `→stopped` 기록 → socket·잠금 정리.
- `supervisor::is_alive(data, &SessionId) -> bool` (잠금 파일 try_lock 실패 = 살아 있음)
- CLI 최상위 명령(프로토타입과 같은 이름): `create <name>`, `list`(이름·상태·표시값 recovery_required·cdp), `open <id> [--url U] [--cdp] [--browser PATH]`, `stop <id>`, `wait-auth <id> [--origin O]`, `handoff <id>`, `resume <id> <request-id>`, `recover <id> --confirm-browser-closed`, `endpoint <id>`, 숨은 `__supervise …`. `open`은 감독자를 분리된 자식 프로세스(`keyharbor __supervise`)로 띄우고 `running`이 될 때까지(최대 15초) 기다린다. 기본 브라우저는 `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`.
- `stop`은 socket에 `{"op":"close"}`를 보내 감독자가 `Browser.close`로 끝내게 한다.

- [ ] 실패 테스트(`session_cli.rs`, 프로토타입처럼 가짜 브라우저 셸 스크립트 사용: fd 3을 읽고 버리며 `sleep`하는 스크립트): create→open→list running→stop→list stopped; 이중 open 거부(Review Focus 2); 감독자 kill -9 → list에 recovery_required, open 거부, `recover` 없이 resume 거부, `recover --confirm-browser-closed` 후 stopped(Review Focus 1); 가짜 브라우저가 스스로 종료 → stopped(Review Focus 3); wait-auth/handoff/resume 흐름과 잘못된 request id 거부.
- [ ] 구현, 테스트 통과, 커밋 `feat(broker): supervisor with OS lock and session lifecycle commands`

## Task 3: CDP 파이프 클라이언트 (`keyharbor-broker::cdp`)

**Produces:** `struct Cdp`: `async fn call(&self, method: &str, params: Value, session: Option<&str>) -> Result<Value>`, `fn events(&self) -> broadcast::Receiver<Value>`; 생성은 감독자가 자식의 fd 3/4로. 요청 id는 증가 정수, 응답은 oneshot으로 매칭, 30초 타임아웃.

- [ ] 실패 테스트: 메모리 내 가짜 파이프(tokio duplex)로 요청 직렬화(`...}\0`), 응답 매칭, 순서가 섞인 응답, 이벤트 브로드캐스트, 타임아웃.
- [ ] 구현·통과·커밋 `feat(broker): CDP client over the remote-debugging pipe`

## Task 4: 브라우저 조작과 제어 socket (`keyharbor-broker::{ops, control}`)

**Produces (control op → 결과):**
| op | 인자 | 결과 |
|---|---|---|
| `navigate` | url | `{url, title}` (Page.navigate 후 load 대기, 15초) |
| `snapshot` | — | `{url, title, text}`: Accessibility.getFullAXTree를 들여쓴 줄로: `[ref] role "name" value?`(ref = backendDOMNodeId, 무시 노드 제외, 최대 2000줄) |
| `screenshot` | — | `{path}`: `run/shots/<id>-<n>.png` |
| `click` | target | `{}` — target은 `{"ref":N}` 또는 `{"selector":"css"}`; DOM.getBoxModel 중심점에 Input.dispatchMouseEvent pressed/released |
| `fill` | target, text | `{}` — DOM.focus 후 값 비우기(Runtime callFunctionOn `this.value=''`) → Input.insertText |
| `type` | text | `{}` — 포커스된 요소에 Input.insertText |
| `press` | key | `{}` — Enter/Tab/Escape/Backspace/ArrowUp/Down/Left/Right 등 Input.dispatchKeyEvent |
| `tabs` | — | `[{id, url, title, active}]` |
| `new_tab` | url? | `{id}` (Target.createTarget, 활성 탭 전환) |
| `switch_tab` | id | `{}` |
| `wait` | ms? / selector? | `{}` (selector면 100ms 간격 폴링, 최대 15초) |
| `close` | — | `{}` (Browser.close) |
| `status` | — | `{state, generation}` |

- `ops.rs`가 모든 op의 입력 텍스트와 응답 JSON이 지나가는 유일한 지점이다(3단계 치환·마스킹 자리: `fn prepare_text(&self, text:&str)->Result<Zeroizing<String>>`, `fn redact(&self, v: Value)->Value` — 1단계에서는 그대로 통과).
- 클라이언트: `broker::client::call(data, &SessionId, Request) -> Result<Value>` (socket 연결·한 줄 쓰기·한 줄 읽기, 감독자 없음이면 `"supervisor is not running"`).
- [ ] 실패 테스트(실제 Chrome, `KEYHARBOR_TEST_CHROME` 없으면 skip): 테스트가 `std::net::TcpListener`로 로컬 HTTP 서버를 띄워 로그인 폼 페이지(`<input id=user> <input id=pw type=password> <button id=go>`)와 결과 페이지를 제공 → open → navigate → snapshot에 `textbox`와 `button "로그인"` → fill/click로 폼 제출 → 결과 페이지 제목 확인 → tabs·new_tab·switch_tab → screenshot 파일 존재 → 없는 ref 클릭은 오류이고 이어서 `status`가 정상(Review Focus 4) → stop.
- [ ] 구현·통과(로컬에서 KEYHARBOR_TEST_CHROME 지정해 실제로 실행)·커밋 `feat(broker): browser operations over the control socket`

## Task 5: raw CDP 선택 기능

- `open --cdp`: Chrome에 `--remote-debugging-port=0` 추가, 프로필의 `DevToolsActivePort`에서 주소를 읽어 `endpoint`가 `ws://127.0.0.1:<port><path>`를 출력. cdp가 아닌 세션의 `endpoint`는 거부.
- [ ] 테스트(실제 Chrome): cdp 세션 endpoint가 ws://로 시작, 일반 세션은 거부. 커밋 `feat(broker): optional raw CDP endpoint`

## Task 6: MCP 서버 (`keyharbor-mcp`, `keyharbor mcp`)

**Files:** Create `crates/mcp/{Cargo.toml,src/lib.rs}`; Modify CLI(`mcp` 하위 명령); Test `crates/cli/tests/mcp_stdio.rs`.

- `keyharbor mcp [--session ID] [--allow-cdp] [--agent NAME] [--browser PATH]`, rmcp stdio.
- 도구: `session_list`, `session_create`, `session_open {id, url?, cdp?}`, `session_stop`, `session_wait_auth {id, origin?}`, `session_wait_for_resume {id, request_id, timeout_seconds 1..=300, 기본 120}`, `session_endpoint`(allow-cdp일 때만 등록·허용), `browser_navigate|snapshot|screenshot|click|fill|type|press|tabs|new_tab|switch_tab|wait {session, …}`, `vault_list`(이름·종류·도메인·필드 이름·origin만).
- `--session`이면 그 세션만 보이고 다른 세션·생성 거부, `run/mcp-<id>.lock`으로 같은 세션의 범위 지정 서버는 하나만(Review Focus 5). `--agent`는 저장해 두기만 한다(3단계 정책에서 사용).
- [ ] 실패 테스트(프로토타입 `tests/mcp_stdio.rs`·`mcp_scope.rs` 이식, JSON-RPC로 initialize→tools/list→tools/call): 도구 목록, 가짜 브라우저로 create/open/stop/wait_auth/wait_for_resume(CLI resume으로 깨움), 범위 거부, vault_list에 값 없음. 실제 Chrome 있으면 browser_navigate·snapshot 왕복 한 번.
- [ ] 구현·통과·커밋 `feat(mcp): stdio MCP server with session and browser tools`

## Task 7: 문서·CI

- `rebuild/README.md`에 세션·브라우저·MCP 사용법(MCP 등록 예: `{"command":"/abs/path/keyharbor","args":["mcp","--agent","claude"]}`), 한계(1단계는 입력 텍스트를 그대로 보냄 — 비밀번호 입력은 3단계부터).
- `docs/design/05` 1단계에 완료 표시·테스트 이름, `ROADMAP.md`·`.sticky/roadmap.md` 갱신.
- `.github/workflows/ci.yml` test job에 `KEYHARBOR_TEST_CHROME` 설정: macOS `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`, Ubuntu `google-chrome`(`which google-chrome` 결과), 그리고 `KEYHARBOR_BROWSER_HEADLESS=1`.
- [ ] `cargo fmt --check`, clippy `-D warnings`, `cargo test --workspace`(로컬은 KEYHARBOR_TEST_CHROME 지정), `sh scripts/check-docs.sh`, `sh scripts/test-scripts.sh` 통과. 커밋 `docs: phase 1 browser broker usage and status`
