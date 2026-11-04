//! The checked path every vault value takes into a page (03 §3.8, J-35, I4): settle, find the
//! focused element and its frame's origin, run the guard's checks, re-check focus and
//! navigation, then type. The order of these steps is the protection; do not reorder them.

use super::GuardedBrowser;
use crate::{
    guard::LockReason,
    pep::substitute::{Destination, Prepared},
};
use anyhow::{Context, Result, anyhow, bail};
use keyharbor_domain::{
    audit::{AuditEvent, AuditKind},
    denial::DenialReason,
    ports::{
        BrowserPort, FrameInspector, PageInput,
        browser::{Element, FillOutcome, Focus, Page, Target},
    },
    vault::{Domain, ItemName, Origin, has_placeholder, parse_placeholders},
};

/// The refusal after page code moved the focus while a value was being typed.
const FOCUS_MOVED: &str = "substitution locked for this session: the focus moved while typing";

/// The first item `template` names: refusals are recorded against it.
fn first_item(template: &str) -> Result<ItemName> {
    let refs = parse_placeholders(template).map_err(|e| anyhow!("{e}"))?;
    Ok(refs.into_iter().next().context("no placeholder")?.item().clone())
}

impl<B: BrowserPort + PageInput + FrameInspector> GuardedBrowser<'_, B> {
    /// Records (and returns) the refusal of a value whose element no longer has the focus.
    fn deny_focus(&self, item: &ItemName, origin: Option<&Origin>) -> anyhow::Error {
        self.guard.denied(
            self.agent,
            item,
            origin.map(Origin::as_str),
            DenialReason::Focus,
        )
    }

    /// The focused element, what it is and its frame's origin, refusing (and recording) when
    /// the focus cannot be located or, for `fill`, is not on the target element.
    pub(crate) async fn checked_focus(
        &mut self,
        page: &Page,
        template: &str,
        expected: Option<u64>,
    ) -> Result<(Focus, Element, Option<Origin>)> {
        let first = first_item(template)?;
        let focus = match self.inner.focus(page).await {
            Ok(focus) => focus,
            Err(_) => {
                return Err(self
                    .guard
                    .denied(self.agent, &first, None, DenialReason::Frame));
            }
        };
        let origin = self.inner.frame_origin(page, &focus.frame).await;
        if expected.is_some_and(|node| node != focus.node) {
            return Err(self.deny_focus(&first, origin.as_ref()));
        }
        let element = match self.inner.element(page, focus.node).await {
            Ok(element) => element,
            Err(_) => return Err(self.deny_focus(&first, origin.as_ref())),
        };
        Ok((focus, element, origin))
    }

    /// Types resolved text, then re-checks the frame's origin (detection only: the text is
    /// already in) and records the use.
    async fn insert_prepared(
        &mut self,
        page: &Page,
        focus: &Focus,
        origin: Option<Origin>,
        prepared: Prepared,
    ) -> Result<FillOutcome> {
        // From here on the element may hold a value: snapshots redact it for the session.
        self.guard.taint(focus.node);
        let inserted = self.inner.insert_text(page, &prepared.text).await;
        drop(prepared.text);
        inserted?;
        // The focus must still be where the text was checked to go; if page code moved it,
        // the text may have gone elsewhere, so nothing more is substituted in this session.
        let moved_focus = self.inner.focus(page).await.ok().as_ref() != Some(focus);
        let after = self.inner.frame_origin(page, &focus.frame).await;
        let changed = after != origin;
        self.note_used(
            (&prepared.items, &prepared.domains, &prepared.codes),
            origin.as_ref(),
            after.as_ref(),
        );
        if moved_focus {
            self.guard.lock(self.agent, LockReason::FocusMoved);
            bail!(FOCUS_MOVED);
        }
        Ok(FillOutcome {
            origin_changed: changed,
        })
    }

    /// Records that `prepared`'s values went into a frame of `origin` (and, if the frame is
    /// now at `after`, that it moved). An item whose one-time code went in is recorded as
    /// `otp_used`, never with the code.
    pub(crate) fn note_used(
        &self,
        (items, domains, codes): (&[ItemName], &[Domain], &[ItemName]),
        origin: Option<&Origin>,
        after: Option<&Origin>,
    ) {
        let session = self.guard.session();
        for (item, domain) in items.iter().zip(domains) {
            let kind = if codes.contains(item) {
                AuditKind::OtpUsed
            } else {
                AuditKind::SecretUsed
            };
            let used = AuditEvent::new(self.agent, kind, "ok")
                .session(session)
                .item(item.as_str())
                .origin(origin.map(Origin::as_str));
            self.guard.record(&used);
            if let Some(origin) = origin {
                self.guard.note_use(self.agent, item, *domain, origin);
            }
            if after != origin {
                let moved = AuditEvent::new(
                    self.agent,
                    AuditKind::OriginChangedAfterFill,
                    after.map_or("unknown", Origin::as_str),
                )
                .session(session)
                .item(item.as_str())
                .origin(origin.map(Origin::as_str));
                self.guard.record(&moved);
            }
        }
    }

    /// Waits for navigations to finish before checking anything, so the checks see the
    /// document the text will go into (I4). Returns the page's navigation count then.
    pub(crate) async fn settled(&mut self, page: &Page, template: &str) -> Result<u64> {
        if self.inner.settle(page, None).await.unwrap_or(false) {
            return Ok(self.inner.navigation_epoch(page).await);
        }
        let first = first_item(template)?;
        Err(self
            .guard
            .denied(self.agent, &first, None, DenialReason::Navigating))
    }

    /// Refuses when a navigation started (or finished) in the page since `epoch`, or is still
    /// under way in the frame: the checked document may be gone (I4).
    pub(crate) async fn still_there(
        &mut self,
        page: &Page,
        frame: &str,
        epoch: u64,
        prepared: &Prepared,
        origin: Option<&Origin>,
    ) -> Result<()> {
        let quiet = self.inner.settle(page, Some(frame)).await.unwrap_or(false);
        if quiet && self.inner.navigation_epoch(page).await == epoch {
            return Ok(());
        }
        Err(self.guard.denied(
            self.agent,
            &prepared.items[0],
            origin.map(Origin::as_str),
            DenialReason::Navigating,
        ))
    }

    pub(crate) async fn fill_checked(
        &mut self,
        target: &Target,
        template: &str,
    ) -> Result<FillOutcome> {
        if !has_placeholder(template) {
            return self.inner.fill(target, template).await;
        }
        let page = self.inner.page().await?;
        let epoch = self.settled(&page, template).await?;
        let node = self.inner.focus_element(&page, target).await?;
        let (focus, element, origin) = self.checked_focus(&page, template, Some(node)).await?;
        let prepared = self.guard.prepare_text(
            self.agent,
            template,
            Destination {
                frame: origin.as_ref(),
                element: Some(&element),
            },
        )?;
        self.inner.clear_element(&page, node).await?;
        // Clearing ran page code; the text must still go where it was checked.
        if self.inner.focus(&page).await.ok().as_ref() != Some(&focus) {
            return Err(self.deny_focus(&prepared.items[0], origin.as_ref()));
        }
        self.still_there(&page, &focus.frame, epoch, &prepared, origin.as_ref())
            .await?;
        self.insert_prepared(&page, &focus, origin, prepared).await
    }

    pub(crate) async fn type_checked(&mut self, template: &str) -> Result<FillOutcome> {
        if !has_placeholder(template) {
            return self.inner.type_text(template).await;
        }
        let page = self.inner.page().await?;
        let epoch = self.settled(&page, template).await?;
        let (focus, element, origin) = self.checked_focus(&page, template, None).await?;
        let prepared = self.guard.prepare_text(
            self.agent,
            template,
            Destination {
                frame: origin.as_ref(),
                element: Some(&element),
            },
        )?;
        self.still_there(&page, &focus.frame, epoch, &prepared, origin.as_ref())
            .await?;
        self.insert_prepared(&page, &focus, origin, prepared).await
    }
}
