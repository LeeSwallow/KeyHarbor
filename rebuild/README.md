# KeyHarbor: 재구현

이 디렉터리는 기존 프로토타입과 별도의 데이터 경로를 쓰는 새 Rust 워크스페이스다. 아직 재구현 중이다. 지금 동작하는 것은 브라우저 세션과 조작 도구(1단계), 로컬 금고(2단계), 이를 제공하는 CLI와 MCP 서버다. 기존 데이터를 자동으로 가져오거나 변경하지 않는다. 설계는 [`../docs/design/`](../docs/design/README.md)에 있다.

```sh
cargo test --workspace
cargo run -p keyharbor -- create work
cargo run -p keyharbor -- list
```

crate: `keyharbor-domain`(규칙), `keyharbor-sqlite`(저장), `keyharbor-vault`(암호), `keyharbor-core`(금고 유스케이스), `keyharbor-broker`(감독자·CDP·제어 socket), `keyharbor-mcp`(MCP 서버), `keyharbor`(CLI).

기본 데이터 경로는 `~/Library/Application Support/KeyHarbor`이며 `--data-dir` 또는 `KEYHARBOR_HOME`으로 변경할 수 있다. 세션 이름과 금고 항목 이름·종류·사이트(origin) 같은 메타데이터는 SQLite에 평문으로 저장된다. 이름에 비밀을 넣지 않는다.

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
| `mcp [--session ID] [--allow-cdp] [--agent NAME] [--browser PATH]` | stdio MCP 서버 |

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
| 금고 | `vault_list` — 이름·종류·도메인·필드 이름·origin만 |

`browser_snapshot`은 접근성 트리를 `[ref] role "name"` 줄로 돌려준다. `browser_click`·`browser_fill`은 `ref` 또는 CSS `selector`를 받는다. `--session ID`로 띄운 서버는 그 세션만 보고 쓸 수 있으며 새 세션을 만들 수 없고, 같은 세션의 범위 지정 서버는 하나만 뜬다(`run/mcp-<id>.lock`). 재개(`resume`)와 복구(`recover`)는 사람이 CLI로 한다. MCP 도구에는 없다.

### 1단계의 한계

- **`browser_fill`·`browser_type`은 받은 텍스트를 그대로 입력한다.** placeholder 치환과 응답 마스킹은 3단계부터다. 그 전에는 비밀번호를 에이전트에게 주고 입력시키지 말고 `wait-auth`로 사용자에게 넘긴다. 스냅샷에는 입력란의 값이 보일 수 있다.
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
