# ADR 목록

> 양식: Michael Nygard, [*Documenting Architecture Decisions*](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions) — 상태 · 맥락 · 결정 · 결과

| # | 결정 | 상태 |
|---|---|---|
| [0001](0001-local-foundation-not-flows.md) | 로컬 기반 도구로 정의하고 흐름은 제공하지 않는다 | Proposed |
| [0002](0002-own-local-vault.md) | 외부 금고 대신 앱 소유 로컬 금고를 둔다 | Proposed |
| [0003](0003-keychain-vault-key.md) | 금고 키는 Keychain에 두고 생체 조건 없이 쓴다 | Proposed (키 계층 부분은 0011이 대체) |
| [0004](0004-supervisor-owns-cdp.md) | 감독 프로세스가 CDP를 소유하고 에이전트에는 도구만 준다 | Proposed |
| [0005](0005-user-chosen-auth-default-none.md) | 관리 앱 인증은 사용자 선택, 기본값은 없음 | Proposed |
| [0006](0006-tauri-admin-app.md) | 관리 앱은 Tauri로 만든다 | Proposed |
| [0007](0007-excluded-capabilities.md) | 넣지 않는 기능 | Proposed |
| [0008](0008-build-on-rebuild-workspace.md) | `rebuild/` 워크스페이스 위에 구현한다 | Proposed |
| [0009](0009-own-chrome-backend.md) | 브라우저 백엔드는 직접 띄우는 Chrome | Proposed |
| [0010](0010-env-run-injection.md) | `.env` 변수는 `keyharbor run`으로 자식 프로세스에만 주입한다 | Proposed |
| [0011](0011-domain-split.md) | 세션 · 자격 증명 · 결제 · 개인정보의 네 도메인으로 나눈다 | Proposed |
| [0012](0012-crypto-scheme.md) | 암호화 방식은 선행 사례의 합의를 따르고, 표준 형식이 있으면 재사용한다 | Proposed |
| [0013](0013-registration-draft-model.md) | 미등록 정보는 네 경로로 받고 draft로 먼저 저장한다 | Proposed |
| [0014](0014-hexagonal-guard-layer.md) | 헥사고날 구조로 나누고, 평문은 보안 계층(guard) 하나만 다룬다 | Accepted |
