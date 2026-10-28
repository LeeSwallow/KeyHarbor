# 2단계 로컬 금고 구현 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `rebuild/` 워크스페이스에 세 도메인(자격 증명·결제·개인정보) 로컬 암호화 금고와 그 CLI를 만든다. 평문은 CLI로 출력하지 않는다.

**Architecture:** `keyharbor-domain`에 금고 도메인 타입(종류·도메인·이름·origin·placeholder)을 더한다. 새 crate `keyharbor-vault`에 암호(HKDF 키 분리, XChaCha20-Poly1305 레코드, 메타데이터 MAC, 키 저장소)를 둔다. `keyharbor-sqlite`에 금고 테이블을 더하고, 새 crate `keyharbor-core`가 이들을 묶어 유스케이스(init·add·edit·rm·list·rotate·export·import)를 제공한다. CLI는 `keyharbor-core`만 호출한다.

**Tech Stack:** Rust 2024, rusqlite 0.37 (bundled), chacha20poly1305 0.11, hkdf 0.13, hmac 0.13, sha2 0.11, zeroize 1.9, age 0.12, security-framework 3.7 (macOS), url 2.5, rpassword 7.5, serde/serde_json 1, clap 4, tempfile 3.

**Spec:** `docs/design/07-cryptography-design.md`가 정본이다. 함께 읽을 것: `docs/design/02-system-architecture.md` §2.7(데이터 모델·도메인·필드 스키마), `docs/design/03-security-whitepaper.md` §3.4·§3.12, `docs/design/05-roadmap-and-verification.md` 2단계·§5.3, ADR 0003·0011·0012.

## Global Constraints

- 툴체인이 PATH에 없다. 모든 명령 앞에 `export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"`를 실행한다. 작업 디렉터리는 `rebuild/`다.
- 암호 알고리즘을 직접 구현하지 않는다. 위 crate만 쓴다.
- **CLI는 평문 비밀을 stdout·stderr에 쓰지 않는다** (03 §3.12 A안). `show`·`reveal`·평문 `export` 명령은 만들지 않는다.
- 비밀 값은 **명령행 인자로 받지 않는다** (`ps`에 보임). 터미널에서는 에코 없는 프롬프트(rpassword), 아니면 stdin JSON으로 받는다.
- 비밀을 담는 타입은 `Debug`·`Display`를 구현하지 않거나 `[REDACTED]`만 출력한다. 키·평문은 `zeroize::Zeroizing`으로 감싼다.
- 레코드 상수: `version = 0x01`, `suite = 0x01`(XChaCha20-Poly1305), nonce 24바이트(OS CSPRNG), key_id 16바이트(UUID bytes).
- HKDF-SHA256: `salt = vault_id(16B)`, `info = b"keyharbor/kek/v1"`(KEK), `b"keyharbor/meta-mac/v1"`(MK). 출력은 각각 32바이트.
- AD 도메인 태그: 필드 `b"keyharbor/field/v1"`, 도메인 키 `b"keyharbor/dk/v1"`, 메타 MAC `b"keyharbor/meta/v1"`. 각 태그 뒤에 `0x00`을 붙인다. 가변 길이 값은 `u16 big-endian 길이 ‖ 바이트`로 이어 붙인다.
- 도메인 바이트: `credentials = 1`, `payment = 2`, `personal = 3`.
- Keychain: `service = "dev.keyharbor.vault"`, `account = vault_id`(하이픈 있는 UUID 문자열). 파일 기반 login keychain, 생체 조건 없음. **이미 있으면 덮어쓰지 않는다. 없으면 새로 만들지 않는다.**
- SQLite: `PRAGMA secure_delete = ON`. 테이블에 값 평문 컬럼을 두지 않는다.
- 필드 값 최대 64 KiB, 항목 이름 1–64자.
- 데이터 경로 기본값은 기존대로 `~/.keyharbor-rebuild`. 프로토타입 데이터는 이전하지 않는다.
- 새 crate의 `Cargo.toml`은 기존 crate처럼 `version.workspace`·`edition.workspace`·`license.workspace`를 쓰고, 공용 의존성은 루트 `[workspace.dependencies]`에 둔다.

## Review Focus

1. **비밀을 명령행 인자로 넘김** (`vault add github --password hunter2`): clap이 그 플래그를 모른다고 거부해야 하고, 값은 프롬프트나 stdin으로만 받는다. → Task 9 `add_rejects_secret_flags`.
2. **DB는 있는데 Keychain 키가 없음** (Keychain 항목 수동 삭제, 다른 Mac으로 데이터 폴더 복사): 새 키를 만들지 않고 "금고 키를 찾을 수 없음" 오류로 끝나야 하며 DB는 그대로여야 한다. → Task 6 `open_without_key_fails_and_keeps_db`.
3. **placeholder를 깨는 항목 이름** (`my.github`, `a}b`, 공백, 빈 문자열): 이름 검증에서 거부해야 한다. → Task 1 `item_names_reject_placeholder_syntax`.
4. **같은 이름을 두 프로세스가 동시에 추가**: 하나만 성공하고 다른 쪽은 "이미 있음" 오류, 부분 레코드는 남지 않아야 한다. → Task 6 `concurrent_add_same_name_one_wins`.
5. **`.env`의 따옴표·주석·`export`·중복 키**: 일반적인 dotenv 문법을 받아들이고 중복 키는 줄 번호와 함께 거부해야 한다. → Task 8 `parses_common_dotenv_forms`, `rejects_duplicate_keys`.

---

## File Structure

```text
rebuild/
  Cargo.toml                                  수정: members에 vault·core 추가, 공용 의존성
  crates/domain/src/lib.rs                    수정: pub mod vault; 기존 세션 타입은 그대로
  crates/domain/src/vault.rs                  생성: Domain, ItemKind, ItemName, FieldName, Origin, Placeholder
  crates/domain/Cargo.toml                    수정: url 추가
  crates/vault/Cargo.toml                     생성
  crates/vault/src/lib.rs                     생성: pub mod 선언, 오류 타입
  crates/vault/src/keys.rs                    생성: SecretKey, RootKey, KeySet(HKDF)
  crates/vault/src/record.rs                  생성: 필드·도메인 키 레코드 seal/open
  crates/vault/src/meta.rs                    생성: 메타데이터 MAC
  crates/vault/src/keystore.rs                생성: KeyStore trait, MacKeychain, FileKeyStore(시험용)
  crates/vault/tests/kat.rs                   생성: 알려진 답 테스트
  crates/sqlite/src/lib.rs                    수정: open()에서 vault 마이그레이션 호출
  crates/sqlite/src/vault_repo.rs             생성: 금고 테이블과 저장소 메서드
  crates/core/Cargo.toml                      생성
  crates/core/src/lib.rs                      생성: Vault 파사드
  crates/core/src/rotate.rs                   생성: rotate_root, rotate_domain
  crates/core/src/transfer.rs                 생성: age 내보내기·가져오기
  crates/core/src/dotenv.rs                   생성: .env 파서
  crates/cli/src/main.rs                      수정: vault 하위 명령
  crates/cli/src/vault_cmd.rs                 생성: vault 명령 처리, 입력 수집
  crates/cli/tests/vault_cli.rs               생성: 바이너리 통합·누출 테스트
  README.md                                   수정: 금고 명령과 한계
```

---

### Task 1: 금고 도메인 타입

**Files:**
- Create: `rebuild/crates/domain/src/vault.rs`
- Modify: `rebuild/crates/domain/src/lib.rs` (끝에 `pub mod vault;`), `rebuild/crates/domain/Cargo.toml` (`url = "2.5"`)

**Interfaces:**
- Produces:
  - `enum Domain { Credentials, Payment, Personal }` — `fn as_byte(self) -> u8` (1/2/3), `fn as_str(self) -> &'static str` ("credentials"/"payment"/"personal"), `fn parse(&str) -> Option<Domain>`
  - `enum ItemKind { Login, Secret, Env, Card, Identity, Address }` — `fn domain(self) -> Domain`, `fn as_str`, `fn parse`, `fn schema(self) -> Option<&'static [&'static str]>` (Env는 `None` = 자유 필드), `fn default_origin_binding(self) -> bool` (Login·Card만 true)
  - `struct ItemName(String)` — `fn parse(&str) -> Result<ItemName, InvalidValue>`, `fn as_str`
  - `struct FieldName(String)` — `fn parse(&str) -> Result<FieldName, InvalidValue>`
  - `struct Origin(String)` — `fn parse(&str) -> Result<Origin, InvalidValue>`, `fn as_str`
  - `struct PlaceholderRef { pub item: ItemName, pub field: FieldName }`, `fn parse_placeholders(text: &str) -> Result<Vec<PlaceholderRef>, InvalidValue>`
  - `struct InvalidValue(&'static str)` — `Display`, `Error`

스키마(02 §2.7.2, 모든 종류에 `note` 포함):

| 종류 | 필드 |
|---|---|
| Login | `username`, `password`, `totp_seed`, `note` |
| Secret | `value`, `note` |
| Env | 자유 (FieldName 규칙 + 대문자·숫자·`_`, 숫자로 시작 금지) |
| Card | `holder`, `number`, `exp_month`, `exp_year`, `cvc`, `billing_address`, `note` |
| Identity | `full_name`, `given_name`, `family_name`, `email`, `phone`, `birth_date`, `note` |
| Address | `recipient`, `line1`, `line2`, `city`, `region`, `postal_code`, `country`, `phone`, `note` |

- [ ] **Step 1: 실패하는 테스트 작성** (`vault.rs`의 `#[cfg(test)] mod tests`)

```rust
#[test]
fn kinds_map_to_domains() {
    assert_eq!(ItemKind::Login.domain(), Domain::Credentials);
    assert_eq!(ItemKind::Env.domain(), Domain::Credentials);
    assert_eq!(ItemKind::Card.domain(), Domain::Payment);
    assert_eq!(ItemKind::Address.domain(), Domain::Personal);
    assert_eq!(Domain::Payment.as_byte(), 2);
    assert!(ItemKind::Card.default_origin_binding());
    assert!(!ItemKind::Identity.default_origin_binding());
}
#[test]
fn item_names_reject_placeholder_syntax() {
    for bad in ["", " ", "my.github", "a}b", "a{b", "a b", "x\n", &"a".repeat(65)] {
        assert!(ItemName::parse(bad).is_err(), "{bad:?}");
    }
    for good in ["github", "my-visa", "회사_메일", "a".repeat(64).as_str()] {
        assert!(ItemName::parse(good).is_ok(), "{good:?}");
    }
}
#[test]
fn origins_are_normalized() {
    assert_eq!(Origin::parse("HTTPS://GitHub.com:443").unwrap().as_str(), "https://github.com");
    assert_eq!(Origin::parse("http://localhost:8080").unwrap().as_str(), "http://localhost:8080");
    assert_eq!(Origin::parse("https://한국.kr").unwrap().as_str(), "https://xn--3e0b707e.kr");
    for bad in ["github.com", "https://github.com/login", "https://u:p@github.com", "ftp://x.com", "http://example.com"] {
        assert!(Origin::parse(bad).is_err(), "{bad:?}");
    }
}
#[test]
fn placeholders_parse_and_reject_malformed() {
    let refs = parse_placeholders("id={{secret:github.username}} pw={{secret:github.password}}").unwrap();
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[1].item.as_str(), "github");
    assert_eq!(refs[1].field.as_str(), "password");
    assert!(parse_placeholders("plain text").unwrap().is_empty());
    for bad in ["{{secret:github}}", "{{secret:.x}}", "{{secret:a.b", "{{secret:a b.c}}"] {
        assert!(parse_placeholders(bad).is_err(), "{bad:?}");
    }
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test -p keyharbor-domain` — Expected: FAIL (`vault` 모듈 없음)

- [ ] **Step 3: 구현** — `ItemName`: 문자는 `char::is_alphanumeric`·`-`·`_`만, 1–64자. `FieldName`: ASCII 영숫자·`_`, 1–64자. `Origin::parse`: `url::Url`로 파싱해 path가 `/`이고 query·fragment·userinfo가 없을 때만 허용. scheme은 `https`, 또는 host가 `localhost`/`127.0.0.1`/`[::1]`인 `http`만 받는다(기존 프로토타입 `src/lib.rs` `validate_origin`과 같은 규칙). `url::Url::origin().ascii_serialization()`으로 정규화한다. `parse_placeholders`: `{{secret:`로 시작하는 구간마다 `}}`까지 잘라 `item.field`로 나누고, 어느 쪽이든 검증에 실패하면 전체를 `Err`로 돌려준다.

- [ ] **Step 4: 통과 확인** — Run: `cargo test -p keyharbor-domain` — Expected: PASS (기존 테스트 2개 포함)

- [ ] **Step 5: 커밋** — `git add rebuild/crates/domain && git commit -m "feat(domain): vault item kinds, domains, names, origins, placeholders"`

---

### Task 2: 키와 레코드 암호화 (`keyharbor-vault`)

**Files:**
- Create: `rebuild/crates/vault/Cargo.toml`, `src/lib.rs`, `src/keys.rs`, `src/record.rs`, `tests/kat.rs`
- Modify: `rebuild/Cargo.toml` (members에 `crates/vault`, 공용 의존성 추가)

**Interfaces:**
- Consumes: `keyharbor_domain::vault::Domain`
- Produces:
  - `struct SecretKey(Zeroizing<[u8; 32]>)` — `fn generate() -> SecretKey`, `fn from_bytes([u8;32]) -> SecretKey`, `fn expose(&self) -> &[u8; 32]`. `Debug`는 `SecretKey([REDACTED])`.
  - `struct KeyId([u8; 16])` — `fn new() -> KeyId`(UUID v4 bytes), `fn from_bytes`, `fn as_bytes`, `fn to_hex`
  - `struct VaultId([u8; 16])` — 같은 메서드, `fn to_uuid_string(&self) -> String`
  - `struct KeySet { kek: SecretKey, mac: SecretKey }` — `fn derive(root: &SecretKey, vault: &VaultId) -> KeySet`, 필드 접근자 `kek()`, `mac()`
  - `struct FieldAd<'a> { pub vault: &'a VaultId, pub domain: Domain, pub item: &'a [u8;16], pub field: &'a str }`
  - `fn seal_field(dk: &SecretKey, dk_id: &KeyId, ad: &FieldAd, plaintext: &[u8]) -> Vec<u8>`
  - `fn open_field(dk: &SecretKey, ad: &FieldAd, record: &[u8]) -> Result<Zeroizing<Vec<u8>>, VaultError>`
  - `fn record_key_id(record: &[u8]) -> Result<KeyId, VaultError>` — 복호화 없이 헤더에서 key_id 읽기
  - `fn wrap_domain_key(kek: &SecretKey, kek_id: &KeyId, vault: &VaultId, domain: Domain, dk_id: &KeyId, dk: &SecretKey) -> Vec<u8>`
  - `fn unwrap_domain_key(kek: &SecretKey, vault: &VaultId, domain: Domain, dk_id: &KeyId, record: &[u8]) -> Result<SecretKey, VaultError>`
  - `enum VaultError { Decrypt, Format(&'static str), KeyNotFound, KeyExists, Keystore(String), Tampered }` — `Decrypt`는 실패 이유를 구분하지 않는다(07 §3.6.2).
  - 테스트 전용 `#[doc(hidden)] pub fn seal_field_with_nonce(..., nonce: [u8;24]) -> Vec<u8>`

레코드 배치와 AD는 07 §7.6.1·§7.6.2를 그대로 따른다. 도메인 키 레코드 AD = `b"keyharbor/dk/v1" ‖ 0x00 ‖ vault_id ‖ domain_byte ‖ dk_id ‖ version ‖ suite ‖ kek_id`.

- [ ] **Step 1: 실패하는 테스트 작성** (`src/record.rs` 테스트 모듈)

```rust
fn ad<'a>(v: &'a VaultId, item: &'a [u8;16], field: &'a str, domain: Domain) -> FieldAd<'a> {
    FieldAd { vault: v, domain, item, field }
}
#[test]
fn field_roundtrip() {
    let (v, dk, id) = (VaultId::new(), SecretKey::generate(), KeyId::new());
    let item = [7u8; 16];
    let rec = seal_field(&dk, &id, &ad(&v, &item, "password", Domain::Credentials), b"hunter2");
    assert_eq!(rec.len(), 42 + 7 + 16);
    assert_eq!(&rec[..2], &[1, 1]);
    assert_eq!(record_key_id(&rec).unwrap(), id);
    assert_eq!(&*open_field(&dk, &ad(&v, &item, "password", Domain::Credentials), &rec).unwrap(), b"hunter2");
}
#[test]
fn every_ad_component_is_bound() {
    let (v, dk, id) = (VaultId::new(), SecretKey::generate(), KeyId::new());
    let item = [7u8; 16];
    let rec = seal_field(&dk, &id, &ad(&v, &item, "password", Domain::Credentials), b"x");
    let other_v = VaultId::new();
    let other_item = [8u8; 16];
    for wrong in [
        ad(&other_v, &item, "password", Domain::Credentials),
        ad(&v, &other_item, "password", Domain::Credentials),
        ad(&v, &item, "username", Domain::Credentials),
        ad(&v, &item, "password", Domain::Payment),
    ] {
        assert!(matches!(open_field(&dk, &wrong, &rec), Err(VaultError::Decrypt)));
    }
    let mut header_flipped = rec.clone();
    header_flipped[5] ^= 1; // key_id 바이트
    assert!(open_field(&dk, &ad(&v, &item, "password", Domain::Credentials), &header_flipped).is_err());
    assert!(open_field(&SecretKey::generate(), &ad(&v, &item, "password", Domain::Credentials), &rec).is_err());
}
#[test]
fn rejects_unknown_version_suite_and_short_records() {
    let (v, dk, id) = (VaultId::new(), SecretKey::generate(), KeyId::new());
    let item = [0u8; 16];
    let a = ad(&v, &item, "f", Domain::Personal);
    let mut rec = seal_field(&dk, &id, &a, b"x");
    rec[0] = 2;
    assert!(matches!(open_field(&dk, &a, &rec), Err(VaultError::Format(_))));
    assert!(matches!(open_field(&dk, &a, &rec[..41]), Err(VaultError::Format(_))));
}
#[test]
fn domain_key_wrap_roundtrip_and_binding() {
    let (v, kek, kek_id, dk, dk_id) = (VaultId::new(), SecretKey::generate(), KeyId::new(), SecretKey::generate(), KeyId::new());
    let w = wrap_domain_key(&kek, &kek_id, &v, Domain::Payment, &dk_id, &dk);
    assert_eq!(unwrap_domain_key(&kek, &v, Domain::Payment, &dk_id, &w).unwrap().expose(), dk.expose());
    assert!(unwrap_domain_key(&kek, &v, Domain::Personal, &dk_id, &w).is_err());
    assert!(unwrap_domain_key(&kek, &v, Domain::Payment, &KeyId::new(), &w).is_err());
}
#[test]
fn keyset_derivation_separates_keys_and_depends_on_vault() {
    let root = SecretKey::from_bytes([9; 32]);
    let a = KeySet::derive(&root, &VaultId::from_bytes([1; 16]));
    let b = KeySet::derive(&root, &VaultId::from_bytes([2; 16]));
    assert_ne!(a.kek().expose(), a.mac().expose());
    assert_ne!(a.kek().expose(), b.kek().expose());
    assert_eq!(format!("{:?}", root), "SecretKey([REDACTED])");
}
```

`tests/kat.rs`: `root = [0x11;32]`, `vault = [0x22;16]`의 KEK·MK를 hex로 고정하고, `dk = [0x33;32]`, `dk_id = [0x44;16]`, `item = [0x55;16]`, `field = "password"`, `nonce = [0x66;24]`, 평문 `"KH-KAT"`인 `seal_field_with_nonce` 출력 바이트를 hex로 고정한다. 기대값은 Step 3 구현 뒤 한 번 계산해 넣고, 이후 형식이 바뀌면 실패하게 둔다. 고정 hex 위에 "07 §7.6.1 v1 형식 KAT — 값을 바꾸면 기존 금고를 읽지 못한다" 주석을 단다.

- [ ] **Step 2: 실패 확인** — Run: `cargo test -p keyharbor-vault` — Expected: FAIL (crate 없음)
- [ ] **Step 3: 구현** — `chacha20poly1305::XChaCha20Poly1305`와 `aead::Payload { msg, aad }`, `hkdf::Hkdf::<sha2::Sha256>::new(Some(salt), ikm).expand(info, &mut out)`. nonce는 `getrandom`(또는 aead가 노출하는 OS RNG)으로 24바이트. AD는 Global Constraints의 길이 접두사 규칙으로 만든다.
- [ ] **Step 4: KAT 값 고정 후 통과 확인** — Run: `cargo test -p keyharbor-vault` — Expected: PASS
- [ ] **Step 5: 커밋** — `git commit -m "feat(vault): HKDF key separation and XChaCha20-Poly1305 field/domain-key records"`

---

### Task 3: 메타데이터 MAC

**Files:**
- Create: `rebuild/crates/vault/src/meta.rs`

**Interfaces:**
- Consumes: Task 1의 `Domain`, `ItemKind`, `ItemName`, `Origin`; Task 2의 `SecretKey`, `VaultId`
- Produces:
  - `struct ItemMeta<'a> { pub vault: &'a VaultId, pub item: &'a [u8;16], pub domain: Domain, pub kind: ItemKind, pub name: &'a ItemName, pub origin_binding: bool, pub origins: &'a [Origin], pub status: &'a str, pub updated_at: &'a str }`
  - `fn meta_mac(mk: &SecretKey, meta: &ItemMeta) -> [u8; 32]`
  - `fn verify_meta_mac(mk: &SecretKey, meta: &ItemMeta, mac: &[u8]) -> Result<(), VaultError>` — 불일치면 `VaultError::Tampered`

입력 인코딩은 07 §7.6.3에 `status`를 더한 것이다. origin은 바이트 순서로 정렬해 넣는다.

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
#[test]
fn mac_detects_origin_and_binding_changes_but_not_order() {
    // 공통 픽스처: origins = [github.com, gist.github.com], binding = true
    // 1) 같은 입력 → verify Ok
    // 2) origins 순서만 바꿈 → verify Ok
    // 3) origin 하나 추가(https://evil.example) → Err(Tampered)
    // 4) origin_binding false → Err(Tampered)
    // 5) name·kind·status·updated_at 각각 변경 → Err(Tampered)
    // 6) 다른 MK → Err(Tampered)
}
```

각 번호를 별도 `assert!`로 쓴다.

- [ ] **Step 2: 실패 확인** — Run: `cargo test -p keyharbor-vault meta` — Expected: FAIL
- [ ] **Step 3: 구현** — `hmac::Hmac<Sha256>`, 검증은 `Mac::verify_slice`(상수 시간).
- [ ] **Step 4: 통과 확인** — Run: `cargo test -p keyharbor-vault meta` — Expected: PASS
- [ ] **Step 5: 커밋** — `git commit -m "feat(vault): item metadata HMAC"`

---

### Task 4: 키 저장소

**Files:**
- Create: `rebuild/crates/vault/src/keystore.rs`

**Interfaces:**
- Consumes: `SecretKey`, `VaultId`, `VaultError`
- Produces:
  - `trait KeyStore { fn create(&self, account: &str, key: &SecretKey) -> Result<(), VaultError>; fn get(&self, account: &str) -> Result<SecretKey, VaultError>; fn replace(&self, account: &str, key: &SecretKey) -> Result<(), VaultError>; fn delete(&self, account: &str) -> Result<(), VaultError>; }`
    - `create`: 이미 있으면 `KeyExists`. `get`·`replace`·`delete`: 없으면 `KeyNotFound`.
  - `struct MacKeychain` (`#[cfg(target_os = "macos")]`) — `service = "dev.keyharbor.vault"`, `security_framework::passwords`의 generic password API
  - `struct FileKeyStore { dir: PathBuf }` — **시험 전용**. 파일 `<dir>/<account>.key`(0600, 32바이트). 모듈 문서 첫 줄: "INSECURE: tests only — stores the root key in plaintext."
  - `fn keystore_from_env() -> Box<dyn KeyStore>` — `KEYHARBOR_TEST_KEYSTORE_DIR`가 있으면 `FileKeyStore`, 없으면 `MacKeychain`. 이 환경 변수 이름은 README에 "시험 전용, 쓰지 말 것"으로 적는다.

- [ ] **Step 1: 실패하는 테스트 작성** — `FileKeyStore`로: create 후 get 일치, 두 번째 create는 `KeyExists`, 없는 account의 get은 `KeyNotFound`, replace 후 get이 새 키, delete 후 get은 `KeyNotFound`, 파일 권한 `0o600`. `MacKeychain`도 같은 시나리오를 `#[ignore]` 테스트로 둔다(account = 무작위 UUID, 마지막에 delete).
- [ ] **Step 2: 실패 확인** — Run: `cargo test -p keyharbor-vault keystore` — Expected: FAIL
- [ ] **Step 3: 구현**
- [ ] **Step 4: 통과 확인** — Run: `cargo test -p keyharbor-vault keystore` — Expected: PASS. 그리고 `cargo test -p keyharbor-vault keystore -- --ignored`를 한 번 실행해 Keychain 확인 창이 뜨는지, "항상 허용" 뒤 다시 실행하면 뜨지 않는지 **결과를 커밋 메시지 본문에 기록**한다(03 §3.4의 실험).
- [ ] **Step 5: 커밋** — `git commit -m "feat(vault): KeyStore trait with macOS Keychain and test-only file store"`

---

### Task 5: 금고 SQLite 저장소

**Files:**
- Create: `rebuild/crates/sqlite/src/vault_repo.rs`
- Modify: `rebuild/crates/sqlite/src/lib.rs` (`mod vault_repo; pub use vault_repo::*;`, `open()`에서 `PRAGMA secure_delete=ON`과 금고 테이블 생성)

**Interfaces:**
- Produces (모두 `impl SqliteStore`):
  - 행 타입 `VaultRow { vault_id: [u8;16], kek_id: [u8;16], version: i64 }`, `DomainKeyRow { domain: u8, dk_id: [u8;16], wrapped: Vec<u8> }`, `ItemRow { id: [u8;16], name: String, domain: u8, kind: String, origin_binding: bool, status: String, source: String, meta_mac: Vec<u8>, updated_at: String, origins: Vec<String> }`, `FieldRow { field: String, sealed: Vec<u8> }`
  - `fn vault_get(&self) -> Result<Option<VaultRow>>`
  - `fn vault_create(&self, vault: &VaultRow, dks: &[DomainKeyRow]) -> Result<()>` — 이미 있으면 오류
  - `fn domain_keys(&self) -> Result<Vec<DomainKeyRow>>`
  - `fn item_insert(&self, item: &ItemRow, fields: &[FieldRow]) -> Result<()>` — 이름 중복은 `anyhow` 오류 메시지 `"item already exists"`
  - `fn item_update(&self, item: &ItemRow, fields: &[FieldRow]) -> Result<()>` — 필드 전체 교체
  - `fn item_delete(&self, name: &str) -> Result<bool>`
  - `fn item_get(&self, name: &str) -> Result<Option<(ItemRow, Vec<FieldRow>)>>`
  - `fn item_list(&self) -> Result<Vec<ItemRow>>` (이름순)
  - `fn rekey_domain_keys(&self, new_kek_id: [u8;16], dks: &[DomainKeyRow], metas: &[([u8;16], Vec<u8>)]) -> Result<()>` — DK 감싸기·kek_id·모든 meta_mac을 **한 트랜잭션**에서 교체
  - `fn replace_domain(&self, dk: &DomainKeyRow, fields: &[([u8;16], FieldRow)]) -> Result<()>` — 한 트랜잭션
  - `fn vault_version(&self) -> Result<i64>`. 모든 쓰기 메서드는 같은 트랜잭션에서 `version += 1`

테이블 (02 §2.7):

```sql
CREATE TABLE IF NOT EXISTS vault (id BLOB PRIMARY KEY CHECK(length(id)=16), kek_id BLOB NOT NULL, version INTEGER NOT NULL DEFAULT 0, singleton INTEGER NOT NULL DEFAULT 1 UNIQUE CHECK(singleton=1));
CREATE TABLE IF NOT EXISTS domain_keys (domain INTEGER PRIMARY KEY CHECK(domain IN (1,2,3)), dk_id BLOB NOT NULL, wrapped BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS vault_items (id BLOB PRIMARY KEY, name TEXT NOT NULL UNIQUE, domain INTEGER NOT NULL, kind TEXT NOT NULL, origin_binding INTEGER NOT NULL, status TEXT NOT NULL CHECK(status IN ('draft','active','discarded')), source TEXT NOT NULL, meta_mac BLOB NOT NULL, updated_at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS vault_fields (item_id BLOB NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE, field TEXT NOT NULL, sealed BLOB NOT NULL, PRIMARY KEY(item_id, field));
CREATE TABLE IF NOT EXISTS item_origins (item_id BLOB NOT NULL REFERENCES vault_items(id) ON DELETE CASCADE, origin TEXT NOT NULL, PRIMARY KEY(item_id, origin));
```

`PRAGMA foreign_keys = ON`도 켠다.

- [ ] **Step 1: 실패하는 테스트 작성** — `item_roundtrip_and_cascade`(insert → get 일치 → delete → 필드·origin 행 0개), `duplicate_name_fails_without_partial_rows`, `writes_bump_version`(insert·update·delete마다 +1), `rekey_is_atomic`(잘못된 domain 값 4를 넣어 CHECK로 실패시키면 kek_id와 meta_mac이 바뀌지 않음).
- [ ] **Step 2: 실패 확인** — Run: `cargo test -p keyharbor-sqlite` — Expected: FAIL
- [ ] **Step 3: 구현** — 쓰기는 기존 `reserve`처럼 `TransactionBehavior::Immediate`.
- [ ] **Step 4: 통과 확인** — Run: `cargo test -p keyharbor-sqlite` — Expected: PASS (기존 테스트 포함)
- [ ] **Step 5: 커밋** — `git commit -m "feat(sqlite): vault tables with atomic rekey and version counter"`

---

### Task 6: 금고 파사드 (`keyharbor-core`)

**Files:**
- Create: `rebuild/crates/core/Cargo.toml`, `rebuild/crates/core/src/lib.rs`
- Modify: `rebuild/Cargo.toml` (members에 `crates/core`)

**Interfaces:**
- Consumes: Task 1–5 전부
- Produces:
  - `struct Vault { store: SqliteStore, keystore: Box<dyn KeyStore>, vault_id: VaultId }`
  - `fn Vault::init(store: SqliteStore, keystore: Box<dyn KeyStore>) -> Result<Vault>` — 금고 행이 이미 있으면 오류. RK 생성 → `keystore.create` → DK 3개 생성·감싸기 → `vault_create`. `keystore.create`가 `KeyExists`면 DB를 건드리지 않고 오류.
  - `fn Vault::open(store, keystore) -> Result<Vault>` — 금고 행이 없으면 `"vault not initialized"`, 키가 없으면 `"vault key not found; refusing to create a new one"`
  - `struct NewItem { pub name: ItemName, pub kind: ItemKind, pub fields: Vec<(FieldName, Zeroizing<String>)>, pub origins: Vec<Origin>, pub origin_binding: Option<bool>, pub status: ItemStatus, pub source: ItemSource }`
  - `enum ItemStatus { Draft, Active, Discarded }`, `enum ItemSource { User, Import, Generated, Captured, Requested, Agent }` — 문자열은 02 §2.7의 값
  - `struct ItemSummary { pub name, pub kind, pub domain, pub fields: Vec<String>, pub origins: Vec<String>, pub origin_binding, pub status, pub source, pub updated_at }` — **값 없음**
  - `fn add(&self, item: NewItem) -> Result<()>` — 스키마 검사(`ItemKind::schema`), 필드 값 64 KiB 초과 거부, `origin_binding` 생략 시 `kind.default_origin_binding()`
  - `fn edit(&self, name: &ItemName, change: ItemChange) -> Result<()>` — `ItemChange { set_fields, remove_fields, origins: Option<Vec<Origin>>, origin_binding: Option<bool>, status: Option<ItemStatus> }`
  - `fn remove(&self, name: &ItemName) -> Result<bool>`
  - `fn list(&self) -> Result<Vec<ItemSummary>>`
  - `fn resolve(&self, r: &PlaceholderRef) -> Result<Zeroizing<String>>` — 메타 MAC 검증 후 복호화. 이후 단계(치환)가 쓴다. **CLI는 이것을 호출하지 않는다.**
  - `fn item_origins_verified(&self, name: &ItemName) -> Result<(bool, Vec<Origin>)>` — 메타 MAC 검증 후 바인딩·origin 반환

- [ ] **Step 1: 실패하는 테스트 작성** (`FileKeyStore` + tempdir)

```rust
#[test] fn init_twice_fails() { /* 두 번째 init → Err, 첫 금고 그대로 open 가능 */ }
#[test] fn open_without_key_fails_and_keeps_db() {
    // init → keystore 파일 삭제 → open은 Err("vault key not found…")
    // DB 파일의 SHA-256이 open 전후로 같다
}
#[test] fn add_list_resolve_roundtrip() {
    // login github {username, password} + origin https://github.com
    // list: fields == ["password","username"], 값 문자열이 Debug 출력에 없음
    // resolve(github.password) == 원래 값
}
#[test] fn schema_and_size_are_enforced() {
    // Card에 "password" 필드 → Err; 64*1024+1 바이트 값 → Err; Env에 "lower_case" → Err
}
#[test] fn tampered_origins_block_resolve() {
    // add → SqliteStore로 item_origins에 https://evil.example 직접 INSERT → resolve Err(Tampered), item_origins_verified Err
}
#[test] fn concurrent_add_same_name_one_wins() {
    // 8 스레드가 각자 Vault::open 후 같은 이름 add → Ok 정확히 1개, list 길이 1
}
#[test] fn db_and_wal_never_contain_plaintext() {
    // 값 "KH-SYNTH-<uuid>"로 login·card·identity·env 항목 추가 후
    // keyharbor.sqlite3, -wal 파일 바이트에 그 문자열이 없다
}
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test -p keyharbor-core` — Expected: FAIL
- [ ] **Step 3: 구현** — 시각은 RFC 3339 UTC 문자열(외부 crate 없이 `std::time::SystemTime`에서 초 단위로 만들어도 된다). 메타 MAC은 쓰기마다 다시 계산한다.
- [ ] **Step 4: 통과 확인** — Run: `cargo test -p keyharbor-core` — Expected: PASS
- [ ] **Step 5: 커밋** — `git commit -m "feat(core): vault facade with schema checks, metadata MAC verification"`

---

### Task 7: 키 교체

**Files:**
- Create: `rebuild/crates/core/src/rotate.rs`

**Interfaces:**
- Consumes: `Vault`, `rekey_domain_keys`, `replace_domain`, `KeyStore`
- Produces:
  - `fn Vault::rotate_root(&self) -> Result<()>` — 07 §7.8 시퀀스: 새 RK를 `account#next`에 `create` → DB `rekey_domain_keys`(새 kek_id) → `replace(account, 새 RK)` → `delete(account#next)`
  - `fn Vault::rotate_domain(&self, domain: Domain) -> Result<()>` — 새 DK로 그 도메인 필드 전부 재암호화, `replace_domain` 한 트랜잭션
  - `fn Vault::open`이 복구를 맡는다: 금고 행의 kek_id로 DK를 풀 수 없으면 `account#next`의 키로 시도하고, 성공하면 `replace(account, next)` → `delete(next)`로 마무리한다.
  - 시험용 `#[doc(hidden)] pub fn rotate_root_with_failpoint(&self, stop_after: u8) -> Result<()>` — 1·2·3단계 뒤에 `Err("failpoint")`를 반환

- [ ] **Step 1: 실패하는 테스트 작성** — `rotate_root_keeps_values`, `rotate_domain_only_touches_that_domain`(payment 교체 후 credentials 필드의 sealed 바이트는 그대로, card 필드는 바뀜, 값은 모두 resolve 일치), `interrupted_root_rotation_recovers`(stop_after 1·2·3 각각 → 새 `Vault::open` 성공 → 모든 값 resolve 일치 → `#next` 키 없음).
- [ ] **Step 2: 실패 확인** — Run: `cargo test -p keyharbor-core rotate` — Expected: FAIL
- [ ] **Step 3: 구현**
- [ ] **Step 4: 통과 확인** — Run: `cargo test -p keyharbor-core rotate` — Expected: PASS
- [ ] **Step 5: 커밋** — `git commit -m "feat(core): crash-safe root and domain key rotation"`

---

### Task 8: `.env` 가져오기와 age 내보내기·가져오기

**Files:**
- Create: `rebuild/crates/core/src/dotenv.rs`, `rebuild/crates/core/src/transfer.rs`

**Interfaces:**
- Produces:
  - `fn parse_dotenv(text: &str) -> Result<Vec<(FieldName, Zeroizing<String>)>>` — `KEY=VALUE`, 앞의 `export `, `#` 주석 줄, 빈 줄, 큰따옴표(`\n`·`\"` 이스케이프)·작은따옴표(그대로) 값, 따옴표 없는 값의 뒤쪽 ` #주석` 제거. 중복 키는 `"duplicate key KEY on line N"`.
  - `fn Vault::import_env(&self, name: ItemName, text: &str) -> Result<usize>` — kind=Env, source=Import, 변수 개수 반환
  - `fn Vault::export_age(&self, passphrase: SecretString, out: &Path) -> Result<()>` — 평문 페이로드 JSON `{"format":"keyharbor-export/v1","exported_at":…,"items":[{name,kind,origins,origin_binding,status,source,fields:{name:value}}]}`. `age::scrypt::Recipient`. 대상 파일이 있으면 덮어쓰지 않는다. 권한 0600.
  - `fn Vault::import_age(&self, passphrase: SecretString, input: &Path) -> Result<ImportReport>` — `ImportReport { added: usize, skipped_existing: Vec<String> }`. 같은 이름은 건너뛰고 보고한다. 새 DK로 다시 암호화한다(add 경유).
  - `SecretString`은 `age::secrecy::SecretString`을 쓴다.

age 0.12의 정확한 API는 docs.rs `age/0.12.1`에서 `Encryptor::with_recipients`, `scrypt::Recipient::new`, `Decryptor::new`, `scrypt::Identity::new`를 확인하고 쓴다.

- [ ] **Step 1: 실패하는 테스트 작성**

```rust
#[test] fn parses_common_dotenv_forms() {
    let t = "# c\nexport A=1\nB=\"x\\ny\"\nC='raw $X'\nD=plain # tail\n\nE=\n";
    // A="1", B="x\ny", C="raw $X", D="plain", E=""
}
#[test] fn rejects_duplicate_keys() { /* "A=1\nA=2" → Err 메시지에 "line 2" */ }
#[test] fn rejects_invalid_names() { /* "a-b=1", "1A=1" → Err */ }
#[test] fn export_import_roundtrip_into_other_vault() {
    // 금고 A에 login·card·env 추가 → export → 금고 B(다른 tempdir·keystore)에 import → B에서 값 resolve 일치
}
#[test] fn export_is_age_and_contains_no_plaintext() {
    // 파일이 "age-encryption.org/v1\n"으로 시작, "KH-SYNTH-" 문자열 없음, age Decryptor + 같은 passphrase로 열림
}
#[test] fn wrong_passphrase_and_existing_target_fail() { /* 둘 다 Err, 기존 파일 내용 불변 */ }
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test -p keyharbor-core dotenv transfer` — Expected: FAIL
- [ ] **Step 3: 구현**
- [ ] **Step 4: 통과 확인** — Run: `cargo test -p keyharbor-core` — Expected: PASS
- [ ] **Step 5: 커밋** — `git commit -m "feat(core): dotenv import and age export/import"`

---

### Task 9: `keyharbor vault` CLI

**Files:**
- Create: `rebuild/crates/cli/src/vault_cmd.rs`, `rebuild/crates/cli/tests/vault_cli.rs`
- Modify: `rebuild/crates/cli/src/main.rs` (`Command::Vault(VaultCmd)` 추가), `rebuild/crates/cli/Cargo.toml` (`keyharbor-core`, `keyharbor-vault`, `rpassword`, `serde_json`; dev: `tempfile`)

**Interfaces:**
- Consumes: Task 6–8의 `Vault` 메서드, `keystore_from_env()`

명령 (모두 `--data-dir` 공용 옵션 유지):

| 명령 | 비밀 입력 | 출력 (stdout, 탭 구분) |
|---|---|---|
| `vault init` | — | `vault\t<vault_id>` |
| `vault add <name> --kind <kind> [--origin <o>]… [--no-origin-binding] [--draft] [--set <field>=<비밀 아닌 값>]… [--stdin]` | 스키마의 비밀 필드(`password`, `totp_seed`, `value`, `number`, `cvc`, 그리고 Env 전부)는 TTY면 에코 없는 프롬프트, `--stdin`이면 stdin JSON `{"field":"value"}` | `added\t<name>` |
| `vault edit <name> [--set …] [--unset <field>]… [--origin …] [--clear-origins] [--origin-binding on|off] [--status active|draft|discarded] [--stdin]` | add와 같다 | `updated\t<name>` |
| `vault rm <name>` | — | `removed\t<name>` |
| `vault list [--json]` | — | `name kind domain status origins fields` 한 줄씩. `--json`은 `ItemSummary` 배열 |
| `vault import-env <name> <path>` | 파일 | `imported\t<name>\t<n> variables` |
| `vault export <path>` | passphrase 두 번 (TTY) 또는 `KEYHARBOR_EXPORT_PASSPHRASE_FD=<fd>` | `exported\t<path>` |
| `vault import <path>` | passphrase | `imported\t<added>` + 건너뛴 이름은 stderr |
| `vault rotate-root` | — | `rotated\troot` |
| `vault rotate-domain <credentials|payment|personal>` | — | `rotated\t<domain>` |

`--set`은 비밀이 아닌 필드(예: `username`, `holder`, `exp_month`, `exp_year`, `full_name`, `email`, `phone`, `line1` …)에만 허용한다. 비밀 필드에 `--set`을 쓰면 `"field <f> is secret; use the prompt or --stdin"` 오류로 거부한다. 비밀 필드 목록은 `vault_cmd.rs`에 `const SECRET_FIELDS: &[&str]`로 둔다(Env는 전부 비밀). stdin JSON과 프롬프트로 받은 값은 즉시 `Zeroizing<String>`에 넣는다.

- [ ] **Step 1: 실패하는 테스트 작성** (`tests/vault_cli.rs`, `env!("CARGO_BIN_EXE_keyharbor")`, 각 테스트마다 tempdir data-dir + `KEYHARBOR_TEST_KEYSTORE_DIR`)

```rust
#[test] fn init_add_list_rm_flow() { /* init → add github --kind login --origin https://github.com --set username=me --stdin '{"password":"KH-SYNTH-…"}' → list에 github·login·credentials·active → rm → list 비어 있음 */ }
#[test] fn add_rejects_secret_flags() {
    // `--password x` → clap 오류(exit≠0); `--set password=x` → 위 오류 메시지; 두 경우 모두 stdout·stderr에 "x"를 포함한 값 echo 없음
}
#[test] fn no_command_prints_plaintext() {
    // 종류별 합성 비밀(login·secret·env·card·identity·address)을 넣은 뒤
    // init/add/edit/list/list --json/import-env/export/import/rotate-root/rotate-domain/rm 및
    // 실패 경로(중복 add, 잘못된 kind, 없는 이름 rm)의 stdout+stderr 전부에서
    // 각 합성 값과 그 percent-encoding·base64가 나타나지 않는다
}
#[test] fn there_is_no_show_command() { /* `vault show github` → 알 수 없는 하위 명령으로 실패 */ }
#[test] fn missing_key_is_reported_not_recreated() { /* keystore 디렉터리 비움 → `vault list` 실패, 메시지에 "vault key not found", keystore 디렉터리에 새 파일 없음 */ }
```

- [ ] **Step 2: 실패 확인** — Run: `cargo test -p keyharbor --test vault_cli` — Expected: FAIL
- [ ] **Step 3: 구현** — 입력 수집은 `vault_cmd.rs`의 `fn collect_secret_fields(kind: ItemKind, stdin: bool) -> Result<Vec<(FieldName, Zeroizing<String>)>>` 하나로 모은다. TTY가 아니고 `--stdin`도 없는데 비밀 필드가 필요하면 `"secret input requires a terminal or --stdin"`로 실패한다.
- [ ] **Step 4: 통과 확인** — Run: `cargo test --workspace` — Expected: PASS
- [ ] **Step 5: 커밋** — `git commit -m "feat(cli): keyharbor vault commands without plaintext output"`

---

### Task 10: 문서 정리와 최종 검증

**Files:**
- Modify: `rebuild/README.md`, `docs/design/05-roadmap-and-verification.md` (2단계 표에 "완료" 표시와 실제 테스트 이름), `docs/design/06-requirements-log.md` (Keychain 확인 창 실험 결과를 03 §3.4 [미검증] 대신 기록)

- [ ] **Step 1: README 갱신** — 금고 명령 표, "CLI는 평문을 출력하지 않는다 — 값 보기는 관리 앱(5단계)", "Mac·키를 잃으면 데이터를 잃는다, `vault export`로 age 백업", `KEYHARBOR_TEST_KEYSTORE_DIR`은 시험 전용이라는 경고, 키체인 확인 창 안내.
- [ ] **Step 2: 전체 검증** — Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` — Expected: 세 명령 모두 성공
- [ ] **Step 3: 실제 Keychain 수동 확인** — `KEYHARBOR_TEST_KEYSTORE_DIR` 없이 임시 `--data-dir`로 `vault init` → `vault add` → `vault list` → `vault rotate-root` → `vault list`. 확인 창 횟수를 기록한 뒤 `security delete-generic-password -s dev.keyharbor.vault -a <vault_id>`로 정리한다.
- [ ] **Step 4: 커밋** — `git commit -m "docs: phase 2 vault usage, limits, and verification results"`
