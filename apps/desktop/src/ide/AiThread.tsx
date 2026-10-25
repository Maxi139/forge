import { AiPulse } from "./AiSignal";
import { Button, Icon } from "@forge/ui";
import { useEffect, useRef, useState } from "react";
import { t } from "../i18n/de";
import { useAi } from "../lib/ai";
import type { AiThread } from "../lib/backend";

/** Das Gespräch zu einem Hinweis der KI, wie ein Issue: Verlauf, Antwortfeld, Schließen. */
export function AiThreadView({ thread }: { thread: AiThread }) {
  const sending = useAi((s) => !!s.sending[thread.id]);
  const reply = useAi((s) => s.reply);
  const closeThread = useAi((s) => s.closeThread);
  const reopenThread = useAi((s) => s.reopenThread);
  const [draft, setDraft] = useState("");
  const input = useRef<HTMLTextAreaElement>(null);
  const root = useRef<HTMLDivElement>(null);
  // Das Panel ist niedrig. Neue Nachrichten und das Antwortfeld sollen sichtbar sein.
  useEffect(() => {
    root.current?.scrollIntoView({ block: "nearest" });
  }, [thread.messages.length, sending]);
  const closed = thread.status === "closed";
  const asked = thread.messages.some((m) => m.role === "user");

  const send = (text: string) => {
    const body = text.trim();
    if (!body || sending) return;
    setDraft("");
    void reply(thread.id, body);
  };

  return (
    <div ref={root} className="ai-thread" role="group" aria-label={t.ai.discussTitle}>
      <div className="ai-line fg-mono" title={thread.lineText}>
        {thread.lineText}
      </div>
      <div className="ai-msgs" aria-live="polite">
        {thread.messages.map((m, i) => (
          <div key={`${m.at}-${i}`} className={`ai-msg is-${m.role}`}>
            <span className="who">
              {m.role === "ai" ? <Icon name="spark" size={12} /> : null}
              {m.role === "ai" ? t.ai.assistant : t.ai.you}
            </span>
            <div className="text">{m.text}</div>
          </div>
        ))}
        {sending ? (
          <div className="ai-msg is-ai is-typing">
            <span className="who">
              <Icon name="spark" size={12} />
              {t.ai.assistant}
            </span>
            <div className="text" role="status">
              <AiPulse />
              {t.ai.typing}
            </div>
          </div>
        ) : null}
      </div>

      {closed ? (
        <div className="ai-closed">
          <Icon name="check-circle" size={14} />
          <span>{thread.resolution ? t.ai.closedBy[thread.resolution] : t.ai.close}</span>
          {thread.resolution !== "fixed" ? (
            <Button size="sm" onClick={() => void reopenThread(thread.id)}>
              {t.ai.reopen}
            </Button>
          ) : null}
        </div>
      ) : (
        <div className="ai-compose">
          <textarea
            ref={input}
            value={draft}
            rows={2}
            placeholder={t.ai.replyPlaceholder}
            aria-label={t.ai.replyPlaceholder}
            disabled={sending}
            spellCheck
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                send(draft);
              }
              e.stopPropagation();
            }}
          />
          <div className="ai-actions">
            {!asked ? (
              <Button size="sm" disabled={sending} onClick={() => send(t.ai.explainAsk)}>
                {t.ai.explain}
              </Button>
            ) : null}
            <Button
              size="sm"
              disabled={sending}
              onClick={() => {
                setDraft((d) => d || t.ai.intendedStart);
                requestAnimationFrame(() => {
                  const el = input.current;
                  if (!el) return;
                  el.focus();
                  el.setSelectionRange(el.value.length, el.value.length);
                });
              }}
            >
              {t.ai.intended}
            </Button>
            <span className="spacer" />
            <Button size="sm" variant="ghost" title={t.ai.closeHint} disabled={sending} onClick={() => void closeThread(thread.id)}>
              {t.ai.close}
            </Button>
            <Button size="sm" variant="primary" icon="send" disabled={sending || !draft.trim()} onClick={() => send(draft)}>
              {t.ai.send}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
