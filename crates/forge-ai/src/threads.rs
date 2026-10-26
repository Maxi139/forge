//! Gespräche über Hinweise der KI. Jeder Hinweis ist wie ein Issue: Man kann antworten, nachfragen und ihn schließen.
//! Geschlossene Hinweise meldet die KI nicht noch einmal, ohne dass dafür ein Aufruf nötig ist.

use forge_analysis::Block;
use forge_core::{utf16_col, Diagnostic, DiagnosticSource, Severity};
use forge_secrets::Redactor;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Mehr Gespräche behält ein Projekt nicht. Zuerst fallen die ältesten geschlossenen heraus.
const MAX_THREADS: usize = 200;
/// So viele Nachrichten eines Gesprächs sieht die KI, wenn sie antwortet.
const MAX_TRANSCRIPT: usize = 10;
const MAX_NOTE_THREADS: usize = 5;
const MAX_MESSAGE_CHARS: usize = 1500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Ai,
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub text: String,
    pub at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Open,
    Closed,
}

/// Wie ein Gespräch zu Ende ging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Resolution {
    /// Die KI hat die Erklärung des Nutzers akzeptiert.
    Accepted,
    /// Der Nutzer hat den Hinweis geschlossen.
    Dismissed,
    /// Die Zeile wurde geändert, der Hinweis hat sich erledigt.
    Fixed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    /// Fingerabdruck aus Funktionsname und Text der Zeile. Er bleibt gleich, wenn die KI denselben Fund anders formuliert
    /// oder die Zeile in der Datei wandert.
    pub id: String,
    pub function: String,
    pub line_text: String,
    pub severity: String,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<Resolution>,
    pub messages: Vec<Message>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Thread {
    fn user_messages(&self) -> usize {
        self.messages
            .iter()
            .filter(|m| m.role == Role::User)
            .count()
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Zeile ohne Einrückung und mit einfachen Leerzeichen, damit Umformatieren den Fund nicht neu macht.
fn normalize(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn fingerprint(function: &str, line: &str) -> String {
    let mut h = Sha256::new();
    h.update(function.as_bytes());
    h.update(b"\n");
    h.update(normalize(line).as_bytes());
    h.finalize()
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max).collect::<String>())
    }
}

pub struct ThreadStore {
    path: PathBuf,
    threads: Vec<Thread>,
    dirty: bool,
}

impl ThreadStore {
    pub fn load(path: PathBuf) -> Self {
        let threads = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            path,
            threads,
            dirty: false,
        }
    }

    pub fn all(&self) -> &[Thread] {
        &self.threads
    }

    pub fn get(&self, id: &str) -> Option<&Thread> {
        self.threads.iter().find(|t| t.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut Thread> {
        self.dirty = true;
        self.threads.iter_mut().find(|t| t.id == id)
    }

    /// Schreibt die Datei, nur wenn sich etwas geändert hat.
    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        if self.threads.len() > MAX_THREADS {
            // Offene Gespräche und solche mit Antwort des Nutzers bleiben am längsten.
            self.threads.sort_by_key(|t| {
                (
                    t.status == Status::Open,
                    t.user_messages() > 0,
                    std::cmp::Reverse(t.updated_at),
                )
            });
            self.threads.reverse();
            self.threads.truncate(MAX_THREADS);
            self.threads.sort_by_key(|t| t.created_at);
        }
        if self.threads.is_empty() {
            let _ = std::fs::remove_file(&self.path);
        } else {
            if let Some(parent) = self.path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let tmp = self.path.with_extension("tmp");
            if std::fs::write(&tmp, serde_json::to_vec(&self.threads).unwrap_or_default()).is_ok() {
                let _ = std::fs::rename(tmp, &self.path);
            }
        }
        self.dirty = false;
    }

    /// Gleicht die Funde einer Prüfung mit den Gesprächen ab und gibt die Hinweise zurück, die angezeigt werden.
    ///
    /// - Ein Fund, zu dem es ein geschlossenes Gespräch gibt, verschwindet.
    /// - Ein neuer Fund eröffnet ein Gespräch, mit dem Hinweis der KI als erster Nachricht.
    /// - Ein offenes Gespräch, das die KI nicht mehr meldet, bleibt sichtbar, wenn der Nutzer schon geantwortet hat
    ///   und die Zeile noch da ist. Ohne Antwort verschwindet es leise. Ist die Zeile weg, gilt es als erledigt.
    pub fn apply(
        &mut self,
        block: &Block,
        found: Vec<Diagnostic>,
        file_text: &str,
    ) -> Vec<Diagnostic> {
        let lines: Vec<&str> = file_text.split('\n').collect();
        let line_at = |n: u32| lines.get(n as usize - 1).copied().unwrap_or("");
        let mut shown: Vec<Diagnostic> = Vec::new();
        let mut seen: Vec<String> = Vec::new();

        for mut d in found {
            let text = line_at(d.line);
            let id = fingerprint(&block.name, text);
            if seen.contains(&id) {
                continue;
            }
            seen.push(id.clone());
            match self.threads.iter().find(|t| t.id == id) {
                Some(t) if t.status == Status::Closed => continue,
                Some(t) => {
                    if let Some(first) = t.messages.first() {
                        d.message = first.text.clone();
                    }
                }
                None => {
                    self.dirty = true;
                    self.threads.push(Thread {
                        id: id.clone(),
                        function: block.name.clone(),
                        line_text: normalize(text),
                        severity: if d.severity == Severity::Warning {
                            "warning".into()
                        } else {
                            "info".into()
                        },
                        status: Status::Open,
                        resolution: None,
                        messages: vec![Message {
                            role: Role::Ai,
                            text: d.message.clone(),
                            at: now(),
                        }],
                        created_at: now(),
                        updated_at: now(),
                    });
                }
            }
            d.id = format!("ai:{id}");
            shown.push(d);
        }

        // Offene Gespräche dieser Funktion, die diesmal nicht gemeldet wurden.
        let candidates: Vec<String> = self
            .threads
            .iter()
            .filter(|t| {
                t.function == block.name && t.status == Status::Open && !seen.contains(&t.id)
            })
            .map(|t| t.id.clone())
            .collect();
        for id in candidates {
            let Some(t) = self.threads.iter().find(|t| t.id == id).cloned() else {
                continue;
            };
            let at =
                (block.start_line..=block.end_line).find(|n| normalize(line_at(*n)) == t.line_text);
            match (at, t.user_messages() > 0) {
                (Some(n), true) => {
                    let line = line_at(n);
                    let indent = line.len() - line.trim_start().len();
                    let from = utf16_col(line, indent);
                    let to = utf16_col(line, line.trim_end().len()).max(from + 1);
                    shown.push(Diagnostic {
                        id: format!("ai:{id}"),
                        file: block.file.clone(),
                        line: n,
                        from,
                        to,
                        severity: if t.severity == "warning" {
                            Severity::Warning
                        } else {
                            Severity::Info
                        },
                        message: t
                            .messages
                            .first()
                            .map(|m| m.text.clone())
                            .unwrap_or_default(),
                        source: DiagnosticSource::Ai,
                        fix: None,
                        action: None,
                    });
                }
                (Some(_), false) => self.threads.retain(|x| x.id != id),
                (None, true) => {
                    if let Some(x) = self.get_mut(&id) {
                        x.status = Status::Closed;
                        x.resolution = Some(Resolution::Fixed);
                        x.updated_at = now();
                    }
                }
                (None, false) => self.threads.retain(|x| x.id != id),
            }
            self.dirty = true;
        }
        shown
    }

    /// Was der Nutzer für diese Funktion schon geklärt hat. Geht an die KI, damit sie es nicht anders formuliert noch
    /// einmal meldet. Leer, wenn es nichts gibt.
    pub fn closed_notes(&self, function: &str) -> String {
        let mut closed: Vec<&Thread> = self
            .threads
            .iter()
            .filter(|t| {
                t.function == function
                    && t.status == Status::Closed
                    && t.resolution != Some(Resolution::Fixed)
            })
            .collect();
        closed.sort_by_key(|t| std::cmp::Reverse(t.updated_at));
        closed
            .into_iter()
            .take(MAX_NOTE_THREADS)
            .map(|t| {
                let reason = t
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == Role::User)
                    .map(|m| clip(&m.text, 300))
                    .unwrap_or_else(|| "ohne Begründung geschlossen".into());
                let hint = t
                    .messages
                    .first()
                    .map(|m| clip(&m.text, 200))
                    .unwrap_or_default();
                format!(
                    "- Zeile „{}“: Hinweis „{hint}“ (vom Nutzer geklärt: {reason})\n",
                    clip(&t.line_text, 120)
                )
            })
            .collect()
    }

    pub fn push(&mut self, id: &str, role: Role, text: &str) -> Option<&Thread> {
        let t = self.get_mut(id)?;
        t.messages.push(Message {
            role,
            text: clip(text, MAX_MESSAGE_CHARS),
            at: now(),
        });
        t.updated_at = now();
        self.get(id)
    }

    pub fn close(&mut self, id: &str, resolution: Resolution) -> Option<&Thread> {
        let t = self.get_mut(id)?;
        t.status = Status::Closed;
        t.resolution = Some(resolution);
        t.updated_at = now();
        self.get(id)
    }

    pub fn reopen(&mut self, id: &str) -> Option<&Thread> {
        let t = self.get_mut(id)?;
        t.status = Status::Open;
        t.resolution = None;
        t.updated_at = now();
        self.get(id)
    }
}

/// Anweisung für die Antwort im Gespräch.
pub const THREAD_SYSTEM: &str = r#"Du bist ein aufmerksamer, freundlicher Kollege, der den Code des Nutzers durchsieht (ESP32-Sketch, Arduino-Framework). Du hast einen Hinweis zu einer Zeile gegeben, und der Nutzer antwortet darauf.

Lies seine Antwort ernst und beantworte genau das, was er schreibt.
- Zeigt seine Erklärung, dass das Verhalten gewollt oder korrekt ist, gib dich geschlagen: bestätige kurz, was du jetzt verstanden hast. verdict "agree".
- Fragt er nach, erkläre es verständlich in zwei bis fünf Sätzen, wenn es hilft mit einem kleinen Beispiel mit konkreten Werten. verdict "explain".
- Siehst du nach seiner Antwort weiter einen echten Fehler, bleib freundlich, nenne die Zeile und konkrete Werte, bei denen es schiefgeht. verdict "disagree".
Behaupte nichts, was nicht im Code steht. Der Kontext zeigt, wie die Funktion benutzt wird, verlass dich darauf. Wenn du dir nicht sicher bist, sag das offen. Der Nutzer entscheidet am Ende.

Antworte ausschließlich mit einem JSON-Objekt: {"verdict":"agree" oder "explain" oder "disagree","reply":"<deine Antwort auf Deutsch>"}"#;

/// Anweisung für das Gespräch. Bei Webdateien kommt ein Hinweis dazu, worum es geht.
pub fn thread_system(block: Option<&Block>) -> String {
    match block {
        Some(b) if b.lang != "cpp" => format!(
            "{THREAD_SYSTEM}\n\nDer Code ist keine Funktion, sondern eine Webdatei ({}) des Sketches, die der ESP32 über seinen Webserver ausliefert.",
            b.lang
        ),
        _ => THREAD_SYSTEM.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Agree,
    Explain,
    Disagree,
}

#[derive(Deserialize)]
struct RawReply {
    #[serde(default)]
    verdict: String,
    #[serde(default)]
    reply: String,
}

/// Liest die Antwort der KI. Kommt kein verwertbares JSON, gilt der Text als Erklärung.
pub fn parse_thread_reply(content: &str) -> (Verdict, String) {
    let parsed = content
        .find('{')
        .zip(content.rfind('}'))
        .and_then(|(a, b)| serde_json::from_str::<RawReply>(&content[a..=b]).ok())
        .filter(|r| !r.reply.trim().is_empty());
    match parsed {
        Some(r) => (
            match r.verdict.as_str() {
                "agree" => Verdict::Agree,
                "disagree" => Verdict::Disagree,
                _ => Verdict::Explain,
            },
            r.reply.trim().to_string(),
        ),
        None => (Verdict::Explain, content.trim().to_string()),
    }
}

/// Der Text für die KI: Kontext, Funktion und der bisherige Verlauf. Secrets sind geschwärzt.
pub fn thread_prompt(thread: &Thread, block: Option<&Block>, redactor: &Redactor) -> String {
    let mut out = String::new();
    if let Some(b) = block {
        out.push_str(&format!(
            "Kontext:\n{}\n{} „{}“ (die Zeilen sind nummeriert):\n```{}\n{}\n```\n\n",
            b.context,
            crate::prompt::what(b),
            b.name,
            crate::prompt::fence(b),
            b.code
        ));
    } else {
        out.push_str(&format!(
            "Die Funktion „{}“ liegt gerade nicht vor. Es geht um die Zeile: {}\n\n",
            thread.function, thread.line_text
        ));
    }
    out.push_str(&format!(
        "Betroffene Zeile: {}\n\nVerlauf:\n",
        thread.line_text
    ));
    let skip = thread.messages.len().saturating_sub(MAX_TRANSCRIPT);
    for m in thread.messages.iter().skip(skip) {
        let who = if m.role == Role::Ai { "Du" } else { "Nutzer" };
        out.push_str(&format!("{who}: {}\n", m.text));
    }
    redactor.apply(&out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_analysis::{analyze, blocks, AnalysisInput, Chip, SourceFile};

    const SRC: &str = "#define SIZE 8\nint buf[SIZE];\nvoid fill() {\n  for (int i = 0; i <= SIZE; i++) {\n    buf[i] = analogRead(4);\n  }\n  Serial.println(buf[0]);\n}\nvoid setup() { fill(); }\nvoid loop() {}\n";

    fn block_of(src: &str) -> Block {
        let files = [SourceFile {
            path: "a.ino".into(),
            text: src.into(),
        }];
        let input = AnalysisInput {
            files: &files,
            chip: Chip::Esp32S3,
        };
        blocks(&input, &analyze(&input).facts, "ESP32-S3")
            .into_iter()
            .find(|b| b.name == "fill")
            .unwrap()
    }

    fn found(line: u32, message: &str) -> Diagnostic {
        Diagnostic {
            id: "x".into(),
            file: "a.ino".into(),
            line,
            from: 2,
            to: 10,
            severity: Severity::Warning,
            message: message.into(),
            source: DiagnosticSource::Ai,
            fix: None,
            action: None,
        }
    }

    fn store() -> (tempfile::TempDir, ThreadStore) {
        let tmp = tempfile::tempdir().unwrap();
        let s = ThreadStore::load(tmp.path().join(".forge/threads.json"));
        (tmp, s)
    }

    #[test]
    fn a_finding_opens_a_thread_with_a_stable_id() {
        let (_t, mut s) = store();
        let b = block_of(SRC);
        let first = s.apply(
            &b,
            vec![found(4, "Die Schleife läuft einmal zu weit.")],
            SRC,
        );
        assert_eq!(first.len(), 1);
        assert!(first[0].id.starts_with("ai:"));
        assert_eq!(s.all().len(), 1);
        assert_eq!(s.all()[0].messages[0].role, Role::Ai);
        // Dieselbe Zeile, anders formuliert: dasselbe Gespräch, die erste Formulierung bleibt.
        let again = s.apply(&b, vec![found(4, "Index läuft hinter das Array.")], SRC);
        assert_eq!(s.all().len(), 1);
        assert_eq!(again[0].id, first[0].id);
        assert_eq!(again[0].message, "Die Schleife läuft einmal zu weit.");
    }

    #[test]
    fn closing_a_thread_silences_the_finding_even_when_reworded_and_after_moving() {
        let (_t, mut s) = store();
        let b = block_of(SRC);
        let id = s.apply(&b, vec![found(4, "A")], SRC)[0]
            .id
            .trim_start_matches("ai:")
            .to_string();
        s.push(
            &id,
            Role::User,
            "Das <= ist Absicht, ich brauche SIZE+1 Werte.",
        );
        s.close(&id, Resolution::Accepted);
        assert!(s
            .apply(&b, vec![found(4, "Ganz anders formuliert")], SRC)
            .is_empty());

        // Zwei Zeilen davor eingefügt: die Zeile wandert, der Hinweis bleibt geschlossen.
        let moved = format!("// neu\n// neu\n{SRC}");
        let b2 = block_of(&moved);
        assert!(s.apply(&b2, vec![found(6, "Nochmal")], &moved).is_empty());
        assert_eq!(s.all().len(), 1);
    }

    #[test]
    fn reopening_brings_the_finding_back() {
        let (_t, mut s) = store();
        let b = block_of(SRC);
        let id = s.apply(&b, vec![found(4, "A")], SRC)[0]
            .id
            .trim_start_matches("ai:")
            .to_string();
        s.close(&id, Resolution::Dismissed);
        assert!(s.apply(&b, vec![found(4, "A")], SRC).is_empty());
        s.reopen(&id);
        assert_eq!(s.apply(&b, vec![found(4, "A")], SRC).len(), 1);
    }

    #[test]
    fn open_threads_stay_visible_only_with_a_user_reply_and_close_when_the_line_is_gone() {
        let (_t, mut s) = store();
        let b = block_of(SRC);
        let id = s.apply(&b, vec![found(4, "A")], SRC)[0]
            .id
            .trim_start_matches("ai:")
            .to_string();

        // Die KI meldet nichts mehr, der Nutzer hat nie geantwortet: der Eintrag verschwindet leise.
        assert!(s.apply(&b, vec![], SRC).is_empty());
        assert!(s.all().is_empty());

        // Mit Antwort des Nutzers bleibt das Gespräch sichtbar, an der richtigen Zeile.
        let id2 = s.apply(&b, vec![found(4, "A")], SRC)[0]
            .id
            .trim_start_matches("ai:")
            .to_string();
        assert_eq!(id, id2);
        s.push(&id2, Role::User, "Warum?");
        let shown = s.apply(&b, vec![], SRC);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].line, 4);

        // Wird die Zeile geändert, ist es erledigt.
        let fixed = SRC.replace("i <= SIZE", "i < SIZE");
        let b3 = block_of(&fixed);
        assert!(s.apply(&b3, vec![], &fixed).is_empty());
        assert_eq!(s.get(&id2).unwrap().resolution, Some(Resolution::Fixed));
        // Erledigte Zeilen tauchen nicht in den Notizen für die KI auf.
        assert!(s.closed_notes("fill").is_empty());
    }

    #[test]
    fn notes_carry_the_users_reason_to_the_model() {
        let (_t, mut s) = store();
        let b = block_of(SRC);
        let id = s.apply(&b, vec![found(4, "Läuft einmal zu weit.")], SRC)[0]
            .id
            .trim_start_matches("ai:")
            .to_string();
        assert!(s.closed_notes("fill").is_empty());
        s.push(&id, Role::User, "Absicht: das Array hat SIZE+1 Plätze.");
        s.close(&id, Resolution::Accepted);
        let notes = s.closed_notes("fill");
        assert!(notes.contains("i <= SIZE"));
        assert!(notes.contains("Absicht"));
        assert!(notes.contains("Läuft einmal zu weit."));
        assert!(s.closed_notes("andere").is_empty());
    }

    #[test]
    fn persists_and_prunes() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".forge/threads.json");
        let mut s = ThreadStore::load(path.clone());
        let b = block_of(SRC);
        s.apply(&b, vec![found(4, "A")], SRC);
        s.save();
        assert_eq!(ThreadStore::load(path.clone()).all().len(), 1);
        for i in 0..MAX_THREADS + 20 {
            s.threads.push(Thread {
                id: format!("{i:016x}"),
                function: "f".into(),
                line_text: format!("x{i}"),
                severity: "info".into(),
                status: Status::Closed,
                resolution: Some(Resolution::Dismissed),
                messages: vec![],
                created_at: i as u64,
                updated_at: i as u64,
            });
        }
        s.dirty = true;
        s.save();
        let back = ThreadStore::load(path);
        assert_eq!(back.all().len(), MAX_THREADS);
        // Das offene Gespräch hat überlebt.
        assert!(back.all().iter().any(|t| t.status == Status::Open));
    }

    #[test]
    fn replies_are_parsed_leniently() {
        assert_eq!(
            parse_thread_reply("```json\n{\"verdict\":\"agree\",\"reply\":\"Verstanden.\"}\n```"),
            (Verdict::Agree, "Verstanden.".into())
        );
        assert_eq!(
            parse_thread_reply(
                "{\"verdict\":\"disagree\",\"reply\":\"Bei i=8 liest du hinter das Array.\"}"
            )
            .0,
            Verdict::Disagree
        );
        // Kein JSON: der Text ist die Antwort.
        assert_eq!(
            parse_thread_reply("Einfach so geantwortet."),
            (Verdict::Explain, "Einfach so geantwortet.".into())
        );
        // Leere Antwort im JSON: Rohtext statt leerer Blase.
        assert_eq!(
            parse_thread_reply("{\"verdict\":\"agree\",\"reply\":\"\"}").0,
            Verdict::Explain
        );
    }

    #[test]
    fn the_thread_prompt_hides_secrets_and_keeps_the_conversation() {
        let (_t, mut s) = store();
        let b = block_of(SRC);
        let id = s.apply(&b, vec![found(4, "A")], SRC)[0]
            .id
            .trim_start_matches("ai:")
            .to_string();
        s.push(
            &id,
            Role::User,
            "Mein Passwort ist hunter2-hunter2, aber das <= ist Absicht.",
        );
        let red = Redactor::new([("WIFI_PASS".to_string(), "hunter2-hunter2".to_string())]);
        let p = thread_prompt(s.get(&id).unwrap(), Some(&b), &red);
        assert!(!p.contains("hunter2-hunter2"));
        assert!(p.contains("Nutzer: Mein Passwort"));
        assert!(p.contains("Du: A"));
        assert!(p.contains("fill"));
    }
}
