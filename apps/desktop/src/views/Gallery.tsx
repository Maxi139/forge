import {
  Badge,
  BuildProgress,
  Button,
  Checkbox,
  CommandPalette,
  Dialog,
  EmptyState,
  FileTree,
  Input,
  Kbd,
  LibraryItem,
  MemoryMeter,
  PartitionMap,
  Pinout,
  Plotter,
  SerialMonitor,
  Select,
  Splash,
  StatusDot,
  Tabs,
  Toast,
  Traces,
  type ConnectionState,
  ICON_NAMES,
  Icon,
} from "@forge/ui";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { ForgeEditor } from "../editor/ForgeEditor";
import { SAMPLE_SKETCH } from "../lib/backend/mock";
import type { Diagnostic } from "../lib/diagnostics";
import { setThemePreference, toggleTheme, useTheme } from "../lib/theme";

const DIAG: Diagnostic[] = [
  { id: "1", file: "x", line: 22, from: 10, to: 11, severity: "error", message: "';' erwartet vor '}'", source: "syntax", fix: { label: "';' einfügen", edits: [{ fromLine: 22, fromCol: 10, toLine: 22, toCol: 10, text: ";" }] } },
  { id: "2", file: "x", line: 18, from: 8, to: 22, severity: "info", message: "volts wird nur ausgegeben, nie weiterverwendet.", source: "ai" },
];

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section>
      <h2>{title}</h2>
      {children}
    </section>
  );
}

export function Gallery() {
  const { resolved } = useTheme();
  const [busy, setBusy] = useState(false);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const id = setInterval(() => setTick((n) => n + 1), 120);
    return () => clearInterval(id);
  }, []);

  const series = useMemo(() => {
    const v1: number[] = [];
    const v2: number[] = [];
    for (let i = 0; i < 120; i++) {
      const x = (tick + i) / 8;
      v1.push(1.65 + Math.sin(x) * 0.9);
      v2.push(1.65 + Math.sin(x / 3) * 0.5);
    }
    return [
      { name: "volts", values: v1, unit: "V" },
      { name: "avg", values: v2, unit: "V" },
    ];
  }, [tick]);

  const states: ConnectionState[] = ["idle", "scanning", "connected", "compiling", "flashing", "error"];

  return (
    <div className="gallery">
      <div className="gallery-bar">
        <strong>Forge Komponenten</strong>
        <span style={{ flex: 1 }} />
        <Button size="sm" onClick={toggleTheme}>{`Thema: ${resolved === "dark" ? "Dunkel" : "Hell"}`}</Button>
        <Button size="sm" variant="ghost" onClick={() => setThemePreference("system")}>
          System
        </Button>
      </div>
      <div className="gallery-inner">
        <h1>Komponenten</h1>

        <Section title="Buttons">
          <div className="row">
            <Button variant="primary" icon="flash" shortcut="⌘U">Flash</Button>
            <Button icon="build" shortcut="⌘B">Kompilieren</Button>
            <Button variant="ghost" icon="serial">Monitor öffnen</Button>
            <Button variant="danger" icon="erase">Flash löschen</Button>
            <Button disabled icon="flash">Kein Board</Button>
            <Button variant="primary" size="lg" icon="flash" busy={busy} onClick={() => setBusy(!busy)}>{busy ? "Flashe …" : "Flash (klick mich)"}</Button>
            <Button size="sm" icon="refresh">Ports suchen</Button>
          </div>
        </Section>

        <Section title="Felder">
          <div className="grid">
            <Input label="Name" placeholder="neopixel-sensor" mono />
            <Input label="Name" defaultValue="mit leerzeichen" error="Leerzeichen ist im Namen nicht erlaubt." mono />
            <Select label="Board" options={["ESP32-S3 Dev Module", "ESP32 Dev Module"]} />
            <Checkbox label="Git-Repository anlegen" defaultChecked />
          </div>
        </Section>

        <Section title="Status">
          <div className="row">
            {states.map((s) => <StatusDot key={s} state={s} />)}
            <Badge tone="success" icon="check">Installiert</Badge>
            <Badge tone="warning" icon="warning">Wird knapp</Badge>
            <Badge tone="error" icon="error">Fehler</Badge>
            <Badge tone="info" icon="info">Hinweis</Badge>
            <Badge tone="glut" mono>belegt: SENSOR</Badge>
            <Kbd keys={["⌘", "K"]} />
          </div>
        </Section>

        <Section title="Tabs und Baum">
          <div className="grid">
            <div className="box">
              <Tabs items={[{ id: "a", label: "sketch.ino", icon: "file-code", problems: true }, { id: "b", label: "config.h", icon: "file-code", dirty: true }, { id: "c", label: "sensor.cpp", icon: "file-code" }]} onClose={() => undefined} />
            </div>
            <div className="box">
              <FileTree nodes={[{ name: "neopixel-sensor", open: true, children: [{ name: "sketch.ino", icon: "file-code", status: "error", badge: "!" }, { name: "config.h", icon: "file-code", status: "modified", badge: "M" }, { name: "data", children: [{ name: "index.html" }] }] }]} defaultSelected="/neopixel-sensor/config.h" />
            </div>
          </div>
        </Section>

        <Section title="Editor">
          <div className="box" style={{ height: 380 }}>
            <ForgeEditor docKey="gallery" text={SAMPLE_SKETCH} revision={0} diagnostics={DIAG} />
          </div>
        </Section>

        <Section title="Monitor und Plotter">
          <div className="grid" style={{ gridTemplateColumns: "1fr 1fr" }}>
            <div className="box" style={{ height: 260 }}>
              <SerialMonitor port="/dev/cu.usbmodem1101" lines={Array.from({ length: 12 }, (_, i) => ({ id: i, t: 1200 + i * 120, kind: i === 0 ? ("sys" as const) : ("rx" as const), text: i === 0 ? "Verbunden mit /dev/cu.usbmodem1101 bei 115200 baud." : `raw:${2000 + i * 17} volts:${(1.6 + i * 0.01).toFixed(2)}` }))} />
            </div>
            <div className="box" style={{ height: 260 }}>
              <Plotter series={series} min={0} max={3.3} interval={120} onPause={() => undefined} />
            </div>
          </div>
        </Section>

        <Section title="Build und Speicher">
          <div className="grid">
            <BuildProgress title="Flashe neopixel-sensor" target="ESP32-S3" stage={4} progress={0.42} detail="Schreibe 0x10000" />
            <BuildProgress title="Kompilieren" status="error" stage={0} progress={1} detail="sketch.ino, Zeile 22" />
            <MemoryMeter items={[{ label: "Flash", used: 742_000, total: 1_310_720 }, { label: "RAM", used: 23_800, total: 327_680 }, { label: "Partition app0", used: 1_280_000, total: 1_310_720 }]} />
            <PartitionMap flashSize={0x1000000} partitions={[{ name: "nvs", type: "data", offset: 0x9000, size: 0x5000 }, { name: "app0", type: "app", offset: 0x10000, size: 0x640000, used: 0x2a000 }, { name: "spiffs", type: "data", offset: 0x650000, size: 0x300000 }]} />
          </div>
        </Section>

        <Section title="Pinout">
          <div className="box" style={{ maxWidth: 420, padding: 12 }}>
            <Pinout used={{ 4: "SENSOR", 48: "LED" }} />
          </div>
        </Section>

        <Section title="Bibliotheken">
          <div className="grid">
            <LibraryItem name="Adafruit NeoPixel" version="1.12.3" author="Adafruit" downloads="412 k" description="Steuert WS2812-LEDs und kompatible Streifen." state="installed" />
            <LibraryItem name="ArduinoJson" version="7.0.4" latest="7.2.1" author="Benoit Blanchon" state="update" />
            <LibraryItem name="NimBLE-Arduino" version="2.5.1" author="h2zero" state="available" />
          </div>
        </Section>

        <Section title="Überlagerungen">
          <div className="grid" style={{ alignItems: "start" }}>
            <Toast kind="error" title="Kompilieren fehlgeschlagen" message="sketch.ino, Zeile 22: ';' erwartet vor '}'." onClose={() => undefined} />
            <Toast kind="success" title="Flash abgeschlossen" message="742 KB in 8.4 s geschrieben und geprüft." />
            <Toast kind="progress" title="Flashe auf ESP32-S3" message="Schreibe 0x10000" progress={0.55} />
          </div>
          <div className="row" style={{ marginTop: 16, alignItems: "flex-start" }}>
            <Dialog inline title="Änderungen an sketch.ino speichern?" icon="warning" tone="warning" footer={<><Button variant="ghost">Verwerfen</Button><span className="fg-spacer" /><Button>Abbrechen</Button><Button variant="primary">Speichern</Button></>}>
              <p>Ohne Speichern gehen die Änderungen verloren.</p>
            </Dialog>
            <CommandPalette items={[{ id: "1", label: "Kompilieren und flashen", icon: "flash", shortcut: ["⌘", "U"], group: "Zuletzt" }, { id: "2", label: "Seriellen Monitor öffnen", icon: "serial", group: "Zuletzt" }, { id: "3", label: "Board wählen …", icon: "chip", hint: "ESP32-S3", group: "Board" }]} />
          </div>
        </Section>

        <Section title="Kunst">
          <div className="grid">
            <div className="box" style={{ height: 200 }}><Traces seed={3} width={320} height={200} count={9} lit={2} /></div>
            <div className="box" style={{ height: 200 }}><EmptyState icon="chip" title="Bereit zum Flashen" text="Öffne ein Projekt oder lege einen Sketch an." height={200} seed={9} /></div>
            <Splash progress={0.6} status="Starte …" />
          </div>
        </Section>

        <Section title={`Icons (${ICON_NAMES.length})`}>
          <div className="row" style={{ gap: 18 }}>
            {ICON_NAMES.map((n) => (
              <span key={n} title={n} style={{ color: "var(--ink-muted)" }}>
                <Icon name={n} size={20} />
              </span>
            ))}
          </div>
        </Section>
      </div>
    </div>
  );
}
