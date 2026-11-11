use crate::budget::Budget;
use crate::cache::Cache;
use crate::client::{AiError, ChatRequest, OpenRouter};
use crate::prompt::{
    fix_edits, fix_prompt, fix_system_for, parse_fix, parse_reply, system_for, to_diagnostics,
    user_prompt, Review, PROMPT_VERSION,
};
use crate::task::{
    parse_task, plan_changes, plan_created, task_prompt, SourceText, TaskOutcome, TASK_SYSTEM,
};
use crate::threads::{
    parse_thread_reply, thread_prompt, thread_system, Resolution, Role, Thread, ThreadStore,
    Verdict,
};
use forge_analysis::Block;
use forge_core::{Diagnostic, TextEdit};
use forge_secrets::Redactor;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

const PARALLEL: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockState {
    /// Die KI hat nichts gefunden.
    Ok,
    Issues,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockResult {
    pub file: String,
    pub name: String,
    pub start_line: u32,
    pub end_line: u32,
    pub state: BlockState,
    pub message: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub cached: bool,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub calls: u32,
    pub cached: u32,
    pub tokens: u64,
    /// Ein Fehler, der alle weiteren Aufrufe sinnlos macht (Schlüssel, Guthaben, Limit).
    pub stopped: Option<String>,
}

pub struct Paths {
    pub cache: PathBuf,
    pub budget: PathBuf,
    /// Gespräche über die Hinweise der KI.
    pub threads: PathBuf,
}

fn result(
    b: &Block,
    state: BlockState,
    message: Option<String>,
    diagnostics: Vec<Diagnostic>,
    cached: bool,
) -> BlockResult {
    BlockResult {
        file: b.file.clone(),
        name: b.name.clone(),
        start_line: b.start_line,
        end_line: b.end_line,
        state,
        message,
        diagnostics,
        cached,
    }
}

fn cache_key(b: &Block, model: &str, solved: &str) -> String {
    let mut h = Sha256::new();
    h.update(b.key.as_bytes());
    h.update(model.as_bytes());
    // Ohne geklärte Hinweise bleibt der Schlüssel wie er war.
    if !solved.is_empty() {
        h.update(b"\0solved\0");
        h.update(solved.as_bytes());
    }
    h.update(PROMPT_VERSION.to_le_bytes());
    format!("{:x}", h.finalize())
}

/// Prüft Blöcke. Gecachte Ergebnisse kommen sofort, den Rest holt die KI mit begrenzter Parallelität.
#[allow(clippy::too_many_arguments)]
pub async fn review_blocks(
    client: &OpenRouter,
    paths: &Paths,
    model: &str,
    daily_tokens: u64,
    redactor: &Redactor,
    blocks: Vec<Block>,
    texts: &HashMap<String, String>,
    cancel: &CancellationToken,
    mut on_result: impl FnMut(BlockResult),
) -> RunSummary {
    let mut cache = Cache::load(paths.cache.clone());
    let mut budget = Budget::load(paths.budget.clone());
    let mut threads = ThreadStore::load(paths.threads.clone());
    let mut summary = RunSummary::default();
    let text_of = |file: &str| texts.get(file).cloned().unwrap_or_default();

    let mut pending = Vec::new();
    for b in blocks {
        let solved = threads.closed_notes(&b.name);
        // Ein Ergebnis aus der Zeit vor dem Schließen gilt weiter: `apply` filtert geschlossene Hinweise ohnehin heraus.
        // So kostet das Schließen eines Gesprächs keinen neuen Aufruf.
        let hit = cache.get(&cache_key(&b, model, &solved)).or_else(|| {
            (!solved.is_empty())
                .then(|| cache.get(&cache_key(&b, model, "")))
                .flatten()
        });
        match hit {
            Some(review) => {
                summary.cached += 1;
                let diags = to_diagnostics(&b, &review, &text_of(&b.file));
                let diags = threads.apply(&b, diags, &text_of(&b.file));
                let state = if diags.is_empty() {
                    BlockState::Ok
                } else {
                    BlockState::Issues
                };
                on_result(result(&b, state, None, diags, true));
            }
            None => pending.push(b),
        }
    }

    let sem = Arc::new(Semaphore::new(PARALLEL));
    type Done = (Block, Result<(Review, u64), AiError>);
    let mut set: JoinSet<Done> = JoinSet::new();
    let mut queue = pending.into_iter();
    let mut in_flight = 0usize;

    loop {
        // Neue Aufträge nachlegen, solange Platz, Budget und Wille da sind.
        while in_flight < PARALLEL && summary.stopped.is_none() {
            let Some(b) = queue.next() else { break };
            if !budget.allows(daily_tokens) {
                summary.stopped = Some(AiError::Budget.to_string());
                on_result(result(
                    &b,
                    BlockState::Skipped,
                    summary.stopped.clone(),
                    vec![],
                    false,
                ));
                break;
            }
            let (client, model, user, sem, system) = (
                client.clone(),
                model.to_string(),
                user_prompt(&b, redactor, &threads.closed_notes(&b.name)),
                sem.clone(),
                system_for(&b),
            );
            summary.calls += 1;
            in_flight += 1;
            set.spawn(async move {
                let _permit = sem.acquire().await;
                let r = client
                    .chat(ChatRequest {
                        model: &model,
                        system,
                        user: &user,
                        max_tokens: 1200,
                    })
                    .await;
                let outcome = r.and_then(|reply| {
                    parse_reply(&reply.content)
                        .map(|rev| (rev, reply.tokens))
                        .map_err(AiError::BadReply)
                });
                (b, outcome)
            });
        }
        if in_flight == 0 {
            break;
        }
        let joined = tokio::select! {
            _ = cancel.cancelled() => {
                set.abort_all();
                break;
            }
            j = set.join_next() => j,
        };
        let Some(Ok((b, outcome))) = joined else {
            in_flight -= 1;
            continue;
        };
        in_flight -= 1;
        match outcome {
            Ok((review, tokens)) => {
                summary.tokens += tokens;
                budget.add(tokens);
                cache.put(
                    cache_key(&b, model, &threads.closed_notes(&b.name)),
                    review.clone(),
                );
                let diags = to_diagnostics(&b, &review, &text_of(&b.file));
                let diags = threads.apply(&b, diags, &text_of(&b.file));
                let state = if diags.is_empty() {
                    BlockState::Ok
                } else {
                    BlockState::Issues
                };
                on_result(result(&b, state, None, diags, false));
            }
            Err(e) => {
                let fatal = matches!(
                    e,
                    AiError::BadKey | AiError::NoCredit | AiError::Offline | AiError::RateLimited
                );
                if fatal {
                    summary.stopped = Some(e.to_string());
                }
                on_result(result(
                    &b,
                    BlockState::Failed,
                    Some(e.to_string()),
                    vec![],
                    false,
                ));
            }
        }
    }
    cache.save();
    threads.save();
    summary
}

/// Das Ergebnis einer Antwort im Gespräch.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadReply {
    pub thread: Thread,
    pub tokens: u64,
}

/// Der Nutzer antwortet auf einen Hinweis. Die KI liest den Verlauf und antwortet. Akzeptiert sie die Erklärung,
/// schließt Forge das Gespräch.
#[allow(clippy::too_many_arguments)]
pub async fn reply_in_thread(
    client: &OpenRouter,
    paths: &Paths,
    model: &str,
    daily_tokens: u64,
    redactor: &Redactor,
    thread_id: &str,
    user_text: &str,
    block: Option<&Block>,
) -> Result<ThreadReply, AiError> {
    let mut threads = ThreadStore::load(paths.threads.clone());
    let mut budget = Budget::load(paths.budget.clone());
    if threads.get(thread_id).is_none() {
        return Err(AiError::BadReply("Das Gespräch gibt es nicht mehr.".into()));
    }
    if !budget.allows(daily_tokens) {
        return Err(AiError::Budget);
    }
    // Die Nachricht des Nutzers gilt auch dann, wenn die KI gerade nicht antworten kann.
    threads.push(thread_id, Role::User, user_text);
    threads.save();

    let prompt = thread_prompt(
        threads.get(thread_id).expect("eben angelegt"),
        block,
        redactor,
    );
    let reply = client
        .chat(ChatRequest {
            model,
            system: &thread_system(block),
            user: &prompt,
            max_tokens: 700,
        })
        .await?;
    budget.add(reply.tokens);

    let (verdict, text) = parse_thread_reply(&reply.content);
    threads.push(thread_id, Role::Ai, &text);
    if verdict == Verdict::Agree {
        threads.close(thread_id, Resolution::Accepted);
    }
    threads.save();
    Ok(ThreadReply {
        thread: threads.get(thread_id).cloned().expect("eben geändert"),
        tokens: reply.tokens,
    })
}

/// Was die KI zur Behebung eines Hinweises vorschlägt.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixOutcome {
    /// Leer, wenn die KI den Fehler nicht ohne Rückfrage beheben kann.
    pub edits: Vec<TextEdit>,
    pub note: String,
    pub tokens: u64,
}

/// Lässt die KI einen Hinweis beheben. `line` ist die Zeile in der Datei, `file_text` ihr heutiger Stand.
#[allow(clippy::too_many_arguments)]
pub async fn fix_issue(
    client: &OpenRouter,
    paths: &Paths,
    model: &str,
    daily_tokens: u64,
    redactor: &Redactor,
    block: &Block,
    file_text: &str,
    line: u32,
    message: &str,
) -> Result<FixOutcome, AiError> {
    let mut budget = Budget::load(paths.budget.clone());
    if !budget.allows(daily_tokens) {
        return Err(AiError::Budget);
    }
    let prompt = fix_prompt(block, redactor, line, message);
    let reply = client
        .chat(ChatRequest {
            model,
            system: fix_system_for(block),
            user: &prompt,
            max_tokens: 900,
        })
        .await?;
    budget.add(reply.tokens);
    let (fixes, note) = parse_fix(&reply.content).map_err(AiError::BadReply)?;
    let edits = fix_edits(block, file_text, &fixes).map_err(AiError::BadReply)?;
    Ok(FixOutcome {
        edits,
        note,
        tokens: reply.tokens,
    })
}

/// Setzt eine Aufgabe des Nutzers um. `files` sind Paare aus gezeigtem Pfad, vollem Pfad und Text.
#[allow(clippy::too_many_arguments)]
pub async fn run_task(
    client: &OpenRouter,
    paths: &Paths,
    model: &str,
    daily_tokens: u64,
    redactor: &Redactor,
    root: &std::path::Path,
    board: &str,
    files: &[(String, String, String)],
    active: Option<&str>,
    problems: &str,
    task: &str,
) -> Result<TaskOutcome, AiError> {
    let mut budget = Budget::load(paths.budget.clone());
    if !budget.allows(daily_tokens) {
        return Err(AiError::Budget);
    }
    let sources: Vec<SourceText> = files
        .iter()
        .map(|(shown, _, text)| SourceText {
            shown: shown.clone(),
            text: text.clone(),
        })
        .collect();
    let prompt = task_prompt(board, &sources, active, problems, task, redactor);
    let reply = client
        .chat(ChatRequest {
            model,
            system: TASK_SYSTEM,
            user: &prompt,
            max_tokens: 6000,
        })
        .await?;
    budget.add(reply.tokens);
    let (fixes, new_files, note) = parse_task(&reply.content).map_err(AiError::BadReply)?;
    let changes = plan_changes(files, &fixes).map_err(AiError::BadReply)?;
    let created = plan_created(root, |p| p.exists(), &new_files).map_err(AiError::BadReply)?;
    Ok(TaskOutcome {
        changes,
        created,
        note,
        tokens: reply.tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_analysis::{analyze, blocks, AnalysisInput, Chip, SourceFile};
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Ein winziger HTTP-Server, der jede Anfrage aufzeichnet und feste Antworten liefert.
    async fn mock(reply: String, status: u16) -> (String, Arc<Mutex<Vec<(String, String)>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
        let log = seen.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let (reply, log) = (reply.clone(), log.clone());
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut tmp = [0u8; 8192];
                    let (head_end, len) = loop {
                        let n = sock.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8_lossy(&buf[..p]).to_lowercase();
                            let len = head
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length:"))
                                .and_then(|v| v.trim().parse::<usize>().ok())
                                .unwrap_or(0);
                            break (p + 4, len);
                        }
                    };
                    while buf.len() < head_end + len {
                        let n = sock.read(&mut tmp).await.unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        buf.extend_from_slice(&tmp[..n]);
                    }
                    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                    let body = String::from_utf8_lossy(&buf[head_end..]).to_string();
                    log.lock().unwrap().push((head, body));
                    let resp = format!("HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}", reply.len());
                    let _ = sock.write_all(resp.as_bytes()).await;
                });
            }
        });
        (base, seen)
    }

    fn chat_reply(content: &str) -> String {
        serde_json::json!({ "choices": [{ "message": { "content": content } }], "usage": { "total_tokens": 321 } }).to_string()
    }

    const SRC: &str = "const char* password = \"geheim-test-123\";\n#define SIZE 8\nint buf[SIZE];\nvoid fill() {\n  for (int i = 0; i <= SIZE; i++) {\n    buf[i] = analogRead(4);\n  }\n  WiFi.begin(\"Buero\", \"hunter2-hunter2\");\n  Serial.println(buf[0]);\n}\nvoid setup() { fill(); }\nvoid loop() {}\n";

    fn blocks_of(src: &str) -> (Vec<Block>, HashMap<String, String>) {
        let files = [SourceFile {
            path: "a.ino".into(),
            text: src.into(),
        }];
        let input = AnalysisInput {
            files: &files,
            chip: Chip::Esp32S3,
        };
        (
            blocks(&input, &analyze(&input).facts, "ESP32-S3"),
            HashMap::from([("a.ino".to_string(), src.to_string())]),
        )
    }

    fn paths(dir: &std::path::Path) -> Paths {
        Paths {
            cache: dir.join("cache.json"),
            budget: dir.join("budget.json"),
            threads: dir.join("threads.json"),
        }
    }

    #[tokio::test]
    async fn end_to_end_with_cache_budget_and_no_secrets_on_the_wire() {
        let tmp = tempfile::tempdir().unwrap();
        let issue = "{\"status\":\"issues\",\"issues\":[{\"line\":2,\"severity\":\"warning\",\"message\":\"Die Schleife läuft einmal zu weit.\",\"fix\":{\"old\":\"<=\",\"new\":\"<\"}}]}";
        let (base, seen) = mock(chat_reply(issue), 200).await;
        let client = OpenRouter::new("sk-test-key".into(), Some(base));
        let red = Redactor::new([("WIFI_SSID".to_string(), "Buero".to_string())]);
        let (blocks, texts) = blocks_of(SRC);
        assert_eq!(
            blocks.len(),
            1,
            "{:?}",
            blocks.iter().map(|b| &b.name).collect::<Vec<_>>()
        );

        let mut results = Vec::new();
        let s = review_blocks(
            &client,
            &paths(tmp.path()),
            "m/x",
            10_000,
            &red,
            blocks.clone(),
            &texts,
            &CancellationToken::new(),
            |r| results.push(r),
        )
        .await;
        assert_eq!((s.calls, s.cached, s.tokens), (1, 0, 321));
        assert_eq!(results[0].state, BlockState::Issues);
        assert_eq!(results[0].diagnostics[0].line, 5);

        // Auf dem Draht: Schlüssel im Header, aber kein Secret im Text.
        let (head, body) = seen.lock().unwrap()[0].clone();
        assert!(head
            .to_lowercase()
            .contains("authorization: bearer sk-test-key"));
        for leaked in ["geheim-test-123", "hunter2-hunter2", "Buero"] {
            assert!(!body.contains(leaked), "{leaked} im Request: {body}");
        }
        assert!(body.contains("fill") && body.contains("response_format"));

        // Zweiter Lauf: aus dem Cache, ohne Anfrage.
        let mut again = Vec::new();
        let s2 = review_blocks(
            &client,
            &paths(tmp.path()),
            "m/x",
            10_000,
            &red,
            blocks,
            &texts,
            &CancellationToken::new(),
            |r| again.push(r),
        )
        .await;
        assert_eq!((s2.calls, s2.cached), (0, 1));
        assert!(again[0].cached);
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_conversation_can_close_a_finding_for_free_and_the_model_learns_the_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let issue = "{\"status\":\"issues\",\"issues\":[{\"line\":2,\"severity\":\"warning\",\"message\":\"Die Schleife läuft einmal zu weit.\"}]}";
        let (review_base, review_seen) = mock(chat_reply(issue), 200).await;
        let reviewer = OpenRouter::new("k".into(), Some(review_base));
        let agree =
            "{\"verdict\":\"agree\",\"reply\":\"Stimmt, mit SIZE+1 Plätzen ist das richtig.\"}";
        let (reply_base, reply_seen) = mock(chat_reply(agree), 200).await;
        let talker = OpenRouter::new("k".into(), Some(reply_base));
        let red = Redactor::new([("WIFI_PASS".to_string(), "hunter2-hunter2".to_string())]);
        let (blocks, texts) = blocks_of(SRC);
        let run = |client: OpenRouter, blocks: Vec<Block>, texts: HashMap<String, String>| {
            let paths = paths(tmp.path());
            let red = red.clone();
            async move {
                let mut out = Vec::new();
                let s = review_blocks(
                    &client,
                    &paths,
                    "m/x",
                    100_000,
                    &red,
                    blocks,
                    &texts,
                    &CancellationToken::new(),
                    |r| out.push(r),
                )
                .await;
                (s, out)
            }
        };

        // 1. Die KI meldet etwas, es entsteht ein Gespräch.
        let (s, out) = run(reviewer.clone(), blocks.clone(), texts.clone()).await;
        assert_eq!(s.calls, 1);
        let id = out[0].diagnostics[0].id.clone();
        assert!(id.starts_with("ai:"));
        let thread_id = id.trim_start_matches("ai:").to_string();

        // 2. Der Nutzer erklärt, die KI gibt nach, das Gespräch schließt sich.
        let r = reply_in_thread(
            &talker,
            &paths(tmp.path()),
            "m/x",
            100_000,
            &red,
            &thread_id,
            "Absicht: buf hat SIZE+1 Plätze. Mein Passwort hunter2-hunter2 ist geheim.",
            Some(&blocks[0]),
        )
        .await
        .unwrap();
        assert_eq!(r.thread.status, crate::threads::Status::Closed);
        assert_eq!(r.thread.resolution, Some(Resolution::Accepted));
        assert_eq!(r.thread.messages.len(), 3);
        let (_, body) = reply_seen.lock().unwrap()[0].clone();
        assert!(body.contains("Absicht: buf hat SIZE+1") && body.contains("fill"));
        assert!(
            !body.contains("hunter2-hunter2"),
            "Secret im Gespräch: {body}"
        );

        // 3. Dieselbe Funktion noch einmal prüfen: nichts mehr gemeldet, und es kostet keinen Aufruf.
        let (s, out) = run(reviewer.clone(), blocks.clone(), texts.clone()).await;
        assert_eq!((s.calls, s.cached), (0, 1));
        assert_eq!(out[0].state, BlockState::Ok);
        assert!(out[0].diagnostics.is_empty());
        assert_eq!(review_seen.lock().unwrap().len(), 1);

        // 4. Ändert sich die Funktion, fragt Forge die KI neu und sagt ihr, was schon geklärt ist.
        let changed = SRC.replace("analogRead(4)", "analogRead(5)");
        let (blocks2, texts2) = blocks_of(&changed);
        let (s, out) = run(reviewer.clone(), blocks2.clone(), texts2.clone()).await;
        assert_eq!(s.calls, 1);
        // Die KI meldet dieselbe Zeile wieder. Sie bleibt trotzdem still.
        assert!(out[0].diagnostics.is_empty());
        let (_, body) = review_seen.lock().unwrap()[1].clone();
        assert!(body.contains("Bereits geklärt") && body.contains("Absicht: buf hat SIZE+1"));
        assert!(!body.contains("hunter2-hunter2"));

        // 5. Wieder geöffnet, kommt der Hinweis zurück. Für den alten Stand der Funktion ohne neuen Aufruf.
        let mut store = ThreadStore::load(paths(tmp.path()).threads);
        store.reopen(&thread_id);
        store.save();
        let (s, out) = run(reviewer, blocks, texts).await;
        assert_eq!(s.calls, 0);
        assert_eq!(out[0].diagnostics.len(), 1);
    }

    #[tokio::test]
    async fn an_empty_budget_stops_before_any_request() {
        let tmp = tempfile::tempdir().unwrap();
        let (base, seen) = mock(chat_reply("{\"status\":\"ok\"}"), 200).await;
        let client = OpenRouter::new("k".into(), Some(base));
        Budget::load(tmp.path().join("budget.json")).add(5_000);
        let (blocks, texts) = blocks_of(SRC);
        let mut results = Vec::new();
        let s = review_blocks(
            &client,
            &paths(tmp.path()),
            "m/x",
            1_000,
            &Redactor::default(),
            blocks,
            &texts,
            &CancellationToken::new(),
            |r| results.push(r),
        )
        .await;
        assert_eq!(s.calls, 0);
        assert!(s.stopped.unwrap().contains("Tageslimit"));
        assert_eq!(results[0].state, BlockState::Skipped);
        assert!(seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_rejected_key_is_reported_once_and_stops_the_run() {
        let tmp = tempfile::tempdir().unwrap();
        let (base, _) = mock("{}".into(), 401).await;
        let client = OpenRouter::new("falsch".into(), Some(base));
        let (blocks, texts) = blocks_of(SRC);
        let mut results = Vec::new();
        let s = review_blocks(
            &client,
            &paths(tmp.path()),
            "m/x",
            1_000,
            &Redactor::default(),
            blocks,
            &texts,
            &CancellationToken::new(),
            |r| results.push(r),
        )
        .await;
        assert!(s.stopped.unwrap().contains("Schlüssel"));
        assert_eq!(results[0].state, BlockState::Failed);
    }

    #[tokio::test]
    async fn unusable_replies_fail_the_block_but_not_the_run() {
        let tmp = tempfile::tempdir().unwrap();
        let (base, _) = mock(chat_reply("Das sieht gut aus!"), 200).await;
        let client = OpenRouter::new("k".into(), Some(base));
        let (blocks, texts) = blocks_of(SRC);
        let mut results = Vec::new();
        let s = review_blocks(
            &client,
            &paths(tmp.path()),
            "m/x",
            10_000,
            &Redactor::default(),
            blocks,
            &texts,
            &CancellationToken::new(),
            |r| results.push(r),
        )
        .await;
        assert!(s.stopped.is_none());
        assert_eq!(results[0].state, BlockState::Failed);
    }
}
