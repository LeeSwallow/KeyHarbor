# KeyHarbor: 재구현

저장소 루트의 Rust 워크스페이스(`Cargo.toml`, 이 `crates/`)는 프로토타입(`prototype/`)과 별도의 데이터 경로를 쓰는 재구현이다. 아직 재구현 중이다. 지금 동작하는 것은 브라우저 세션과 조작 도구(1단계), 로컬 금고(2단계), placeholder 치환 입력·응답 마스킹·정책·사용 기록·`keyharbor run`(3단계), 이를 제공하는 CLI와 MCP 서버다. 기존 데이터를 자동으로 가져오거나 변경하지 않는다. 설계는 [`../docs/design/`](../docs/design/README.md)에 있다.

```sh
cargo test --workspace
cargo run -p keyharbor -- create work
cargo run -p keyharbor -- list
```

기본 데이터 경로는 `~/Library/Application Support/KeyHarbor`이며 `--data-dir` 또는 `KEYHARBOR_HOME`으로 변경할 수 있다. 세션 이름과 금고 항목 이름·종류·사이트(origin) 같은 메타데이터는 SQLite에 평문으로 저장된다. 이름에 비밀을 넣지 않는다.

## crate 지도

헥사고날(Ports & Adapters) 구조다. 의존은 위에서 아래로만 가고, `sh ../scripts/check-layers.sh`가 이 규칙을 검사한다. 설계: [02 §2.4](../docs/design/02-system-architecture.md#24-c4-level-3--컴포넌트-crate-구조), [ADR-0014](../docs/design/adr/0014-hexagonal-guard-layer.md).

| 계층 | crate | 하는 일 | 주요 모듈 |
|---|---|---|---|
| interfaces | `keyharbor` (CLI) | clap 명령, 어댑터 조립(`compose` 한 곳: 저장소·키 저장소·시계·감독자), 파일 읽기·쓰기, `run`의 프로세스 실행 | `compose`, `session_cmd`, `grant_cmd`, `browser_cmd/{host, install}`, `vault_cmd/{input, items, output, files}`, `policy_cmd`, `audit_cmd`, `settings_cmd`, `site_cmd`, `run_cmd/{child, files, output, signals}`, `tty_confirm` |
| | `keyharbor-mcp` | stdio MCP 서버. 저장소·금고 opener·런타임은 생성자로 받는다 | `server`, `scope`, `tools/{session, browser, site, vault}` |
| adapters | `keyharbor-supervisor` | 세션 감독자 프로세스: 잠금, Chrome을 guard로 감싸 제어 socket 처리, 상태 기록(`core::session::supervision`), 제어 프로토콜 wire 타입과 client, `__supervise` argv(`SuperviseArgs`), `SessionRuntime`(`ProcessRuntime`) | `process/{mod, prepare}`, `control/{wire, server, client}`, `ops`, `argv`, `runtime`, `lock`, `paths` |
| | `keyharbor-broker` | Chrome 어댑터: Chrome 실행과 CDP pipe, `BrowserPort`·`PageInput`·`FrameInspector` 구현(`CdpBrowser`), 모든 페이지 감시(`pages/{capture, gate, cookies}`: 로그인 포착, grant 밖 문서 차단, 끝난 grant의 쿠키 삭제). domain에만 의존하고 supervisor만 이것에 의존한다 | `chrome`, `cdp/{client, protocol}`(crate 내부), `browser/{navigate, snapshot, input, tabs, screenshot, wait, frames}`(crate 내부) |
| | `keyharbor-sqlite` | Diesel 저장소, 저장소 포트와 `RotationLock`(`vault.lock`) 구현 | `connection`, `rotation_lock`, `contexts/{vault, session, site, access, audit, config}/{schema, models, mappers, repo}` |
| | `keyharbor-keystore` | `KeyStore` 어댑터: macOS login keychain(`KeychainKeyStore`, 다른 OS에서는 오류), 테스트용 파일 저장소(`FileKeyStore`) | `keychain`, `file` |
| inner | `keyharbor-vault` | 금고 레코드 형식·메타 MAC 입력·키 분리·키 ID, I/O 없음 | `record`, `meta`, `keys` |
| crypto | `keyharbor-crypto` | 모든 암호 원시 연산과 난수(id 포함), I/O 없음 ([ADR-0016](../docs/design/adr/0016-crypto-crate.md)) | `aead`, `envelope`, `kdf`, `mac`, `random`, `export` |
| security | `keyharbor-guard` | 평문의 판정과 집행: PDP·PEP·사용 기록·`GuardedBrowser`, 항목 add/edit 순서, 세션 작업 → 정책 Action 표 | `pdp/checks/<검사>`, `pep/{substitute, mask/variants/<변형>, origin, selector}`, `audit`, `run`, `items`, `session`, `capture`, `guard`, `browser` |
| application | `keyharbor-core` | 유스케이스(포트만 사용), `PlaintextAccess`, 약화 작업 판정(`weaken`) | `vault/{open, items, summary, resolve, developer}`, `session/{manage, lifecycle, handoff, supervision}`, `site`, `request`, `grant`, `policy`, `audit`, `settings`, `rotate`, `transfer`, `dotenv`, `weaken` |
| domain | `keyharbor-domain` | 엔티티·값 객체·규칙, 항목 종류 레지스트리 `KINDS`, 설정 레지스트리 `SETTINGS`, placeholder 문법, 요청자 이름 `AgentName`, 거부 사유 `DenialReason`, 포트(trait) | `vault`, `policy`, `audit`, `denial`, `secret`, `settings`, `site`, `ports/{vault, session, site, access, audit, keys, browser, clock, confirm, runtime}` |
| dev tool | `keyharbor-design-check` | `design-system/` 디자인 시스템 검사(계층·테마·대비·아이콘·클래스), 쇼케이스 서버 `design-serve`. 출시물에 들어가지 않고 아무도 의존하지 않는다([ADR-0015](../docs/design/adr/0015-design-system-layers.md)) | `scanner`, `model`, `layers`, `rules/*`, `color`, `theme`, `report`, `serve` |

- 평문 값은 `Vault::resolve`·`Vault::mask_values`에서만 나오고, 둘 다 guard가 검사 뒤 발급하는 `PlaintextAccess`를 요구한다. 브라우저 조작은 제어 socket → `ops` → `GuardedBrowser`로만 간다.
- **스키마 확장**: 저장소는 바운디드 컨텍스트별로 나뉘어 있다. 새 컨텍스트(예: 4단계 `vault_requests`)는 `crates/sqlite/src/contexts/<이름>/`(schema·models·mappers·repo) + `crates/sqlite/migrations/<이름>/<timestamp>_<이름>_<설명>/up.sql` + `contexts::CONTEXTS`에 한 줄이면 된다. 버전 타임스탬프는 모든 컨텍스트에서 유일해야 한다. 원시 SQL은 마이그레이션과 테스트에만 쓴다.
- **항목 종류 확장**: `keyharbor_domain::vault::KINDS`에 `KindSpec` 하나(필드마다 `secret`·`maskable`·`input`·기본 `description`, 종류마다 origin 바인딩·`run` 허용 기본값)와 `ItemKind` 변형 하나를 더한다. 스키마·기본값·CLI의 비밀 필드 거부·필드 이름 검사·마스킹이 이 표를 읽는다.
- **검사·변형 확장**: 사용 검사는 `crates/guard/src/pdp/checks/<검사>.rs` 하나 + `checks::BROWSER`/`PROCESS`에 한 줄, 거부 사유는 `DenialReason` 변형 하나, 마스킹 형태는 `crates/guard/src/pep/mask/variants/<변형>.rs` 하나 + `VARIANTS`에 한 줄, 약화 작업은 `core::weaken::WeakeningOp` 변형 하나다.
- 테스트 전용 기능: `keyharbor-keystore/insecure-test-keystore`, `keyharbor-sqlite/test-support`. 릴리스 빌드에 켜지지 않는지 `scripts/check-release-features.sh`가 확인한다.

## 브라우저 세션 (1단계)

세션마다 전용 Chrome 프로필(`profiles/<id>/`)을 쓴다. `open`은 세션을 예약하고 **감독 프로세스**를 띄운다. 감독자는 OS 잠금(`run/<id>.lock`)을 쥐고, Chrome을 `--remote-debugging-pipe`로 실행해 CDP를 직접 소유하며(포트를 열지 않는다), `run/<id>.sock`(0600)으로 명령을 받는다. 브라우저가 닫히면 `stopped`를 기록하고 socket을 지운다. 설계: [02 §2.3–§2.6](../docs/design/02-system-architecture.md#23-c4-level-2--컨테이너), [ADR-0004](../docs/design/adr/0004-supervisor-owns-cdp.md).

| 명령 | 설명 |
|---|---|
| `create <name> [--description TEXT]` · `list` | `list`는 `id 이름 상태[ cdp][ 설명]`. 상태가 활성인데 감독자가 없으면 `recovery_required` |
| `edit <id> --description TEXT \| --no-description` | 세션 설명(에이전트에게 보인다, 비밀 아님) |
| `open <id> [--url U] [--cdp] [--browser PATH]` | 기본 브라우저는 `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`. `--url`은 HTTPS(또는 localhost HTTP), 자격 증명·query·fragment 없이 |
| `stop <id>` | 감독자에게 `close`를 보내 `Browser.close`로 끝내고 `stopped`까지 기다린다(10초 안에 안 끝나면 강제 종료) |
| `wait-auth <id> [--origin O]` · `handoff <id>` · `resume <id> <request-id>` | 사용자에게 로그인을 넘기고, 같은 request id로만 재개. 새 요청은 이전 대기 요청을 대체한다 |
| `recover <id> --confirm-browser-closed` | 감독자가 죽은 세션(`recovery_required`·`failed`)을 `stopped`로. 감독자가 살아 있으면 거부 |
| `endpoint <id>` | `--cdp`로 연 세션의 raw CDP 주소(`ws://127.0.0.1:…`). **켜면 KeyHarbor의 보호가 우회된다** |
| `mcp [--session ID] [--allow-cdp] [--agent NAME] [--browser PATH]` | stdio MCP 서버. `--agent`가 없으면 에이전트 이름은 `mcp` |

`KEYHARBOR_BROWSER_HEADLESS=1`이면 감독자가 Chrome에 `--headless=new`를 붙인다(테스트·CI용). 이 변수는 composition root(`cli/src/compose.rs`)가 읽어 감독자에게 넘긴다.

### MCP 서버

에이전트 설정 예(Claude Code `.mcp.json` 형식):

```json
{"mcpServers": {"keyharbor": {"command": "/abs/path/keyharbor", "args": ["mcp", "--agent", "claude"]}}}
```

| 분류 | 도구 |
|---|---|
| 세션 | `session_list`(설명 포함), `session_create {name, description?}`, `session_open {id, url?, cdp?}`, `session_stop`, `session_wait_auth {id, origin?}`, `session_wait_for_resume {id, request_id, timeout_seconds 1–300}` |
| 세션 (선택) | `session_endpoint` — `--allow-cdp`일 때만 등록 |
| 브라우저 | `browser_navigate`, `browser_snapshot`, `browser_screenshot`, `browser_click`, `browser_fill`, `browser_type`, `browser_login {item, submit?}`, `browser_press`, `browser_tabs`, `browser_new_tab`, `browser_switch_tab`, `browser_wait` — 모두 `session` 인자 |
| 금고 | `vault_list` — 이름·종류·도메인·필드 이름·필드별 placeholder·설명(항목·필드)·origin·바인딩·`run_allowed`·`expires_at`·`expired`·상태·출처만. 메타 MAC이 맞지 않는 항목은 이름과 `tampered: true`만 |
| 금고 등록 | `vault_generate {name, origin, username?, password_rule? {length 8–128, symbols, forbid}}`, `vault_put {name, kind, fields, origins?}` — draft로만 저장한다. 이름이 이미 있으면 `<이름>~draft-N`이 되고 기존 항목은 바뀌지 않는다. 응답은 이름·`draft: true`·placeholder뿐이다 |
| 정보 요청 | `vault_request {kind, name, reason, origin?}` → `request_id`, `vault_wait {request_id, timeout_seconds 1–300}` → `fulfilled`(이름·placeholder)·`declined`·`expired`·`pending`(대기만 끝남, 요청은 남음). 사용자는 `keyharbor vault requests`·`fulfill <id> [--stdin \| --use ITEM]`·`decline <id>`로 답한다 |
| 사이트 상태 | `site_state_get {session, origin}` → `logged_in`·`expired`·`unknown`(기록 없음), `site_state_set {session, origin, state, item?}` — 에이전트가 페이지를 보고 남기는 메모다. KeyHarbor가 확인하지 않는다. `logged_in`이면 그 origin의 생성·포착 draft를 확정하고 `confirmed`에 이름을 돌려준다(에이전트가 put한 draft는 사람이 확정) |

`browser_login`은 페이지(최상위 또는 같은 origin frame)의 username·password 칸을 포커스와 상관없이 찾아 login 항목의 값을 `browser_fill`과 같은 검사로 넣고, `submit`(기본 참)이면 Enter를 누른다. 칸을 못 찾으면 아무것도 넣지 않고 `not found: password field`처럼 실패한다. browser-use와 함께 쓰는 방법은 [docs/guides/browser-use.md](../docs/guides/browser-use.md).

`browser_snapshot`은 접근성 트리를 `[ref] role "name"` 줄로 돌려준다. `browser_click`·`browser_fill`은 `ref` 또는 CSS `selector`를 받는다. `--session ID`로 띄운 서버는 그 세션만 보고 쓸 수 있으며 새 세션을 만들 수 없고, 같은 세션의 범위 지정 서버는 하나만 뜬다(`run/mcp-<id>.lock`). 재개(`resume`)와 복구(`recover`)는 사람이 CLI로 한다. MCP 도구에는 없다.

### 1단계의 한계

- 같은 세션을 여러 MCP 서버가 쓰면 명령은 감독자에서 하나씩 실행되지만 브라우저 상태(활성 탭 등)는 공유된다.
- 제어 socket 경로는 104바이트를 넘을 수 없다. 데이터 경로가 너무 길면 `open`이 오류를 낸다.
- 제어 socket은 같은 OS 사용자의 다른 프로세스를 막지 못한다. 보안 경계가 아니라 프로세스 간 통로다.
- macOS에서 사용자가 마지막 탭을 닫으면 감독자가 브라우저를 닫고 `stopped`를 기록한다(창 없는 Chrome이 남지 않게).

## 로컬 금고 (2단계)

네 도메인(자격 증명·결제·개인정보·개발자)의 항목을 암호화해 저장한다. 루트 키는 macOS login keychain(`io.github.leeswallow.keyharbor`)에, 나머지는 SQLite에 암호문으로 둔다. 방식은 [07 암호화 설계](../docs/design/07-cryptography-design.md)를 따른다.

| 명령 | 설명 |
|---|---|
| `vault init` | 금고와 루트 키를 만든다. 이미 있으면 덮어쓰지 않는다 |
| `vault add <name> --kind <login\|secret\|env\|card\|identity\|address> [--origin URL]… [--set FIELD=VALUE]… [--stdin] [--no-origin-binding] [--run-allowed on\|off] [--expires YYYY-MM-DD \| --expires-in Nd \| --no-expiry] [--description TEXT] [--field-description FIELD=TEXT]… [--draft]` | 비밀 필드는 숨김 프롬프트나 `--stdin`(JSON 객체)으로만 받는다 |
| `vault edit <name> [--set …] [--stdin] [--unset FIELD]… [--origin URL]… [--clear-origins] [--origin-binding on\|off] [--run-allowed on\|off] [--expires … \| --expires-in … \| --no-expiry] [--description TEXT \| --no-description] [--field-description FIELD=TEXT]… [--status active\|draft\|discarded]` | 보호를 약하게 하는 변경은 터미널의 사람 확인이 필요하다(아래) |
| `vault rm <name>` | |
| `vault list [--json]` | 이름·종류·도메인·상태·사이트·필드 이름(만료가 있으면 `expires 2027-01-31`·`expired`)만. `--json`은 `run_allowed`·`expires_at`·`expired`·`description`·`field_descriptions`도. 값은 보여 주지 않는다 |
| `vault import-env <name> <path>` | `.env` 파일을 env 항목으로 가져온다 |
| `vault export <path> [--passphrase-stdin]` | [age](https://age-encryption.org/v1) 형식의 암호화 백업. `age -d`로도 열린다. 기존 파일을 덮어쓰지 않는다 |
| `vault import <path> [--passphrase-stdin]` | 같은 이름의 항목은 건너뛴다 |
| `vault rotate-root` · `vault rotate-domain <credentials\|payment\|personal\|developer>` | 키 교체. 도중에 끊겨도 다음 실행 때 복구된다 |

```sh
cargo run -p keyharbor -- vault init
printf '{"password":"…"}' | cargo run -p keyharbor -- vault add github --kind login \
  --origin https://github.com --set username=me --stdin
cargo run -p keyharbor -- vault list
```

### 알아 둘 것

- **CLI는 평문을 출력하지 않는다.** `show` 같은 명령이 없다. 셸을 쓰는 에이전트가 CLI로 값을 꺼내지 못하게 하기 위해서다. 값 보기는 관리 앱(5단계)에서 한다.
- 비밀 값을 명령행 인자로 넘기지 않는다(`ps`에 보인다). `--set`은 아이디·이메일처럼 비밀이 아닌 필드에만 쓰고, 비밀 필드에 쓰면 거부된다.
- **Mac이나 키체인 항목을 잃으면 금고도 잃는다.** KeyHarbor는 복구 서비스를 제공하지 않는다. `vault export`로 만든 백업 파일과 그 비밀번호는 직접 보관한다.
- 키체인 항목이 사라지면 새 키를 만들지 않고 "vault key not found"로 멈춘다. 기존 암호문을 덮어쓰지 않기 위해서다.
- 처음 쓰거나 다시 빌드한 뒤 macOS가 키체인 접근을 물을 수 있다. "항상 허용"을 누르면 그 바이너리에 대해서는 다시 묻지 않는다.
- `KEYHARBOR_TEST_KEYSTORE_DIR`는 **테스트 전용**이다. `insecure-test-keystore` 기능으로 빌드한 테스트 바이너리에서만 동작하고, 일반 빌드에서는 무시된다. 켜지면 stderr에 경고를 출력한다.
- **만료**(R-37): 기본은 무기한. 만료일(UTC) 다음 날부터 값은 남지만 에이전트의 입력·주입은 `denied: <항목> (expired)`로 거부된다. `settings set default_item_ttl 90d`면 새 항목에 오늘+90일이 붙는다.
- **설명**(R-38): 항목·필드·세션에 "무엇에 쓰는지"를 500자까지 적는다. 필드는 종류의 기본 설명을 쓰다가 `--field-description`으로 바꿀 수 있다. 평문으로 저장되고 에이전트에게 보이므로 **비밀을 적지 않는다**. 금고 값이 든 설명은 거부된다.
- R-40 이전 금고는 처음 열 때 `secret`·`env` 항목을 developer 도메인으로 한 번 옮긴다(한 트랜잭션).
- 키 교체 중에 다른 명령이 금고를 바꾸면 교체는 아무것도 바꾸지 않고 실패한다. 다시 실행하면 된다. 교체 전에 열린 오래된 프로세스의 쓰기는 "vault changed; reopen"으로 거부된다.

## 치환 입력 · 마스킹 · 정책 · 사용 기록 (3단계)

에이전트는 비밀 값 대신 `{{secret:<항목>.<필드>}}`를 `browser_fill`·`browser_type`에 넘긴다(`vault_list`가 필드마다 보여 준다). 감독자가 그 자리에서 값을 꺼내 입력하고, 에이전트에게 가는 모든 응답(snapshot·URL·탭 제목·오류)에서 금고의 값을 다시 placeholder로 바꾼다. 설계: [02 §2.5.2–§2.5.5](../docs/design/02-system-architecture.md#252-placeholder-치환-입력), [03 §3.7–§3.8](../docs/design/03-security-whitepaper.md#37-마스킹-알고리즘).

- **입력 전 확인**: 항목이 있고 `discarded`·만료가 아니며, 정책이 그 에이전트에게 허용하고, origin 바인딩이 켜진 항목(`login`·`card` 기본)이면 **포커스가 있는 입력란의 frame** origin이 항목의 origin 목록과 정확히 같아야 한다. `secret`·`env`는 origin 목록이 있을 때 그 origin에만 들어간다. `login.password`는 password 칸(`type=password` 또는 `autocomplete` current-/new-password)에만, 모든 값은 텍스트를 받는 요소에만 들어간다. 포커스는 열린 shadow root와 frame을 품은 요소(iframe·object·embed…) 끝까지 따라가고, 탐색 중이면 기다리거나 거부한다. 하나라도 어긋나면 아무것도 입력하지 않고 `denied: <항목> (<이유>)`를 돌려준다. 이유는 `policy`, `origin`, `no-origin`, `field-kind`, `expired`, `not_found`, `tampered`, `discarded`, `focus`, `frame`, `navigating`, `locked`, `vault`(`keyharbor run`은 `run-not-allowed`도).
- **요청 속 값은 거부한다**: 에이전트가 보낸 URL·텍스트·selector에 금고 값이 있으면 요청을 거부하고(`mask_probe`), 세션에서 세 번이면 그 세션은 치환과 텍스트가 든 요청을 모두 거부한다. selector는 태그·`#id`·`.class`·`[name|type|id|aria-label|data-testid=…]`·공백/`>`·`:nth-of-type(n)`만 쓸 수 있다.
- 값을 받은 입력란은 세션 동안 snapshot에서 `value="[redacted]"`로 보인다(값을 고쳐도). 입력 뒤 포커스가 옮겨졌으면 오류를 돌려주고 그 세션의 치환을 잠근다.
- 입력 뒤 frame origin이 바뀌었으면 결과에 `"warning":"origin_changed_after_fill"`이 붙고 기록에 남는다. 이미 입력된 값은 되돌릴 수 없다(탐지이지 예방이 아니다).
- **마스킹**: 금고 전체의 4자 이상 값과 그 percent-encoding(form-urlencoded 포함)·HTML 엔티티·JSON 이스케이프 형태, 카드 번호는 숫자만·4자리 공백·4자리 하이픈 형태까지 가린다. `type=password` 입력란의 값은 snapshot에 넣지 않는다. 감독자는 첫 치환이나 첫 응답 때 금고를 열고, 금고가 바뀌면(`vault_version`) 다시 만든다. 값을 읽지 못하면 응답을 보내지 않는다.
- snapshot은 같은 프로세스의 iframe 내용을 `[frame] "<origin>"` 아래에 붙인다. 그 ref로 iframe 안 입력란을 채울 수 있다.

| 명령 | 설명 |
|---|---|
| `policy list [--json]` | `agent resource target effect` |
| `policy allow\|deny --agent A --resource R [--target T]` · `policy rm …` | R은 `session`·`item`·`domain`·`create_session`·`raw_cdp`, T는 세션 id·항목 이름·도메인(`credentials`·`payment`·`personal`·`developer`) 또는 `*`(기본). A가 `*`면 모든 에이전트 |
| `audit [--limit N] [--json]` | 사용 기록. `seq at agent kind item origin session result`. 값은 없다 |
| `settings get\|set KEY [VALUE]` | `audit_enabled true\|false`(`false`로 끄면 거부를 포함해 아무것도 기록하지 않는다. 그 밖의 값은 켜짐), `capture_enabled true\|false`(4단계), `default_item_ttl <N>d\|none` |
| `run [--agent A] (--env ITEM \| --env-file PATH \| --file VAR=ITEM.FIELD \| --template VAR=PATH)… -- CMD ARGS…` | `run_allowed` 항목(secret·env 기본 허용)만. env 항목의 변수(또는 템플릿 파일의 `KEY={{secret:…}}` 줄)를 자식 프로세스에만 넣는다. `--file`은 필드 하나를, `--template`은 placeholder를 채운 파일 사본을 0700 임시 폴더의 0600 파일로 만들어 그 경로를 VAR에 넣고 `run`이 끝나면 지운다. 인자의 `{{secret:…}}`도 채우되 `ps`로 보인다는 경고를 낸다(J-84). 자식의 그 stdout·stderr를 조각 단위로 가려서 내보낸다(줄바꿈 없는 출력도 잠시 멈추면 보이고, 값 중간에서 끊지 않는다, J-83). SIGTERM·SIGHUP은 자식에게 전달하고(Ctrl-C는 터미널이 자식에게 직접 보낸다), 종료 코드를 그대로 전달하며(신호면 128+번호), 자식의 core dump를 끈다 |

정책은 규칙이 없으면 모두 허용이다. 에이전트 규칙이 `*` 규칙을 이기고, 그다음 항목 > 도메인 > `*` 순으로 구체적인 규칙이 이기며, 같은 단계면 거부가 이긴다. MCP는 세션 도구(`session_open`·`session_create`·`session_endpoint`·`browser_*`)에, 감독자는 항목 사용에 정책을 적용한다. 기록되는 이벤트: `secret_used`, `secret_denied`, `origin_changed_after_fill`, `session_opened`, `session_stopped`, `handoff_requested`, `handoff_resumed`, `policy_changed`, `vault_item_changed`, `env_injected`, `substitution_locked`, `mask_probe`, `secret_denied_suppressed`(초당 10건을 넘은 거부 수), `first_use_origin`(identity·address가 처음 쓰인 origin), `site_state_changed`(사이트 로그인 상태 기록), `vault_requested`·`vault_request_answered`(정보 요청과 답), `credential_captured`(인계 중 포착, 에이전트 이름 `user`), `grant_changed`(`added`·`revoked`·`cleared`), `navigation_denied`(grant 밖 문서), `site_handed_off`(넘겨받은 쿠키 수).

**보호를 약하게 하는 작업은 사람이 확인한다**(R-35): `vault edit`의 origin 바인딩 끄기·origin 추가·`--run-allowed on`·만료 연장/제거, `policy allow`, deny 규칙의 `policy rm`, `settings set audit_enabled false`, `capture_enabled` 변경. stdin과 stderr가 모두 터미널이면 `/dev/tty`에서 `yes`를 받고, 아니면 `… needs confirmation from a person at a terminal (or the admin app)`으로 거부한다. pty를 흉내 내는 프로그램은 대신 답할 수 있으므로 **약한 경계**다([03 §3.12.1](../docs/design/03-security-whitepaper.md#3121-약화-작업은-사람이-확인한다-r-35)).

```sh
keyharbor vault import-env openai ./.env                 # OPENAI_API_KEY 등, developer 도메인
keyharbor run --env openai -- python agent.py            # 설정 파일에 평문 키를 두지 않는다
keyharbor vault import-env myapp-dev ./.env
keyharbor run --agent codex --env myapp-dev -- npm test
printf 'DATABASE_URL={{secret:myapp-dev.DATABASE_URL}}\n' > .env.keyharbor   # 비밀이 없어 커밋해도 된다
keyharbor run --env-file .env.keyharbor -- ./serve
keyharbor run --file GOOGLE_APPLICATION_CREDENTIALS=gcp.key -- ./deploy.sh   # 값을 파일로 (끝나면 지움)
keyharbor run --template CONF=app.yml.tmpl -- ./serve --config "\$CONF"   # placeholder를 채운 사본
keyharbor policy deny --agent codex --resource domain --target payment
```

## 사이트 로그인 상태 (4단계)

세션마다 origin별 로그인 상태(`logged_in`·`expired`·`unknown`)와 로그인에 쓴 항목 이름, 기록한 시각과 기록한 쪽(에이전트 이름, CLI는 `cli`)을 남긴다(`site_states`, 세션을 지우면 함께 지워진다). 값은 없다. 상태는 에이전트가 페이지를 보고 남긴 메모이고 KeyHarbor가 사이트에 가서 확인하지 않는다. 세션 정책을 따르고(`SessionOp::SiteState`), 범위 지정 서버는 자기 세션만 본다.

```sh
keyharbor site list <session-id>                      # origin  state  item  checked_at  recorded_by
keyharbor site set <session-id> https://github.com expired
```

에이전트가 금고에 없는 정보(카드, 주소, 모르는 비밀번호)를 `vault_request`로 요청하면 사용자가 터미널에서 답한다. 값은 숨김 입력이나 `--stdin` JSON으로 받아 바로 금고에 넣고(사용자가 넣었으므로 draft가 아니다), 에이전트는 이름과 placeholder만 받는다. 요청은 하루 동안 열려 있다.

```sh
keyharbor vault requests                               # id  agent  kind  name  origin  reason
keyharbor vault fulfill <id>                           # 숨김 입력 (또는 --stdin, --use 기존항목)
keyharbor vault decline <id>
```

draft(에이전트가 생성·저장했거나 포착한 항목)는 확정 전에도 placeholder로 쓸 수 있다. 번호 붙은 draft(`github~draft-1`)를 확정하면 원래 항목을 대체한다.

```sh
keyharbor vault confirm github~draft-1                # github을 이 값으로 바꾼다
keyharbor vault discard github~draft-2
keyharbor vault prune-drafts                           # 폐기된 항목과 draft_ttl(기본 7d)이 지난 draft 지우기
```

**grant**(R-43): 세션이 열 수 있는 사이트를 정한다. grant를 한 번이라도 받은 세션은 활성 grant가 덮는 사이트(같은 scheme·port의 호스트와 하위 도메인)의 문서만 연다. 링크 클릭이나 리디렉트로 가는 것도 막고, 이미지 같은 하위 리소스는 막지 않는다. grant가 끝나면(만료·회수) 그 사이트로 탐색할 수 없고, 감독자가 몇 초 안에 그 사이트의 쿠키를 지운다. 추가는 약화 작업이라 터미널에서 확인을 받는다.

```sh
keyharbor grant add <session-id> https://github.com --ttl 8h --reason "PR 리뷰"
keyharbor grant list <session-id>                     # id  origin  active|ended  until  created_by
keyharbor grant revoke <grant-id>
```

**사이트 넘기기**(R-42): 평소 Chrome에서 로그인한 사이트를 확장(`integrations/browser-extension`)으로 에이전트 세션에 넘긴다. `keyharbor browser install-host`로 native host를 등록하면, 확장이 그 사이트의 쿠키(고르면 `localStorage`)를 `keyharbor browser host`에 보낸다. host는 쿠키가 그 사이트 것인지 확인하고 실행 중인 세션의 감독자에게 넘긴다. 감독자는 그 사이트의 grant(TTL 동안)를 기록하고 쿠키를 넣은 뒤 사이트를 연다. 쿠키 값은 KeyHarbor DB·사용 기록·응답에 남지 않는다.

```sh
keyharbor browser install-host                         # Chrome·Chromium·Firefox에 host 등록 (우리 확장만 허용)
```

**로그인 포착**(기본 꺼짐, `keyharbor settings set capture_enabled true`로 켠다): 인계(`session_wait_auth`) 중에 사람이 브라우저에서 로그인 폼을 제출하면, 감독자가 그 아이디·비밀번호를 draft로 저장한다. 금고에 같은 값이 있으면 아무것도 하지 않는다. 비밀번호가 다르면 `<이름>~draft-N`, 처음 보는 계정이면 호스트 이름으로 저장한다. 인계 중에는 에이전트의 입력 도구가 거절되므로 포착된 값은 사람이 친 것이다. 페이지 스크립트가 직접 제출한 폼은 세지 않는다. 다른 사이트 iframe과 submit 이벤트 없이 보내는 폼은 놓친다.

### 3단계의 한계

- **에이전트 이름은 기록용 표시다.** 같은 OS 사용자는 다른 이름으로 MCP 서버를 띄울 수 있다. 정책은 오작동 방지이지 보안 경계가 아니다.
- **`keyharbor run`의 자식은 평문을 갖는다.** 값을 파일이나 네트워크로 보내는 것은 막지 못한다(T13). 그래서 origin 바인딩이 켜진 항목과 `run_allowed`가 꺼진 항목은 주입하지 않는다. MCP 서버 설정은 명령을 `keyharbor run --env … -- <server>`로 감싸면 키를 설정 파일에 두지 않아도 된다.
- 4–6자리 PIN처럼 짧은 값은 세션을 여러 번 열어 마스커 거부로 조금씩 추측할 수 있다(세션당 세 번). 탐색 이벤트가 입력 뒤에 오면 확인~입력 사이 탐색을 놓칠 수 있다. 값을 받은 입력란 밖으로 페이지가 옮긴 편집 값은 가리지 못한다. `run` 출력에서 자식이 값의 앞부분만 찍고 끝나면 그 조각은 값이 아니므로 가려지지 않는다.
- 다른 사이트의 iframe(예: 실제 결제사 도메인의 카드 입력란)은 별도 CDP target이라 snapshot에 나오지 않고 입력할 수 없다.
- 스크린샷의 픽셀, base64 같은 다른 인코딩, 여러 요소에 쪼개진 값은 가리지 못한다. 3자 이하 값(3자리 CVC, 월 등)은 가리지 않는다.
- 마스킹 오토마톤이 가진 값 사본과 자식 프로세스 환경 변수 사본은 메모리에서 지워지지 않는다.
- 사용 기록 쓰기는 최선 노력이다. 기록에 실패해도 이미 한 입력·변경은 그대로다.
- 실제 Keychain에서 감독자의 첫 치환 때 접근 확인창이 뜨는지는 아직 확인하지 않았다.
