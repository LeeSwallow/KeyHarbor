<p align="center"><img src="docs/design/assets/app-icon.svg" width="96" alt="KeyHarbor"></p>

# KeyHarbor

**에이전트가 로그인에서 멈추지 않게. 비밀번호는 내 Mac에.**

KeyHarbor는 Claude Code·Codex 같은 AI 에이전트가 권한 정책 안에서 중단 없이 일하도록, 비밀번호·카드·개인정보·`.env` 변수와 브라우저 세션을 **내 컴퓨터 안에서만** 관리하는 무료 오픈소스 도구입니다. 외부 금고나 클라우드 계정이 필요 없습니다.

> **상태: 첫 릴리스 v0.1.0 (macOS).** 들어 있는 것은 로컬 암호화 금고 CLI, 브라우저 세션·조작 MCP 도구, 에이전트가 값을 보지 않고 `{{secret:항목.필드}}`로 입력하는 치환·응답 마스킹·권한 정책·사용 기록, `.env` 변수를 자식 프로세스에만 넣는 `keyharbor run`입니다(macOS). Claude Code·Codex 플러그인([docs/plugin.md](docs/plugin.md))과 관리 앱(메뉴 막대 앱, 보안 입력 폼, 알림)도 있습니다. 남은 일은 [로드맵](ROADMAP.md)에 있습니다.

## 설치

| 무엇 | 어떻게 |
|---|---|
| CLI (Homebrew) | `brew install --cask leeswallow/tap/keyharbor` |
| CLI (직접) | [Releases](https://github.com/LeeSwallow/KeyHarbor/releases)의 `keyharbor_<버전>_darwin_<arm64·amd64>.tar.gz`를 풀어 `keyharbor`를 PATH에 둡니다 |
| 관리 앱 | Releases의 `keyharbor-app-<버전>-arm64.dmg`를 열어 `KeyHarbor.app`을 응용 프로그램으로 끌어다 놓습니다. CLI가 앱 안(`Contents/Helpers/keyharbor`)에 들어 있습니다. Apple silicon 전용 |

아직 서명·공증하지 않았습니다. Homebrew는 설치할 때 격리 표시를 지웁니다. 직접 받은 CLI나 앱은 처음 한 번 `xattr -dr com.apple.quarantine <경로>`를 실행하거나, Finder에서 앱을 우클릭 → 열기로 엽니다. 받은 파일은 함께 올린 `checksums.txt`·`.sha256`로 확인할 수 있습니다.

## 소스에서 빌드

[Rust](https://rustup.rs) stable과 macOS가 필요합니다.

```sh
git clone https://github.com/LeeSwallow/KeyHarbor
cd KeyHarbor
cargo build --release -p keyharbor
cp target/release/keyharbor /usr/local/bin/    # 또는 PATH에 있는 아무 곳
```

Claude Code·Codex 플러그인(MCP 서버 등록과 스킬): `/plugin marketplace add LeeSwallow/KeyHarbor` → `/plugin install keyharbor@keyharbor`. 자세한 것은 [docs/plugin.md](docs/plugin.md).

## 빠른 시작

```sh
keyharbor vault init                                   # 금고와 루트 키(Keychain) 만들기
printf '{"password":"…"}' | keyharbor vault add blog \
  --kind login --origin https://blog.example.com --set username=me --stdin
keyharbor vault list                                   # 이름·종류·사이트만, 값은 보이지 않음
keyharbor vault list --prefix myapp/ --tag prod            # 경로 이름(myapp/prod/db)과 태그로 거르기
keyharbor vault import-env myapp ./.env                # .env 변수 묶음 가져오기
keyharbor vault import-chrome ~/Downloads/passwords.csv  # Chrome 비밀번호 내보내기 가져오기
keyharbor vault export ~/keyharbor-backup.age          # age 형식 암호화 백업
keyharbor run --env myapp -- npm test                  # 자식에만 변수 주입, 출력의 값은 가려짐
keyharbor policy deny --agent codex --resource domain --target payment
keyharbor audit --limit 20                             # 누가 어떤 항목을 어디에 썼는지 (값 없음)
keyharbor site list <session-id>                      # 세션의 사이트별 로그인 상태 (에이전트가 남긴 메모)
keyharbor vault requests                               # 에이전트가 요청한 정보 (fulfill <id> 로 답하기)
keyharbor grant add <session-id> https://github.com --ttl 8h   # 이 세션이 열 수 있는 사이트 (사람 확인)
keyharbor browser install-host                         # 확장으로 평소 Chrome의 사이트 로그인을 세션에 넘기기
```

에이전트는 MCP 도구 `browser_fill`에 `{{secret:blog.password}}`처럼 placeholder를 넘기거나, `browser_login {item: "blog"}`으로 로그인 폼을 한 번에 채웁니다. 등록된 사이트(origin)의 입력란일 때만 값이 들어가고(비밀번호는 비밀번호 칸에만), 에이전트가 받는 결과에서는 값이 다시 placeholder로 바뀝니다.

API 키는 `keyharbor vault import-env openai .env` 뒤 `keyharbor run --env openai -- <명령>`으로 주입합니다. `secret`·`env` 항목은 `run`에 기본으로 쓸 수 있고, 브라우저에는 origin을 등록했을 때 그 사이트에만 들어갑니다. 항목·필드·세션에는 에이전트가 읽는 설명을, 항목에는 만료일(`--expires`, 기본 무기한)을 붙일 수 있습니다. browser-use와 함께 쓰는 방법: [docs/guides/browser-use.md](docs/guides/browser-use.md).

항목 종류는 `login`, `secret`, `env`, `card`, `identity`, `address`이고 자격 증명(`login`)·결제(`card`)·개인정보(`identity`·`address`)·개발자(`secret`·`env`, API 키와 환경 변수) 네 도메인으로 나뉘어 각각 다른 키로 암호화됩니다. 전체 명령은 [`crates/README.md`](crates/README.md)에 있습니다.

## 알아 둘 것

- **CLI는 평문을 출력하지 않습니다.** 셸을 쓰는 에이전트가 값을 꺼내지 못하게 하려는 설계입니다. 비밀 값은 숨김 프롬프트나 `--stdin`으로만 받고, 명령행 인자로는 받지 않습니다.
- **보호를 약하게 하는 변경은 터미널에서 `yes`를 쳐야 합니다**(origin 바인딩 끄기, origin 추가, `run` 허용, 정책 allow, 기록 끄기 등). 터미널이 없으면 거부됩니다. 다만 터미널을 흉내 내는 프로그램은 대신 답할 수 있어 강한 경계는 아닙니다.
- **Mac이나 Keychain 항목을 잃으면 금고도 잃습니다.** 복구 서비스는 없습니다. `vault export`로 만든 백업과 그 비밀번호는 직접 보관하세요.
- 서명하지 않은 바이너리라서 업데이트 후 macOS가 Keychain 접근을 다시 물을 수 있습니다. "항상 허용"을 누르면 됩니다.
- `keyharbor run`의 자식 프로세스는 값을 평문으로 갖습니다. 그 프로세스가 값을 밖으로 보내는 것은 막지 못하고, 출력만 가립니다.
- 데이터는 `~/Library/Application Support/KeyHarbor`에 있습니다(`--data-dir`, `KEYHARBOR_HOME`으로 변경). 항목 이름·사이트 같은 메타데이터는 평문이므로 이름에 비밀을 넣지 마세요.

## 문서

| 문서 | 내용 |
|---|---|
| [로드맵](ROADMAP.md) | 단계별 계획과 현재 상태 |
| [설계 문서](docs/design/README.md) | 아키텍처, 보안 화이트페이퍼, 암호화 설계, ADR |
| [디자인 시스템](design-system/README.md) | 토큰·컴포넌트·패턴 계층, 아이콘, 쇼케이스 |
| [브라우저 확장](integrations/browser-extension/README.md) | 평소 Chrome·Firefox의 사이트 로그인을 에이전트 세션으로 넘기기(4단계에서 native host와 연결) |
| [보안 정책](SECURITY.md) | 약속하는 것과 하지 않는 것, 취약점 신고 |
| [기여 안내](CONTRIBUTING.md) | 이슈부터 릴리스까지의 흐름 |
| [프로토타입](docs/history/prototype.md) | `prototype/`(브라우저 세션 프로토타입) 설명 |

## 라이선스

[MIT](LICENSE-MIT) 또는 [Apache-2.0](LICENSE-APACHE) 중 선택.
