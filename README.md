<p align="center"><img src="docs/design/assets/app-icon.svg" width="96" alt="KeyHarbor"></p>

# KeyHarbor

**에이전트가 로그인에서 멈추지 않게. 비밀번호는 내 Mac에.**

KeyHarbor는 Claude Code·Codex 같은 AI 에이전트가 권한 정책 안에서 중단 없이 일하도록, 비밀번호·카드·개인정보·`.env` 변수와 브라우저 세션을 **내 컴퓨터 안에서만** 관리하는 무료 오픈소스 도구입니다. 외부 금고나 클라우드 계정이 필요 없습니다.

> **상태: 개발 중 — 아직 릴리스하지 않았습니다.** 지금 동작하는 것은 로컬 암호화 금고 CLI와 브라우저 세션·조작 MCP 도구(macOS)입니다. 에이전트가 비밀번호를 보지 않고 브라우저에 입력하는 기능, 관리 앱, Claude Code 플러그인은 [로드맵](ROADMAP.md)에 따라 만들고 있으며, 모두 갖춰지면 첫 릴리스를 냅니다.

## 소스에서 빌드

[Rust](https://rustup.rs) stable과 macOS가 필요합니다.

```sh
git clone https://github.com/LeeSwallow/KeyHarbor
cd KeyHarbor/rebuild
cargo build --release -p keyharbor
cp target/release/keyharbor /usr/local/bin/    # 또는 PATH에 있는 아무 곳
```

첫 릴리스부터는 [Releases](https://github.com/LeeSwallow/KeyHarbor/releases)의 바이너리와 Homebrew(`brew install --cask leeswallow/tap/keyharbor`)로 설치할 수 있습니다.

## 빠른 시작

```sh
keyharbor vault init                                   # 금고와 루트 키(Keychain) 만들기
printf '{"password":"…"}' | keyharbor vault add blog \
  --kind login --origin https://blog.example.com --set username=me --stdin
keyharbor vault list                                   # 이름·종류·사이트만, 값은 보이지 않음
keyharbor vault import-env myapp ./.env                # .env 변수 묶음 가져오기
keyharbor vault export ~/keyharbor-backup.age          # age 형식 암호화 백업
```

항목 종류는 `login`, `secret`, `env`, `card`, `identity`, `address`이고 자격 증명·결제·개인정보 세 도메인으로 나뉘어 각각 다른 키로 암호화됩니다. 전체 명령은 [`rebuild/README.md`](rebuild/README.md)에 있습니다.

## 알아 둘 것

- **CLI는 평문을 출력하지 않습니다.** 셸을 쓰는 에이전트가 값을 꺼내지 못하게 하려는 설계입니다. 비밀 값은 숨김 프롬프트나 `--stdin`으로만 받고, 명령행 인자로는 받지 않습니다.
- **Mac이나 Keychain 항목을 잃으면 금고도 잃습니다.** 복구 서비스는 없습니다. `vault export`로 만든 백업과 그 비밀번호는 직접 보관하세요.
- 서명하지 않은 바이너리라서 업데이트 후 macOS가 Keychain 접근을 다시 물을 수 있습니다. "항상 허용"을 누르면 됩니다.
- 데이터는 `~/Library/Application Support/KeyHarbor`에 있습니다(`--data-dir`, `KEYHARBOR_HOME`으로 변경). 항목 이름·사이트 같은 메타데이터는 평문이므로 이름에 비밀을 넣지 마세요.

## 문서

| 문서 | 내용 |
|---|---|
| [로드맵](ROADMAP.md) | 단계별 계획과 현재 상태 |
| [설계 문서](docs/design/README.md) | 아키텍처, 보안 화이트페이퍼, 암호화 설계, ADR |
| [보안 정책](SECURITY.md) | 약속하는 것과 하지 않는 것, 취약점 신고 |
| [기여 안내](CONTRIBUTING.md) | 이슈부터 릴리스까지의 흐름 |
| [프로토타입](docs/prototype.md) | 루트 crate(브라우저 세션 프로토타입) 설명 |

## 라이선스

[MIT](LICENSE-MIT) 또는 [Apache-2.0](LICENSE-APACHE) 중 선택.
