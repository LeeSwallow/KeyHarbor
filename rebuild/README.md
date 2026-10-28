# KeyHarbor: 재구현

이 디렉터리는 기존 프로토타입과 별도의 데이터 경로를 쓰는 새 Rust 워크스페이스다. 아직 재구현 중이다. 지금 동작하는 것은 브라우저 세션과 조작 도구(1단계), 로컬 금고(2단계), placeholder 치환 입력·응답 마스킹·정책·사용 기록·`keyharbor run`(3단계), 이를 제공하는 CLI와 MCP 서버다. 기존 데이터를 자동으로 가져오거나 변경하지 않는다. 설계는 [`../docs/design/`](../docs/design/README.md)에 있다.

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
| interfaces | `keyharbor` (CLI) | clap 명령, 어댑터 조립, `run`의 프로세스 실행 | `session_cmd`, `vault_cmd`, `policy_cmd`, `audit_cmd`, `run_cmd` |
| | `keyharbor-mcp` | stdio MCP 서버 | `server`, `scope`, `tools/{session, browser, vault}` |
| adapters | `keyharbor-broker` | 감독자, Chrome CDP pipe, 제어 socket, `SessionRuntime` | `cdp/{client, protocol}`, `browser/{navigate, snapshot, input, tabs, screenshot, wait, frames}`(crate 내부), `supervisor/{launch, lock}`, `ops`, `runtime` |
| | `keyharbor-sqlite` | Diesel 저장소, 저장소 포트 구현 | `connection`, `contexts/{vault, session, access, audit, config}/{schema, models, mappers, repo}` |
| | `keyharbor-vault` | 암호 레코드·메타 MAC·키 분리, `KeyStore`(Keychain) | `record`, `meta`, `keys`, `keystore` |
| security | `keyharbor-guard` | 평문의 판정과 집행: PDP·PEP·사용 기록·`GuardedBrowser` | `pdp`, `pep/{substitute, mask, origin}`, `audit`, `run`, `guard`, `browser` |
| application | `keyharbor-core` | 유스케이스(포트만 사용), `PlaintextAccess` | `vault/{open, items, summary, resolve}`, `session/{lifecycle, handoff}`, `rotate`, `transfer`, `dotenv` |
| domain | `keyharbor-domain` | 엔티티·값 객체·규칙, 항목 종류 레지스트리 `KINDS`, 포트(trait) | `vault`, `policy`, `audit`, `secret`, `ports/{vault, session, access, audit, keys, browser, runtime}` |

- 평문 값은 `Vault::resolve`·`Vault::mask_values`에서만 나오고, 둘 다 guard가 검사 뒤 발급하는 `PlaintextAccess`를 요구한다. 브라우저 조작은 제어 socket → `ops` → `GuardedBrowser`로만 간다.
- **스키마 확장**: 저장소는 바운디드 컨텍스트별로 나뉘어 있다. 새 컨텍스트(예: 4단계 `site_states`)는 `crates/sqlite/src/contexts/<이름>/`(schema·models·mappers·repo) + `crates/sqlite/migrations/<이름>/<timestamp>_<이름>_<설명>/up.sql` + `contexts::CONTEXTS`에 한 줄이면 된다. 버전 타임스탬프는 모든 컨텍스트에서 유일해야 한다. 원시 SQL은 마이그레이션과 테스트에만 쓴다.
- **항목 종류 확장**: `keyharbor_domain::vault::KINDS`에 `KindSpec` 하나(필드마다 `secret`·`maskable`)와 `ItemKind` 변형 하나를 더한다. 스키마·origin 바인딩 기본값·CLI의 비밀 필드 거부·마스킹이 이 표를 읽는다.
- 테스트 전용 기능: `keyharbor-vault/insecure-test-keystore`, `keyharbor-sqlite/test-support`. 릴리스 빌드에 켜지지 않는지 `scripts/check-release-features.sh`가 확인한다.

## 브라우저 세션 (1단계)

세션마다 전용 Chrome 프로필(`profiles/<id>/`)을 쓴다. `open`은 세션을 예약하고 **감독 프로세스**를 띄운다. 감독자는 OS 잠금(`run/<id>.lock`)을 쥐고, Chrome을 `--remote-debugging-pipe`로 실행해 CDP를 직접 소유하며(포트를 열지 않는다), `run/<id>.sock`(0600)으로 명령을 받는다. 브라우저가 닫히면 `stopped`를 기록하고 socket을 지운다. 설계: [02 §2.3–§2.6](../docs/design/02-system-architecture.md#23-c4-level-2--컨테이너), [ADR-0004](../docs/design/adr/0004-supervisor-owns-cdp.md).

| 명령 | 설명 |
|---|---|
| `create <name>` · `list` | `list`는 `id 이름 상태[ cdp]`. 상태가 활성인데 감독자가 없으면 `recovery_required` |
| `open <id> [--url U] [--cdp] [--browser PATH]` | 기본 브라우저는 `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`. `--url`은 HTTPS(또는 localhost HTTP), 자격 증명·query·fragment 없이 |
| `stop <id>` | 감독자에게 `close`를 보내 `Browser.close`로 끝내고 `stopped`까지 기다린다(10초 안에 안 끝나면 강제 종료) |
| `wait-auth <id> [--origin O]` · `handoff <id>` · `resume <id> <request-id>` | 사용자에게 로그인을 넘기고, 같은 request id로만 재개. 새 요청은 이전 대기 요청을 대체한다 |
| `recover <id> --confirm-browser-closed` | 감독자가 죽은 세션(`recovery_required`·`failed`)을 `stopped`로. 감독자가 살아 있으면 거부 |
| `endpoint <id>` | `--cdp`로 연 세션의 raw CDP 주소(`ws://127.0.0.1:…`). **켜면 KeyHarbor의 보호가 우회된다** |
| `mcp [--session ID] [--allow-cdp] [--agent NAME] [--browser PATH]` | stdio MCP 서버. `--agent`가 없으면 에이전트 이름은 `mcp` |

`KEYHARBOR_BROWSER_HEADLESS=1`이면 감독자가 Chrome에 `--headless=new`를 붙인다(테스트·CI용).

### MCP 서버

에이전트 설정 예(Claude Code `.mcp.json` 형식):

```json
{"mcpServers": {"keyharbor": {"command": "/abs/path/keyharbor", "args": ["mcp", "--agent", "claude"]}}}
```

| 분류 | 도구 |
|---|---|
| 세션 | `session_list`, `session_create`, `session_open {id, url?, cdp?}`, `session_stop`, `session_wait_auth {id, origin?}`, `session_wait_for_resume {id, request_id, timeout_seconds 1–300}` |
| 세션 (선택) | `session_endpoint` — `--allow-cdp`일 때만 등록 |
| 브라우저 | `browser_navigate`, `browser_snapshot`, `browser_screenshot`, `browser_click`, `browser_fill`, `browser_type`, `browser_press`, `browser_tabs`, `browser_new_tab`, `browser_switch_tab`, `browser_wait` — 모두 `session` 인자 |
| 금고 | `vault_list` — 이름·종류·도메인·필드 이름·필드별 placeholder·origin·바인딩·상태·출처만 |

`browser_snapshot`은 접근성 트리를 `[ref] role "name"` 줄로 돌려준다. `browser_click`·`browser_fill`은 `ref` 또는 CSS `selector`를 받는다. `--session ID`로 띄운 서버는 그 세션만 보고 쓸 수 있으며 새 세션을 만들 수 없고, 같은 세션의 범위 지정 서버는 하나만 뜬다(`run/mcp-<id>.lock`). 재개(`resume`)와 복구(`recover`)는 사람이 CLI로 한다. MCP 도구에는 없다.

### 1단계의 한계

- 같은 세션을 여러 MCP 서버가 쓰면 명령은 감독자에서 하나씩 실행되지만 브라우저 상태(활성 탭 등)는 공유된다.
- 제어 socket 경로는 104바이트를 넘을 수 없다. 데이터 경로가 너무 길면 `open`이 오류를 낸다.
- 제어 socket은 같은 OS 사용자의 다른 프로세스를 막지 못한다. 보안 경계가 아니라 프로세스 간 통로다.
- macOS에서 사용자가 마지막 탭을 닫으면 감독자가 브라우저를 닫고 `stopped`를 기록한다(창 없는 Chrome이 남지 않게).

## 로컬 금고 (2단계)

세 도메인(자격 증명·결제·개인정보)의 항목을 암호화해 저장한다. 루트 키는 macOS login keychain(`io.github.leeswallow.keyharbor`)에, 나머지는 SQLite에 암호문으로 둔다. 방식은 [07 암호화 설계](../docs/design/07-cryptography-design.md)를 따른다.

| 명령 | 설명 |
|---|---|
| `vault init` | 금고와 루트 키를 만든다. 이미 있으면 덮어쓰지 않는다 |
| `vault add <name> --kind <login\|secret\|env\|card\|identity\|address> [--origin URL]… [--set FIELD=VALUE]… [--stdin] [--no-origin-binding] [--draft]` | 비밀 필드는 숨김 프롬프트나 `--stdin`(JSON 객체)으로만 받는다 |
| `vault edit <name> [--set …] [--stdin] [--unset FIELD]… [--origin URL]… [--clear-origins] [--origin-binding on\|off] [--status active\|draft\|discarded]` | |
| `vault rm <name>` | |
| `vault list [--json]` | 이름·종류·도메인·상태·사이트·필드 이름만. 값은 보여 주지 않는다 |
| `vault import-env <name> <path>` | `.env` 파일을 env 항목으로 가져온다 |
| `vault export <path> [--passphrase-stdin]` | [age](https://age-encryption.org/v1) 형식의 암호화 백업. `age -d`로도 열린다. 기존 파일을 덮어쓰지 않는다 |
| `vault import <path> [--passphrase-stdin]` | 같은 이름의 항목은 건너뛴다 |
| `vault rotate-root` · `vault rotate-domain <credentials\|payment\|personal>` | 키 교체. 도중에 끊겨도 다음 실행 때 복구된다 |

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
- 키 교체 중에 다른 명령이 금고를 바꾸면 교체는 아무것도 바꾸지 않고 실패한다. 다시 실행하면 된다. 교체 전에 열린 오래된 프로세스의 쓰기는 "vault changed; reopen"으로 거부된다.

## 치환 입력 · 마스킹 · 정책 · 사용 기록 (3단계)

에이전트는 비밀 값 대신 `{{secret:<항목>.<필드>}}`를 `browser_fill`·`browser_type`에 넘긴다(`vault_list`가 필드마다 보여 준다). 감독자가 그 자리에서 값을 꺼내 입력하고, 에이전트에게 가는 모든 응답(snapshot·URL·탭 제목·오류)에서 금고의 값을 다시 placeholder로 바꾼다. 설계: [02 §2.5.2–§2.5.5](../docs/design/02-system-architecture.md#252-placeholder-치환-입력), [03 §3.7–§3.8](../docs/design/03-security-whitepaper.md#37-마스킹-알고리즘).

- **입력 전 확인**: 항목이 있고 `discarded`가 아니며, 정책이 그 에이전트에게 허용하고, origin 바인딩이 켜진 항목(`login`·`card` 기본)이면 **포커스가 있는 입력란의 frame** origin이 항목의 origin 목록과 정확히 같아야 한다. 하나라도 어긋나면 아무것도 입력하지 않고 `denied: <항목> (<이유>)`를 돌려준다. 이유는 `policy`, `origin`, `not_found`, `tampered`, `discarded`, `focus`, `frame`, `vault`.
- 입력 뒤 frame origin이 바뀌었으면 결과에 `"warning":"origin_changed_after_fill"`이 붙고 기록에 남는다. 이미 입력된 값은 되돌릴 수 없다(탐지이지 예방이 아니다).
- **마스킹**: 금고 전체의 4자 이상 값과 그 percent-encoding·HTML 엔티티 형태, 카드 번호는 숫자만·4자리 공백·4자리 하이픈 형태까지 가린다. `type=password` 입력란의 값은 snapshot에 넣지 않는다. 감독자는 첫 치환이나 첫 응답 때 금고를 열고, 금고가 바뀌면(`vault_version`) 다시 만든다. 값을 읽지 못하면 응답을 보내지 않는다.
- snapshot은 같은 프로세스의 iframe 내용을 `[frame] "<origin>"` 아래에 붙인다. 그 ref로 iframe 안 입력란을 채울 수 있다.

| 명령 | 설명 |
|---|---|
| `policy list [--json]` | `agent resource target effect` |
| `policy allow\|deny --agent A --resource R [--target T]` · `policy rm …` | R은 `session`·`item`·`domain`·`create_session`·`raw_cdp`, T는 세션 id·항목 이름·도메인(`credentials`·`payment`·`personal`) 또는 `*`(기본). A가 `*`면 모든 에이전트 |
| `audit [--limit N] [--json]` | 사용 기록. `seq at agent kind item origin session result`. 값은 없다 |
| `settings get\|set audit_enabled [true\|false]` | 끄면 거부를 포함해 아무것도 기록하지 않는다 |
| `run [--agent A] (--env ITEM \| --env-file PATH)… -- CMD ARGS…` | env 항목의 변수(또는 템플릿 파일의 `KEY={{secret:…}}` 줄)를 자식 프로세스에만 넣고, 그 stdout·stderr를 줄 단위로 가려서 내보낸다. 종료 코드를 그대로 전달하고(신호면 128+번호), 자식의 core dump를 끈다 |

정책은 규칙이 없으면 모두 허용이다. 에이전트 규칙이 `*` 규칙을 이기고, 그다음 항목 > 도메인 > `*` 순으로 구체적인 규칙이 이기며, 같은 단계면 거부가 이긴다. MCP는 세션 도구(`session_open`·`session_create`·`session_endpoint`·`browser_*`)에, 감독자는 항목 사용에 정책을 적용한다. 기록되는 이벤트: `secret_used`, `secret_denied`, `origin_changed_after_fill`, `session_opened`, `session_stopped`, `handoff_requested`, `handoff_resumed`, `policy_changed`, `vault_item_changed`, `env_injected`.

```sh
keyharbor vault import-env myapp-dev ./.env
keyharbor run --agent codex --env myapp-dev -- npm test
printf 'DATABASE_URL={{secret:myapp-dev.DATABASE_URL}}\n' > .env.keyharbor   # 비밀이 없어 커밋해도 된다
keyharbor run --env-file .env.keyharbor -- ./serve
keyharbor policy deny --agent codex --resource domain --target payment
```

### 3단계의 한계

- **에이전트 이름은 기록용 표시다.** 같은 OS 사용자는 다른 이름으로 MCP 서버를 띄울 수 있다. 정책은 오작동 방지이지 보안 경계가 아니다.
- **`keyharbor run`의 자식은 평문을 갖는다.** 값을 파일이나 네트워크로 보내는 것은 막지 못한다(T13). 그래서 origin 바인딩이 켜진 항목은 주입하지 않는다. 마스킹은 줄 단위라 여러 줄에 걸친 값은 놓칠 수 있다.
- 다른 사이트의 iframe(예: 실제 결제사 도메인의 카드 입력란)은 별도 CDP target이라 snapshot에 나오지 않고 입력할 수 없다.
- 스크린샷의 픽셀, base64 같은 다른 인코딩, 여러 요소에 쪼개진 값은 가리지 못한다. 3자 이하 값(3자리 CVC, 월 등)은 가리지 않는다.
- 마스킹 오토마톤이 가진 값 사본과 자식 프로세스 환경 변수 사본은 메모리에서 지워지지 않는다.
- 사용 기록 쓰기는 최선 노력이다. 기록에 실패해도 이미 한 입력·변경은 그대로다.
- 실제 Keychain에서 감독자의 첫 치환 때 접근 확인창이 뜨는지는 아직 확인하지 않았다.
