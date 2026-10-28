# KeyHarbor: 재구현

이 디렉터리는 기존 프로토타입과 별도의 데이터 경로를 쓰는 새 Rust 워크스페이스다. 아직 재구현 중이며 브라우저 실행, 인증 인계, MCP 연동은 구현되지 않았다. 기존 데이터를 자동으로 가져오거나 변경하지 않는다. 설계는 [`../docs/design/`](../docs/design/README.md)에 있다.

```sh
cargo test --workspace
cargo run -p keyharbor -- create work
cargo run -p keyharbor -- list
```

기본 데이터 경로는 `~/.keyharbor-rebuild`이며 `--data-dir` 또는 `KEYHARBOR_HOME`으로 변경할 수 있다. 세션 이름과 금고 항목 이름·종류·사이트(origin) 같은 메타데이터는 SQLite에 평문으로 저장된다. 이름에 비밀을 넣지 않는다.

## 로컬 금고 (2단계)

세 도메인(자격 증명·결제·개인정보)의 항목을 암호화해 저장한다. 루트 키는 macOS login keychain(`dev.keyharbor.vault`)에, 나머지는 SQLite에 암호문으로 둔다. 방식은 [07 암호화 설계](../docs/design/07-cryptography-design.md)를 따른다.

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
