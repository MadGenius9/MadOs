//! `org.mados.Assistant1` D-Bus interface (session bus).

use crate::assistant::Assistant;
use crate::ops::LiveOps;
use mados_api::{AssistantReply, ReplyStatus};
use std::sync::Arc;
use zbus::message::Header;
use zbus::{fdo, interface};

pub struct AssistantService {
    pub inner: Arc<Assistant<LiveOps>>,
}

fn sender(hdr: &Header<'_>) -> fdo::Result<String> {
    hdr.sender()
        .map(|s| s.to_string())
        .ok_or_else(|| fdo::Error::AccessDenied("no sender".into()))
}

fn json(r: &AssistantReply) -> fdo::Result<String> {
    serde_json::to_string(r).map_err(|e| fdo::Error::Failed(e.to_string()))
}

#[interface(name = "org.mados.Assistant1")]
impl AssistantService {
    async fn ask(&self, text: String, #[zbus(header)] hdr: Header<'_>) -> fdo::Result<String> {
        json(&self.inner.ask(&sender(&hdr)?, &text).await)
    }

    async fn confirm(&self, request_id: String, #[zbus(header)] hdr: Header<'_>) -> fdo::Result<String> {
        json(&self.inner.confirm(&sender(&hdr)?, &request_id).await)
    }

    async fn cancel(&self, request_id: String, #[zbus(header)] hdr: Header<'_>) -> fdo::Result<()> {
        self.inner.cancel(&sender(&hdr)?, &request_id);
        Ok(())
    }

    #[zbus(property)]
    fn provider(&self) -> String {
        self.inner.provider_name().to_string()
    }

    /// True when requests are processed on this device only.
    #[zbus(property)]
    fn local(&self) -> bool {
        self.inner.provider_is_local()
    }
}

/// Human-readable rendering of a reply for CLI use.
pub fn render(r: &AssistantReply) -> String {
    let tag = match r.status {
        ReplyStatus::Done => "",
        ReplyStatus::NeedsConfirmation => "[confirm] ",
        ReplyStatus::Unsupported => "[unavailable] ",
        ReplyStatus::NotUnderstood => "[?] ",
        ReplyStatus::Denied => "[denied] ",
        ReplyStatus::Error => "[error] ",
        ReplyStatus::Cancelled => "[cancelled] ",
    };
    format!("{tag}{}", r.message)
}
