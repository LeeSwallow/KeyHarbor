//! `browser_upload` (#118, 03 §3.14): puts local files into a page's file input. A file
//! leaving this Mac is a value leaving the vault's protection, so the person's rule decides
//! (`upload_dirs`, `upload_max_files`), the input must be a real `<input type=file>` that has
//! the focus, and its frame must be a site the session's grants allow. The usage record keeps
//! file names and the origin, never contents.

use super::GuardedBrowser;
use crate::lock_store;
use anyhow::{Result, anyhow, bail};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    grant::allows_document,
    ports::{BrowserPort, FrameInspector, PageInput, browser::Target},
    upload::{UploadFile, UploadPolicy},
    vault::Origin,
};
use serde_json::{Value, json};

/// The refusals of an upload. They name the reason only, never a file or a path.
const NOT_A_FILE_INPUT: &str = "browser_upload needs an <input type=file>";
const DENIED_FOCUS: &str = "denied: upload (focus)";
const DENIED_FRAME: &str = "denied: upload (frame)";
const DENIED_OUT_OF_GRANT: &str = "denied: upload (out-of-grant)";

impl<B: BrowserPort + PageInput + FrameInspector> GuardedBrowser<'_, B> {
    /// Puts `files` into the file input `target`, if `policy` and the page allow it.
    pub async fn upload(
        &mut self,
        target: &Target,
        policy: &UploadPolicy,
        files: &[UploadFile],
    ) -> Result<Value> {
        self.target(target)?;
        let mut literals: Vec<&str> = files.iter().map(|f| f.asked.as_str()).collect();
        literals.push(target.selector().unwrap_or_default());
        self.literals(&literals)?;
        let outcome = self.upload_checked(target, policy, files).await;
        self.guard.redact_outcome(outcome)
    }

    async fn upload_checked(
        &mut self,
        target: &Target,
        policy: &UploadPolicy,
        files: &[UploadFile],
    ) -> Result<Value> {
        policy.check(files).map_err(|e| anyhow!("{e}"))?;
        let page = self.inner.page().await?;
        let node = self.inner.focus_element(&page, target).await?;
        if !self.inner.element(&page, node).await?.is_file_input() {
            bail!(NOT_A_FILE_INPUT);
        }
        let focus = self.inner.focus(&page).await?;
        if focus.node != node {
            bail!(DENIED_FOCUS);
        }
        let Some(origin) = self.inner.frame_origin(&page, &focus.frame).await else {
            bail!(DENIED_FRAME);
        };
        if !self.in_grants(&origin)? {
            bail!(DENIED_OUT_OF_GRANT);
        }
        let paths: Vec<_> = files.iter().map(|f| f.resolved.clone()).collect();
        self.inner.set_files(&page, node, &paths).await?;
        let names: Vec<String> = files.iter().map(UploadFile::name).collect();
        let event = AuditEvent::new(self.agent, AuditKind::FileUploaded, &names.join(", "))
            .session(self.guard.session())
            .origin(Some(origin.as_str()));
        self.guard.record(&event);
        Ok(json!({"uploaded": names}))
    }

    /// Whether the session's grants allow a document at `origin` (no grants: every site).
    fn in_grants(&self, origin: &Origin) -> Result<bool> {
        let store = lock_store(self.guard.store())?;
        let grants = store.grants_for(self.guard.session())?;
        Ok(allows_document(
            &grants,
            self.guard.unix_seconds(),
            &format!("{}/", origin.as_str()),
        ))
    }
}
