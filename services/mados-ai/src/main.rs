//! mados-ai entry point.
//!
//!   mados-ai serve        serve org.mados.Assistant1 on the session bus
//!   mados-ai ask TEXT…    one-shot request; read-only answers are executed,
//!                         state changes are only described (never executed
//!                         from the CLI without the confirmation UI)

use mados_ai::assistant::Assistant;
use mados_ai::ops::LiveOps;
use mados_ai::provider::{self, Config};
use mados_ai::service::{render, AssistantService};
use mados_core::{log_error, log_info, log_warn, names};
use std::sync::Arc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("serve") => match serve() {
            Ok(()) => 0,
            Err(e) => {
                log_error!("fatal: {e}");
                1
            }
        },
        Some("ask") if args.len() > 1 => ask(&args[1..].join(" ")),
        _ => {
            eprintln!("usage: mados-ai serve | mados-ai ask TEXT...");
            2
        }
    };
    std::process::exit(code);
}

async fn build() -> Assistant<LiveOps> {
    let cfg = Config::default_path().map(|p| Config::load(&p)).unwrap_or_default();
    let (provider, warning) = provider::from_config(&cfg);
    if let Some(w) = warning {
        log_warn!("{w}");
    }
    Assistant::new(provider, LiveOps::new().await)
}

fn ask(text: &str) -> i32 {
    zbus::block_on(async {
        let a = build().await;
        let r = a.ask("cli", text).await;
        println!("{}", render(&r));
        if r.request_id.is_some() {
            println!("(Confirm this in the assistant UI; the CLI does not perform state changes.)");
        }
        0
    })
}

fn serve() -> zbus::Result<()> {
    zbus::block_on(async {
        let inner = Arc::new(build().await);
        let provider = inner.provider_name().to_string();
        let _conn = zbus::connection::Builder::session()?
            .name(names::ASSISTANT_BUS_NAME)?
            .serve_at(names::ASSISTANT_OBJECT_PATH, AssistantService { inner })?
            .build()
            .await?;
        log_info!("serving {} (provider: {provider})", names::ASSISTANT_BUS_NAME);
        std::future::pending::<()>().await;
        Ok(())
    })
}
