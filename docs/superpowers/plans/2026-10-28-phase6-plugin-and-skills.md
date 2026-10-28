# 6단계 Claude Code·Codex 플러그인과 스킬 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans. Steps use `- [ ]`.

**Goal:** 이 저장소를 Claude Code·Codex 플러그인으로 만들어, 설치하면 KeyHarbor MCP 서버가 등록되고 에이전트가 로그인·가입·`.env`·인계 흐름을 스킬로 배우게 한다. 구조는 stickypane(`../stickypane`: `.claude-plugin/`, `.codex-plugin/`, `.agents/plugins/marketplace.json`, `skills/`, `commands/`, `hooks/`, `scripts/selfcheck.sh`, `docs/plugin.md`)과 heymoa-harness(`../../HeyMoa/heymoa-harness`: skills/<name>/references/, rules/)를 따른다.

## Global Constraints
- 플러그인 이름 `keyharbor`, 마켓플레이스 이름 `keyharbor`, 저자 LeeSwallow, 라이선스 MIT OR Apache-2.0, brandColor `#05A8A8`.
- 스킬은 `skills/<name>/SKILL.md`: front matter `name`(디렉터리와 같음), `description`은 "Use when"으로 시작. 첫 화면에 실행할 것(MCP 도구 이름 또는 CLI 명령), 그다음 규칙. 80줄 이하. **실제로 존재하는 도구·명령만** 이름을 쓴다(검사 스크립트가 확인).
- 스킬에 비밀 값 예시를 쓰지 않는다. placeholder만.
- MCP 등록: 플러그인 루트 `.mcp.json` → `{"mcpServers":{"keyharbor":{"command":"keyharbor","args":["mcp","--agent","claude-code"]}}}`. Codex는 `commands/setup.md`가 `codex mcp add keyharbor -- keyharbor mcp --agent codex`를 안내한다.
- SessionStart 훅 스크립트는 `keyharbor`가 없으면 아무것도 출력하지 않고(컨텍스트 비용 0), 있으면 한 줄로 금고 초기화 여부·실행 중 세션 수·확인 대기 수만 출력한다(값·항목 이름 없음).

## Skills (최소 세트)
| 이름 | 언제 | 핵심 |
|---|---|---|
| `using-keyharbor` | 로그인·비밀·브라우저 세션이 필요할 때의 입구 | 도구 지도, placeholder 규칙, 다른 스킬 안내 |
| `logging-in-to-a-site` | 사이트 로그인이 필요할 때 | site_state_get → session_open → browser_navigate/snapshot → browser_fill(placeholder) → browser_click → site_state_set; CAPTCHA·2FA면 handing-off |
| `signing-up-for-a-site` | 새 계정이 필요할 때 | vault_generate → 폼 채우기 → 약관 등 승인 필요 시 handing-off → logged_in 기록(자동 확정) |
| `using-env-secrets` | 셸 명령에 API 키·`.env`가 필요할 때 | `keyharbor run --env …`, `.env.keyharbor` 템플릿, 평문 `.env`를 만들지 않기 |
| `handing-off-to-the-user` | CAPTCHA·약관·2FA·결제 확인 | session_wait_auth → 사용자에게 할 일 설명 → session_wait_for_resume |
| `asking-for-a-secret` | 금고에 없는 카드·주소·기존 비밀번호 | vault_request → vault_wait; 이미 받은 값은 vault_put |
- 공통 규칙 `rules/secrets.md`: 값을 출력·요약·기록하지 않는다, placeholder만 쓴다, 거부되면 우회하지 않는다, CAPTCHA를 풀려 하지 않는다.

## Commands
- `commands/setup.md` (`/keyharbor:setup`): `keyharbor --version` 확인 → 없으면 설치 안내(소스 빌드) → `keyharbor vault list`로 초기화 확인 → 없으면 사용자에게 `keyharbor vault init` 안내(키체인 확인 창).
- `commands/status.md` (`/keyharbor:status`): `keyharbor list`, `keyharbor vault requests`, `keyharbor audit --limit 10` 요약.

## Tasks
1. 매니페스트: `.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`, `.codex-plugin/plugin.json`(interface 포함, stickypane 형식), `.agents/plugins/marketplace.json`, `.mcp.json`, `hooks/claude-hooks.json`, `hooks/codex-hooks.json`, `scripts/session-start.sh`. 검사: `scripts/selfcheck-plugin.sh`(stickypane selfcheck 확장: JSON 파싱, sh -n, 스킬 front matter, 명령 front matter) — 먼저 쓰고 실패 확인 → 커밋 `feat(plugin): manifests, MCP registration and session hook`
2. CLI에 숨은 `keyharbor mcp --list-tools`(도구 이름을 한 줄씩) 추가 + 테스트. `scripts/check-skill-names.sh`: 스킬·명령에서 백틱 안의 `snake_case` 도구 이름과 `keyharbor <cmd>`를 뽑아 `--list-tools`와 `keyharbor --help` 하위 명령에 있는지 확인 → 커밋 `chore: check that skills name only real tools`
3. 스킬 6개와 `rules/secrets.md`, `commands/` 2개 → selfcheck·check-skill-names 통과 → 커밋 `feat(plugin): skills for login, sign-up, env secrets and handoff`
4. 실제 연결 확인: 임시 `CLAUDE_CONFIG_DIR`/HOME 없이 `claude mcp list` 같은 사용자 설정 변경은 하지 않는다. 대신 `keyharbor mcp` stdio에 initialize→tools/list를 보내 `.mcp.json`의 명령이 동작하는지 테스트(`tests/plugin_mcp.rs`). → 커밋 `test(plugin): the MCP command in .mcp.json starts and lists tools`
5. `docs/plugin.md`(stickypane 형식), README 설치 절에 `/plugin marketplace add LeeSwallow/KeyHarbor` → `/plugin install keyharbor@keyharbor`, Codex 안내, CI에 selfcheck·check-skill-names 추가, 05·ROADMAP·.sticky 갱신 → 커밋 `docs: plugin usage`
