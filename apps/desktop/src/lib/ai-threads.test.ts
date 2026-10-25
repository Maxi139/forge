import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAi } from "./ai";
import { createMockBackend } from "./backend/mock";
import { setBackend } from "./backend";
import { useStore } from "./store";

const SAMPLE = "/Users/demo/Documents/Forge/neopixel-sensor";
const CODE = "int x[9];\nvoid fill() {\n  for (int i = 0; i <= 8; i++) {\n    x[i] = analogRead(4);\n  }\n  Serial.println(x[0]);\n}\n";

async function setup() {
  const mock = createMockBackend({ speed: 0 });
  setBackend(mock);
  useAi.getState().reset();
  await useAi.getState().init();
  await mock.aiConfigure({ key: "k" });
  await useStore.getState().openProject(SAMPLE);
  await mock.writeFile(`${SAMPLE}/neopixel-sensor.ino`, CODE);
  await useAi.getState().refresh();
  await useAi.getState().run();
  await vi.waitFor(() => expect(Object.keys(useAi.getState().threads)).toHaveLength(1));
  await vi.waitFor(() => expect(diagnosticIds()).toHaveLength(1));
  return mock;
}

const diagnosticIds = () => Object.values(useAi.getState().blocks).flatMap((l) => l.flatMap((b) => b.diagnostics.map((d) => d.id)));

describe("Gespräche über Hinweise der KI", () => {
  beforeEach(() => setBackend(null));

  it("jeder Hinweis ist ein Gespräch, in dem die KI zuerst gesprochen hat", async () => {
    await setup();
    const [thread] = Object.values(useAi.getState().threads);
    expect(thread.status).toBe("open");
    expect(thread.messages).toHaveLength(1);
    expect(thread.messages[0].role).toBe("ai");
    expect(diagnosticIds()).toEqual([`ai:${thread.id}`]);
  });

  it("erklärt der Nutzer, dass es gewollt ist, schließt die KI das Gespräch und der Hinweis verschwindet", async () => {
    await setup();
    const [{ id }] = Object.values(useAi.getState().threads);
    await useAi.getState().reply(id, "Das ist Absicht, x hat 9 Plätze.");
    const t = useAi.getState().threads[id];
    expect(t.status).toBe("closed");
    expect(t.resolution).toBe("accepted");
    expect(t.messages.map((m) => m.role)).toEqual(["ai", "user", "ai"]);
    expect(diagnosticIds()).toEqual([]);
  });

  it("widerspricht die KI, bleibt das Gespräch offen, und der Nutzer kann trotzdem schließen und wieder öffnen", async () => {
    await setup();
    const [{ id }] = Object.values(useAi.getState().threads);
    await useAi.getState().reply(id, "Warum eigentlich?");
    expect(useAi.getState().threads[id].status).toBe("open");
    expect(useAi.getState().threads[id].messages).toHaveLength(3);
    expect(diagnosticIds()).toEqual([`ai:${id}`]);

    await useAi.getState().closeThread(id);
    expect(useAi.getState().threads[id].resolution).toBe("dismissed");
    expect(diagnosticIds()).toEqual([]);

    await useAi.getState().reopenThread(id);
    await vi.waitFor(() => expect(diagnosticIds()).toEqual([`ai:${id}`]));
    expect(useAi.getState().threads[id].status).toBe("open");
  });
});
