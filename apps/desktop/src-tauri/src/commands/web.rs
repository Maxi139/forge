//! Vorschau von Webseiten. Die Oberfläche legt die fertige Seite ab, das Vorschaufenster lädt sie unter der Adresse
//! `forgeweb://localhost/`. Die Adresse hat eine eigene Sicherheitsrichtlinie: Die Seite darf Skripte und Stile
//! enthalten, aber nichts nachladen und nichts ins Netz senden. Sonst könnte jedes Skript im Projekt (auch eines,
//! das die KI geschrieben hat) beim Anzeigen Daten verschicken.

use crate::state::AppState;
use std::borrow::Cow;
use tauri::http::{header, Request, Response};
use tauri::{Manager, State, UriSchemeContext};

pub const PREVIEW_SCHEME: &str = "forgeweb";

/// Skripte und Stile der Seite selbst, Bilder nur als `data:`. Keine Verbindungen, keine fremden Rahmen.
const PREVIEW_CSP: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; form-action 'self'; base-uri 'none'; object-src 'none'";

const OTHER_PAGE: &str = "<!doctype html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><body style=\"font:14px/1.5 system-ui,sans-serif;margin:2rem;color:#555\"><p><strong>Das gibt es in der Vorschau nicht.</strong></p><p>Die Vorschau zeigt nur die eine Seite. Anfragen an das Gerät und Verweise auf andere Seiten führen hier ins Leere.</p>";

/// Legt die Seite ab und gibt die Adresse zurück, unter der das Vorschaufenster sie lädt.
#[tauri::command]
pub fn web_preview_set(state: State<AppState>, html: String) -> Result<String, String> {
    let mut slot = state
        .web_preview
        .lock()
        .map_err(|_| "Interner Fehler: Zustand gesperrt.".to_string())?;
    slot.0 += 1;
    slot.1 = html;
    Ok(format!(
        "{PREVIEW_SCHEME}://localhost/index.html?v={}",
        slot.0
    ))
}

/// Antwortet auf Anfragen an `forgeweb://`. Nur `/` und `/index.html` liefern die Seite.
pub fn serve<R: tauri::Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
) -> Response<Cow<'static, [u8]>> {
    let path = request.uri().path();
    let body = if path == "/" || path == "/index.html" {
        ctx.app_handle()
            .state::<AppState>()
            .web_preview
            .lock()
            .map(|g| g.1.clone())
            .unwrap_or_default()
    } else {
        OTHER_PAGE.to_string()
    };
    page(body)
}

fn page(body: String) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_SECURITY_POLICY, PREVIEW_CSP)
        .header(header::CACHE_CONTROL, "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(Cow::Owned(body.into_bytes()))
        .expect("statische Kopfzeilen")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_cannot_reach_the_network() {
        let r = page("<p>x</p>".into());
        let csp = r.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap();
        assert!(csp.contains("connect-src 'none'") && csp.contains("default-src 'none'"));
        assert!(!csp.contains("http"));
        assert_eq!(r.body().as_ref(), b"<p>x</p>");
    }
}
