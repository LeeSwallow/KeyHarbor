# 협업 저장소 구성과 v0.1.0 릴리스 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** stickypane의 협업 자산을 KeyHarbor에 맞게 들여오고, 공개 저장소·CI·태그 릴리스(GitHub Release + Homebrew cask)·GitHub Pages 웹 페이지를 갖춘 뒤 `v0.1.0`을 낸다.

**Architecture:** 제품 바이너리는 `rebuild/` 워크스페이스의 `keyharbor`(패키지 `keyharbor`, crate `crates/cli`)다. CI는 GitHub Actions(macOS·Ubuntu), 릴리스는 macOS 러너의 GoReleaser v2 Rust builder, 웹 페이지는 `site/`의 정적 파일을 `actions/deploy-pages`로 올린다. 협업 흐름(이슈 → 브랜치 → PR → squash → 태그)과 스크립트는 stickypane(`../stickypane`)의 것을 옮긴다.

**Tech Stack:** Rust 1.97 (rustup stable), GitHub Actions, GoReleaser v2 (`builder: rust`, `tool: cargo`), cargo-deny, cargo-about, gh CLI, POSIX sh, 정적 HTML/CSS.

**Spec:** `docs/superpowers/specs/2026-10-28-collaboration-and-release-design.md` (§7 리뷰 이후 결정 포함). 참고 원본: stickypane의 `AGENTS.md`, `CONTRIBUTING.md`, `.github/`, `.githooks/`, `scripts/`, `.goreleaser.yaml`.

## Global Constraints

- 모든 셸 명령 앞에 `export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$HOME/.cargo/bin:$PATH"`.
- 허용된 명령만 쓴다: cargo(build·test·fmt·clippy·run·install·deny·fetch·tree·check), rustup, gh(repo·pr·label·release·api·run·workflow·issue·secret list), git push. **npm/pnpm/npx 금지, 실제 키체인 쓰기(`security`, `--ignored` 키체인 테스트) 금지.**
- 저장소: `LeeSwallow/KeyHarbor`(public). Homebrew tap: `LeeSwallow/homebrew-tap`. 웹 주소: `https://leeswallow.github.io/KeyHarbor/`.
- 기본 데이터 폴더: `$HOME/Library/Application Support/KeyHarbor`. Keychain 서비스: `io.github.leeswallow.keyharbor`. `--data-dir`·`KEYHARBOR_HOME` 덮어쓰기는 유지. [B3]
- 라이선스: `MIT OR Apache-2.0`, 저작권자 `LeeSwallow`, 2026.
- 릴리스 대상: `aarch64-apple-darwin`, `x86_64-apple-darwin`만. 서명·공증 없음. cask는 `xattr -dr com.apple.quarantine` postflight.
- 릴리스 빌드에 `insecure-test-keystore` 기능이 들어가면 안 된다.
- 커밋은 Conventional Commits, AI 공동 저자 trailer 금지(커밋 훅으로 제거).
- 문서 언어: 한국어(라벨 이름과 커밋 메시지는 영어). 웹 페이지는 한국어 한 장.
- 공개 저장소 생성·push·태그는 사용자가 승인했다(설계 §2). 그 외 원격 작업(다른 저장소 쓰기 등)은 하지 않는다.

## Review Focus

1. **태그 버전과 `Cargo.toml` 버전이 다름**(`v0.1.1` 태그, crate는 0.1.0): 릴리스 워크플로가 GoReleaser 전에 분명한 메시지로 실패해야 한다. → Task 6 `scripts/check-version.sh` 테스트.
2. **`HOMEBREW_TAP_TOKEN`이 없는 상태의 릴리스**: Release는 만들어지고 cask 업로드만 건너뛰어야 한다. → Task 6 `skip_upload` 템플릿, Task 10에서 secret 유무를 확인한 뒤 태그.
3. **기본 데이터 폴더가 이미 0755로 존재**(다른 도구가 만든 `KeyHarbor` 폴더): 덮어쓰지 않고 "private directory" 오류로 멈춰야 한다. → Task 1 `default_dir_with_open_permissions_is_rejected`.
4. **사람 공동 저자 trailer**: 훅이 AI trailer만 지우고 사람의 `Co-authored-by`는 남겨야 한다. → Task 4 `scripts/test-scripts.sh`.
5. **Pages 하위 경로(`/KeyHarbor/`)에서 깨지는 링크**: 사이트의 모든 내부 링크와 자원은 상대 경로여야 한다. → Task 7 `scripts/check-site.sh`.

---

## File Structure

```text
(root)
  AGENTS.md  CLAUDE.md  CONTRIBUTING.md  VISION.md  ROADMAP.md  CHANGELOG.md
  SECURITY.md  SUPPORT.md  CODE_OF_CONDUCT.md  LICENSE-MIT  LICENSE-APACHE
  README.md                  다시 씀 (제품 기준)
  .gitattributes  .goreleaser.yaml  .gitignore(수정)
  Cargo.toml                 수정: license 필드
  docs/prototype.md          생성: 기존 README의 프로토타입 설명 이동
  .githooks/commit-msg
  scripts/ai-trailers.txt  check-commit-trailers.sh  check-pr.sh  issue.sh  pr.sh
          sync-labels.sh  repo-setup.sh  check-version.sh  licenses.sh  check-docs.sh
          check-site.sh  test-scripts.sh
  .github/CODEOWNERS  dependabot.yml  release.yml  labels.txt  pull_request_template.md
  .github/ISSUE_TEMPLATE/bug.yml  feature.yml  proposal.yml  config.yml
  .github/workflows/ci.yml  pr.yml  release.yml  pages.yml
  rebuild/deny.toml  rebuild/about.toml  rebuild/about.hbs
  rebuild/crates/cli/src/main.rs           수정: 기본 데이터 폴더
  rebuild/crates/cli/tests/vault_cli.rs    수정: 기본 폴더 테스트
  rebuild/crates/vault/src/keystore.rs     수정: KEYCHAIN_SERVICE
  site/index.html  site/style.css  site/icon.svg
  .sticky/                                  stickypane init --no-agent-docs
```

---

### Task 1: 데이터 폴더와 Keychain 이름 확정

**Files:**
- Modify: `rebuild/crates/cli/src/main.rs`, `rebuild/crates/vault/src/keystore.rs` (`KEYCHAIN_SERVICE`), `rebuild/crates/cli/tests/vault_cli.rs`
- Modify (문서): `rebuild/README.md`, `docs/design/02-system-architecture.md` §2.10, `docs/design/03-security-whitepaper.md` §3.4, `docs/design/07-cryptography-design.md` §7.5, `docs/design/adr/0003-keychain-vault-key.md`, `docs/design/06-requirements-log.md`(J-33)

**Interfaces:**
- Produces: `fn default_data_dir(home: &Path) -> PathBuf` in `main.rs` = `home.join("Library/Application Support/KeyHarbor")`; `pub const KEYCHAIN_SERVICE: &str = "io.github.leeswallow.keyharbor";`

- [ ] **Step 1: 실패하는 테스트 작성** (`vault_cli.rs`에 helper `run_home(home: &Path, args, stdin)` 추가: `--data-dir` 없이 `HOME=home`, `KEYHARBOR_HOME` 제거, `KEYHARBOR_TEST_KEYSTORE_DIR` 설정)

```rust
#[test] fn default_data_dir_is_application_support() {
    // HOME=tmp 에서 `vault init` 성공 → tmp/Library/Application Support/KeyHarbor/keyharbor.sqlite3 존재, 권한 0o700
    // tmp/.keyharbor-rebuild 는 생기지 않음
}
#[test] fn default_dir_with_open_permissions_is_rejected() {
    // tmp/Library/Application Support/KeyHarbor 를 0o755로 미리 만듦 → `vault init` 실패, stderr에 "private directory"
}
#[test] fn keyharbor_home_still_overrides() { /* KEYHARBOR_HOME=tmp/x → tmp/x/keyharbor.sqlite3 */ }
```
그리고 `keystore.rs` 테스트: `assert_eq!(KEYCHAIN_SERVICE, "io.github.leeswallow.keyharbor");`

- [ ] **Step 2: 실패 확인** — Run: `cd rebuild && cargo test -p keyharbor --test vault_cli default_ && cargo test -p keyharbor-vault keystore` — Expected: FAIL
- [ ] **Step 3: 구현** — `main.rs`의 `None` 분기를 `default_data_dir(&home)`로. `SqliteStore::open`이 이미 재귀 생성·0700·권한 검사를 하므로 추가 로직은 없다.
- [ ] **Step 4: 통과 확인** — Run: `cd rebuild && cargo test --workspace` — Expected: PASS (기존 62개 포함)
- [ ] **Step 5: 문서 갱신** — 위 Files의 문서에서 `~/.keyharbor-rebuild`·`dev.keyharbor.vault`를 새 값으로 바꾸고 J-33(B3) 추가. 확인: `grep -rn "keyharbor-rebuild\|dev.keyharbor.vault" rebuild docs README.md` 결과가 `docs/superpowers/` 기록 외에는 없음.
- [ ] **Step 6: 커밋** — `git commit -m "feat(cli)!: store data in Application Support and name the keychain service io.github.leeswallow.keyharbor"`

---

### Task 2: README 재작성과 프로토타입 설명 이동

**Files:**
- Create: `docs/prototype.md` (현재 `README.md` 전체를 옮기고 맨 위에 "이 문서는 루트 crate 프로토타입 설명이다. 릴리스 제품은 README.md" 한 줄)
- Modify: `README.md` (새로 씀)

새 `README.md` 구성(한국어): 아이콘(`docs/design/assets/app-icon.svg`, width 96) + 한 줄 소개 / 상태(v0.1.0: 로컬 암호화 금고 CLI, macOS) / 설치(`brew install --cask leeswallow/tap/keyharbor`, 또는 Releases의 tar.gz) / 빠른 시작(`keyharbor vault init`, `add … --stdin`, `list`, `export`) / 알아 둘 것(rebuild/README의 "알아 둘 것" 요약: 평문 출력 없음, 분실 책임, 키체인 확인 창, 데이터 폴더) / 로드맵 표(1–6단계, ROADMAP.md 링크) / 문서 링크(docs/design, SECURITY, CONTRIBUTING) / 라이선스(MIT OR Apache-2.0).

- [ ] **Step 1:** `docs/prototype.md` 생성(내용 이동), `README.md` 재작성.
- [ ] **Step 2: 확인** — Task 3의 `scripts/check-docs.sh`가 생기기 전이므로 여기서는 `grep -o '](\([^)#]*\))' README.md`로 뽑은 상대 링크가 모두 존재하는지 수동 확인. Task 3 Step 4에서 자동 검사로 다시 확인된다.
- [ ] **Step 3: 커밋** — `git commit -m "docs: rewrite the README for the v0.1.0 vault CLI and move the prototype notes"`

---

### Task 3: 협업 문서와 라이선스

**Files:**
- Create: `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`, `VISION.md`, `ROADMAP.md`, `CHANGELOG.md`, `SECURITY.md`, `SUPPORT.md`, `CODE_OF_CONDUCT.md`, `LICENSE-MIT`, `LICENSE-APACHE`, `.gitattributes`, `scripts/check-docs.sh`
- Modify: `Cargo.toml` (`license = "MIT OR Apache-2.0"`)

내용 기준은 설계 §4.1 표. 고정값:
- `CLAUDE.md` 전체: `@AGENTS.md`
- `AGENTS.md` "Before you finish" 명령: `cd rebuild && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --locked`
- `AGENTS.md` 규칙에 반드시: CLI·로그·오류에 평문 비밀 금지 / 암호 알고리즘 직접 구현 금지(docs/design/07) / 동작이 바뀌면 docs/design과 ADR 갱신 / Conventional Commits, AI 공동 저자 금지, `git config core.hooksPath .githooks` / push·main 병합·태그·브랜치 삭제 전에 확인 / 이 레포에서 moa 스킬 사용 금지.
- `CHANGELOG.md`: Keep a Changelog 1.1.0, `## [Unreleased]`와 `## [0.1.0] - YYYY-MM-DD`(날짜는 Task 10에서 확정, 그 전까지 `[Unreleased]` 아래에 0.1.0 내용을 둔다).
- `LICENSE-APACHE`: Apache License 2.0 원문(`https://www.apache.org/licenses/LICENSE-2.0.txt` 그대로). `LICENSE-MIT`: MIT 원문, `Copyright (c) 2026 LeeSwallow`.
- `.gitattributes`: `* text=auto eol=lf`
- `scripts/check-docs.sh`: 루트 `*.md`, `docs/**/*.md`, `.github/**/*.md`의 상대 링크 `](path)`(http·#·mailto 제외, `#anchor` 앞부분만)를 파일 기준 경로로 풀어 존재하지 않으면 `file: broken link → path`를 출력하고 실패.

- [ ] **Step 1:** `scripts/check-docs.sh` 작성 후 실행 — Expected: FAIL(README가 CONTRIBUTING.md·ROADMAP.md 등을 가리키는데 아직 없음)
- [ ] **Step 2:** 문서·라이선스 파일 작성, `Cargo.toml` license 추가.
- [ ] **Step 3: 확인** — Run: `sh scripts/check-docs.sh && cargo check --locked` — Expected: `all links resolve`, 빌드 성공
- [ ] **Step 4: 커밋** — `git commit -m "docs: add contribution, security and governance documents and dual MIT/Apache license"`

---

### Task 4: 커밋 훅과 흐름 스크립트

**Files:**
- Create: `.githooks/commit-msg`, `scripts/ai-trailers.txt`, `scripts/check-commit-trailers.sh`, `scripts/check-pr.sh`, `scripts/issue.sh`, `scripts/pr.sh`, `scripts/sync-labels.sh`, `scripts/repo-setup.sh`, `scripts/test-scripts.sh`

stickypane의 같은 이름 파일을 복사하고 바꾼다: 기본 저장소 `LeeSwallow/KeyHarbor`; `repo-setup.sh`의 설명 "Local vault and browser-session foundation for AI agents", 토픽 `rust cli macos password-manager secrets ai-agents mcp claude-code codex keychain`, 그리고 두 가지를 추가한다: Pages 활성화 `gh api -X POST repos/$repo/pages -f build_type=workflow`(이미 있으면 무시), main 보호 `gh api -X PUT repos/$repo/branches/main/protection`(필수 체크 `test (macos-latest)`, `test (ubuntu-latest)`, `enforce_admins=false`, 리뷰 요구 없음). `check-pr.sh`의 종류 라벨 목록은 Task 5 labels.txt와 일치.

- [ ] **Step 1: 실패하는 테스트 작성** — `scripts/test-scripts.sh`: 임시 파일로
  1. `Co-Authored-By: Claude <noreply@anthropic.com>` + `Co-authored-by: Kim <kim@example.com>` 메시지 → 훅 후 Claude 줄 없음, Kim 줄 있음
  2. `🤖 Generated with [Claude Code]` 줄 제거
  3. `check-pr.sh`: `PR_TITLE="feat: x" PR_BODY="Closes #1" PR_LABELS=feature` → 0, 제목 `"update stuff"` → 1, 라벨 없음 → 1, `no-issue` 라벨이면 본문 없어도 0
  4. 브랜치 이름 `fix/12-x` 흉내: `GIT_DIR` 임시 저장소에서 훅이 `Refs: #12` 추가
- [ ] **Step 2: 실패 확인** — Run: `sh scripts/test-scripts.sh` — Expected: FAIL(스크립트 없음)
- [ ] **Step 3:** 파일 작성, `chmod +x`, `git config core.hooksPath .githooks`
- [ ] **Step 4: 통과 확인** — Run: `sh scripts/test-scripts.sh` — Expected: `all script tests passed`
- [ ] **Step 5: 커밋** — `git commit -m "chore: add commit-msg hook, PR checks and issue/PR helper scripts"`

---

### Task 5: GitHub 템플릿·라벨·CI·cargo-deny

**Files:**
- Create: `.github/CODEOWNERS`(`* @LeeSwallow`), `.github/dependabot.yml`(cargo `/`·`/rebuild`, github-actions, weekly), `.github/release.yml`(stickypane과 같은 범주), `.github/labels.txt`, `.github/pull_request_template.md`, `.github/ISSUE_TEMPLATE/{bug,feature,proposal,config}.yml`, `.github/workflows/ci.yml`, `.github/workflows/pr.yml`, `rebuild/deny.toml`

- `labels.txt`: stickypane 목록에서 `translation` 제거, 추가 `security|b60205|A security property or report`, `area: vault|1d76db|`, `area: session|1d76db|`, `area: mcp|1d76db|`, `area: app|1d76db|`, `area: site|1d76db|`.
- `bug.yml` 필수 입력: `keyharbor --version`, macOS 버전, 사용한 에이전트(Claude Code/Codex/기타), 무엇을 했나·기대·실제. 비밀 값을 붙여 넣지 말라는 안내 문구.
- PR 템플릿 체크리스트: Conventional Commit 제목 / 종류 라벨 1개 / `cd rebuild && cargo fmt --all --check && cargo clippy … && cargo test --workspace` 통과 / 평문 비밀이 출력·로그·DB에 나가지 않음(누출 테스트) / 동작 변경 시 README·docs/design·ADR 갱신.
- `ci.yml` jobs:
  - `test` matrix `[macos-latest, ubuntu-latest]`: `dtolnay/rust-toolchain@stable` (components rustfmt, clippy), `working-directory: rebuild`: fmt check, clippy `-D warnings`, `cargo test --workspace --locked`.
  - `prototype` (macos-latest): 루트에서 `cargo test --locked`.
  - `deny` (ubuntu-latest): `EmbarkStudios/cargo-deny-action@v2` with `manifest-path: rebuild/Cargo.toml`.
  - `release-dry-run` (macos-latest, PR과 main push): rustup targets 두 개 추가 → `goreleaser/goreleaser-action@v6` `args: release --snapshot --clean` → `cargo tree -p keyharbor -e features --manifest-path rebuild/Cargo.toml | grep -q insecure-test-keystore && exit 1 || true`.
  - `commits` (PR만): `sh scripts/check-commit-trailers.sh "origin/${{ github.base_ref }}..HEAD"`.
- `pr.yml`: stickypane과 같다.
- `deny.toml`: `[licenses] allow = ["MIT","Apache-2.0","Apache-2.0 WITH LLVM-exception","BSD-2-Clause","BSD-3-Clause","ISC","Unicode-3.0","Zlib","MPL-2.0","CC0-1.0"]`, `[advisories] version = 2`, `[bans] multiple-versions = "warn"`, `[sources] unknown-registry = "deny"`, `unknown-git = "deny"`.

- [ ] **Step 1:** `cargo install cargo-deny --locked` 후 `cd rebuild && cargo deny check` — 결과를 본다(첫 실행이 곧 기준선). 실패 항목은 설계 B5 규칙대로 처리: 버전만 올려 해결되면 `cargo update -p <crate>`, 아니면 `deny.toml`에 사유와 함께 예외를 적고 최종 보고에 포함.
- [ ] **Step 2:** 파일 작성.
- [ ] **Step 3: 확인** — Run: `ruby -ryaml -e 'ARGV.each{|f| YAML.load_file(f)}; puts "yaml ok"' .github/*.yml .github/workflows/*.yml .github/ISSUE_TEMPLATE/*.yml && cd rebuild && cargo deny check` — Expected: `yaml ok`, deny 통과
- [ ] **Step 4: 커밋** — `git commit -m "ci: add test, lint, cargo-deny and release dry-run workflows with issue and PR templates"`

---

### Task 6: 릴리스 파이프라인

**Files:**
- Create: `.goreleaser.yaml`, `.github/workflows/release.yml`, `scripts/check-version.sh`, `scripts/licenses.sh`, `rebuild/about.toml`, `rebuild/about.hbs`
- Modify: `scripts/test-scripts.sh` (check-version 테스트 추가)

- `.goreleaser.yaml`(version 2, `project_name: keyharbor`):
  - `before.hooks`: `sh scripts/licenses.sh` (→ `build/THIRD_PARTY_LICENSES.html`)
  - `builds`: `- builder: rust`, `binary: keyharbor`, `dir: rebuild`, `tool: cargo`, `command: build`, `flags: [--release, --locked, -p=keyharbor]`, `targets: [aarch64-apple-darwin, x86_64-apple-darwin]`
  - `archives`: tar.gz, files `LICENSE-MIT`, `LICENSE-APACHE`, `README.md`, `CHANGELOG.md`, `{src: build/THIRD_PARTY_LICENSES.html, strip_parent: true}`
  - `checksum: {name_template: checksums.txt}`, `changelog: {use: github-native}`, `release: {prerelease: auto}`
  - `homebrew_casks`: stickypane 블록을 그대로 쓰고 이름·homepage(`https://leeswallow.github.io/KeyHarbor/`)·description("Local encrypted vault and browser-session foundation for AI agents")·`license: "MIT OR Apache-2.0"`·xattr 대상 `keyharbor`만 바꾼다. `skip_upload` 템플릿 동일.
  - 파일 끝 주석: 서명·공증 자리(`APPLE_*` secret이 생기면 `notarize` 섹션 추가).
- `scripts/check-version.sh <tag>`: `cargo metadata --manifest-path rebuild/Cargo.toml --no-deps --format-version 1`에서 `keyharbor` 패키지 버전을 읽어 `v$version`과 다르면 `tag vX does not match crate version Y` 출력 후 exit 1.
- `scripts/licenses.sh`: `cargo about generate --manifest-path rebuild/Cargo.toml rebuild/about.hbs -o build/THIRD_PARTY_LICENSES.html`. `about.toml`의 `accepted`는 deny.toml 허용 목록과 같게.
- `release.yml`: `on: push: tags: ["v*"]`, `permissions: contents: write`, macos-latest: checkout(fetch-depth 0) → rust stable + 두 target → `sh scripts/check-version.sh "$GITHUB_REF_NAME"` → rebuild에서 fmt·clippy·test → `cargo install cargo-about --locked` → goreleaser-action v6 `release --clean`, env `GITHUB_TOKEN`, `HOMEBREW_TAP_TOKEN`.
- ci.yml의 `release-dry-run`에도 `cargo install cargo-about --locked` 단계를 넣는다.

- [ ] **Step 1: 실패하는 테스트** — `test-scripts.sh`에 추가: `check-version.sh v0.1.0` → 0, `check-version.sh v9.9.9` → 1이고 출력에 `does not match`. Run: `sh scripts/test-scripts.sh` — Expected: FAIL
- [ ] **Step 2:** 파일 작성. `cargo install cargo-about --locked`
- [ ] **Step 3: 확인** — Run: `sh scripts/test-scripts.sh && sh scripts/licenses.sh && test -s build/THIRD_PARTY_LICENSES.html && ruby -ryaml -e 'YAML.load_file(".goreleaser.yaml"); puts "ok"'` — Expected: 모두 통과. (`goreleaser check`·snapshot은 로컬에 goreleaser가 없으므로 Task 9의 CI `release-dry-run`으로 검증한다.)
- [ ] **Step 4:** `.gitignore`에 `/build/`, `/dist/` 추가.
- [ ] **Step 5: 커밋** — `git commit -m "ci: release macOS binaries with GoReleaser and a Homebrew cask on v* tags"`

---

### Task 7: 웹 페이지

**Files:**
- Create: `site/index.html`, `site/style.css`, `site/icon.svg`(`docs/design/assets/app-icon.svg` 복사), `.github/workflows/pages.yml`, `scripts/check-site.sh`

- 구성은 설계 §4.4 그대로: 헤더(아이콘·이름·"에이전트가 로그인에서 멈추지 않게. 비밀번호는 내 Mac에.") / 설치(brew 명령 + 복사 버튼은 JS 없이 `<code>`만, Releases 링크 `https://github.com/LeeSwallow/KeyHarbor/releases/latest`) / 두 터미널 블록 "지금 되는 것"(vault init·add --stdin·list·export)·"예정 — 1·3단계"(에이전트 블로그 발행, ROADMAP 링크) / 세 약속(§4.4 문구) / 푸터 링크(GitHub, docs/design, SECURITY.md, 라이선스).
- 색: `--accent:#05A8A8`, 다크 모드 `@media (prefers-color-scheme: dark)`, 모바일 360px에서 가로 스크롤 없음. 외부 스크립트·폰트 없음.
- `pages.yml`: `on: push: branches:[main], paths:["site/**"]` + `workflow_dispatch`, permissions `pages: write, id-token: write, contents: read`, `actions/configure-pages@v5` → `actions/upload-pages-artifact@v3 (path: site)` → `actions/deploy-pages@v4`.
- `scripts/check-site.sh`: `site/*.html`에서 `href=`/`src=` 값 중 `http`로 시작하지 않는 것이 `/`로 시작하면 실패(하위 경로 깨짐), 상대 파일이 없으면 실패.

- [ ] **Step 1:** `check-site.sh` 작성 후 실행 — Expected: FAIL(site 없음)
- [ ] **Step 2:** 사이트 파일 작성.
- [ ] **Step 3: 확인** — Run: `sh scripts/check-site.sh` — Expected: `site ok`. 그리고 headless Chrome으로 데스크톱(1280)·모바일(390) 스크린샷: `"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless --screenshot=<scratch>/site-1280.png --window-size=1280,1600 file://$PWD/site/index.html` (390×1800도). 이미지를 보고 잘림·가로 스크롤이 없는지 확인.
- [ ] **Step 4: 커밋** — `git commit -m "feat(site): add the download page and its GitHub Pages workflow"`

---

### Task 8: 작업 보드

**Files:** Create: `.sticky/` (stickypane이 생성), `.sticky/로드맵.md`(checklist)

- [ ] **Step 1:** `stickypane init --no-agent-docs` → `.sticky/` 생성 확인.
- [ ] **Step 2:** `stickypane todo 로드맵 add "<항목>"`으로 1–6단계와 v0.1.0 체크리스트를 넣고, 2단계는 체크한다(`stickypane todo 로드맵 done …` 또는 해당 명령 — `stickypane kinds checklist`로 확인).
- [ ] **Step 3: 커밋** — `git commit -m "chore: add a stickypane board with the roadmap"`

---

### Task 9: 공개 저장소 생성과 CI 확인

**Files:** 없음(원격 작업). 모든 Task 1–8 커밋이 `main`에 있어야 한다(작업 브랜치 `chore/collaboration-release`를 ff 병합).

- [ ] **Step 1: 선행 조건** — `gh auth status`에 `workflow` 권한이 보여야 한다. 없으면 멈추고 사용자에게 별도 터미널에서 `gh auth refresh -h github.com -s workflow`를 요청한다.
- [ ] **Step 2:** `gh repo create LeeSwallow/KeyHarbor --public --source . --remote origin --push --description "Local vault and browser-session foundation for AI agents"`
- [ ] **Step 3:** `sh scripts/repo-setup.sh LeeSwallow/KeyHarbor` (라벨·Discussions·보안 신고·토픽·Pages·main 보호)
- [ ] **Step 4:** 사용자에게 별도 터미널에서 `gh secret set HOMEBREW_TAP_TOKEN -R LeeSwallow/KeyHarbor` 요청. 확인: `gh secret list -R LeeSwallow/KeyHarbor`에 이름이 보임. (없으면 Task 10은 cask 없이 진행하고 보고한다 — Review Focus 2.)
- [ ] **Step 5: CI 확인** — `gh run list -R LeeSwallow/KeyHarbor --branch main --limit 5`, 실패 시 `gh run view <id> --log-failed`로 원인을 보고 수정 커밋(PR 없이 main에 직접, 관리자 예외) → 다시 확인. Expected: `ci`, `pages` 성공, `release-dry-run` 성공(snapshot 산출물 2개).
- [ ] **Step 6:** Pages 확인: `curl -s -o /dev/null -w '%{http_code}' https://leeswallow.github.io/KeyHarbor/` → `200`.

---

### Task 10: v0.1.0 릴리스

- [ ] **Step 1:** `CHANGELOG.md`의 0.1.0을 `## [0.1.0] - <오늘 날짜>`로 확정하고 비교 링크 추가. 커밋 `docs: changelog for 0.1.0`, push, CI 성공 확인.
- [ ] **Step 2:** `git tag -a v0.1.0 -m "v0.1.0"` → `git push origin v0.1.0`
- [ ] **Step 3:** `gh run watch` (release 워크플로) — Expected: 성공.
- [ ] **Step 4: 확인** — `gh release view v0.1.0 -R LeeSwallow/KeyHarbor --json assets --jq '.assets[].name'` → darwin arm64·amd64 tar.gz와 `checksums.txt`. secret이 있었다면 `gh api repos/LeeSwallow/homebrew-tap/contents/Casks/keyharbor.rb --jq .name` → `keyharbor.rb`.
- [ ] **Step 5:** 웹 페이지 설치 링크가 실제 릴리스를 가리키는지 확인하고, `docs/design/05` 로드맵과 `ROADMAP.md`에 "v0.1.0 배포됨"을 기록·커밋·push.
