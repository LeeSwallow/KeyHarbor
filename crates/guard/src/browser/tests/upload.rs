//! `browser_upload`: the person's rule, the file input and the grants decide.

use super::*;

fn upload_fake() -> Fake {
    Fake {
        element: Element {
            local_name: "input".into(),
            input_type: Some("file".into()),
            ..Element::default()
        },
        ..Fake::default()
    }
}
fn resume() -> (UploadPolicy, Vec<UploadFile>) {
    let policy = UploadPolicy {
        dirs: vec!["/r".into()],
        max_files: 10,
    };
    let file = UploadFile {
        asked: "/r/resume.pdf".into(),
        resolved: "/r/resume.pdf".into(),
        is_file: true,
    };
    (policy, vec![file])
}

#[test]
fn upload_records_file_names_only() {
    let e = env();
    init(&e);
    let (guard, probe) = guard(&e);
    let mut fake = upload_fake();
    let (policy, files) = resume();
    let out = block(
        guard
            .browser(&mut fake, "claude")
            .upload(&Target::Ref(7), &policy, &files),
    )
    .unwrap();
    assert_eq!(out, json!({"uploaded": ["resume.pdf"]}));
    assert_eq!(
        fake.uploaded,
        vec![std::path::PathBuf::from("/r/resume.pdf")]
    );
    assert_eq!(
        probe.kinds(),
        vec![(AuditKind::FileUploaded, "resume.pdf".into())]
    );
}

#[test]
fn upload_needs_a_file_input() {
    let e = env();
    init(&e);
    let (guard, _) = guard(&e);
    let mut fake = Fake::default();
    let (policy, files) = resume();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .upload(&Target::Ref(7), &policy, &files),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "browser_upload needs an <input type=file>");
    assert!(fake.uploaded.is_empty());
}

#[test]
fn upload_outside_grant_origin_is_refused() {
    let e = env();
    init(&e);
    let (guard, _) = guard(&e);
    let grant = keyharbor_domain::grant::Grant {
        id: keyharbor_domain::grant::GrantId::from_bytes([9; 16]),
        session: guard.session(),
        origin: Origin::parse("https://jasoseol.com").unwrap(),
        until: None,
        created_by: "cli".into(),
        reason: None,
        created_at: 0,
        revoked_at: None,
        cleared: false,
    };
    SqliteStore::open(&e.data)
        .unwrap()
        .grant_add(&grant)
        .unwrap();
    let mut fake = upload_fake();
    let (policy, files) = resume();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .upload(&Target::Ref(7), &policy, &files),
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "denied: upload (out-of-grant)");
    assert!(fake.uploaded.is_empty());
}

#[test]
fn upload_follows_the_persons_rule() {
    let e = env();
    init(&e);
    let (guard, _) = guard(&e);
    let mut fake = upload_fake();
    let (mut policy, files) = resume();
    policy.dirs.clear();
    let err = block(
        guard
            .browser(&mut fake, "claude")
            .upload(&Target::Ref(7), &policy, &files),
    )
    .unwrap_err();
    assert!(
        err.to_string().starts_with("upload_dirs is not set"),
        "{err}"
    );
    assert!(fake.uploaded.is_empty());
}
