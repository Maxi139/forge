import { type Completion, type CompletionContext, type CompletionResult, snippet } from "@codemirror/autocomplete";

interface Entry {
  label: string;
  type: "function" | "constant" | "class" | "keyword" | "variable";
  detail?: string;
  info?: string;
}

const fn = (label: string, detail: string, info?: string): Entry => ({ label, type: "function", detail, info });
const k = (label: string): Entry => ({ label, type: "constant" });

const ENTRIES: Entry[] = [
  fn("pinMode", "(pin, mode)", "Legt fest, ob ein Pin Eingang oder Ausgang ist."),
  fn("digitalWrite", "(pin, value)", "Setzt einen Ausgang auf HIGH oder LOW."),
  fn("digitalRead", "(pin) → int", "Liest den Zustand eines Eingangs."),
  fn("analogRead", "(pin) → int", "Liest den ADC. Beim ESP32 sind es 12 Bit (0 bis 4095)."),
  fn("analogWrite", "(pin, value)", "PWM-Ausgabe mit 8 Bit."),
  fn("analogReadResolution", "(bits)", "Stellt die ADC-Auflösung ein."),
  fn("analogSetAttenuation", "(attenuation)", "Stellt den ADC-Messbereich ein."),
  fn("ledcAttach", "(pin, freq, resolution)", "Bindet einen Pin an einen PWM-Kanal."),
  fn("ledcWrite", "(pin, duty)", "Setzt das Tastverhältnis eines PWM-Pins."),
  fn("touchRead", "(pin) → int", "Liest einen kapazitiven Touch-Pin."),
  fn("delay", "(ms)", "Wartet die angegebene Zeit. Blockiert alles andere."),
  fn("delayMicroseconds", "(us)", "Wartet Mikrosekunden."),
  fn("millis", "() → unsigned long", "Millisekunden seit dem Start."),
  fn("micros", "() → unsigned long", "Mikrosekunden seit dem Start."),
  fn("map", "(value, fromLow, fromHigh, toLow, toHigh)", "Rechnet einen Wert von einem Bereich in einen anderen um."),
  fn("constrain", "(x, a, b)", "Begrenzt einen Wert auf [a, b]."),
  fn("min", "(a, b)"),
  fn("max", "(a, b)"),
  fn("abs", "(x)"),
  fn("sq", "(x)"),
  fn("sqrt", "(x)"),
  fn("pow", "(base, exponent)"),
  fn("random", "(max)"),
  fn("randomSeed", "(seed)"),
  fn("attachInterrupt", "(digitalPinToInterrupt(pin), isr, mode)", "Ruft isr bei einer Flanke auf. Die Funktion braucht IRAM_ATTR."),
  fn("detachInterrupt", "(interrupt)"),
  fn("digitalPinToInterrupt", "(pin)"),
  fn("tone", "(pin, frequency, duration)"),
  fn("noTone", "(pin)"),
  fn("pulseIn", "(pin, state, timeout)"),
  fn("yield", "()"),
  fn("Serial.begin", "(baud)", "Startet die serielle Schnittstelle."),
  fn("Serial.print", "(value)"),
  fn("Serial.println", "(value)"),
  fn("Serial.printf", "(format, ...)"),
  fn("Serial.available", "() → int"),
  fn("Serial.read", "() → int"),
  fn("Serial.readString", "() → String"),
  fn("Serial.readStringUntil", "(terminator) → String"),
  fn("Serial.write", "(value)"),
  fn("Serial.flush", "()"),
  fn("WiFi.begin", "(ssid, password)", "Verbindet mit einem WLAN."),
  fn("WiFi.status", "() → wl_status_t"),
  fn("WiFi.localIP", "() → IPAddress"),
  fn("WiFi.mode", "(mode)"),
  fn("WiFi.disconnect", "()"),
  fn("WiFi.scanNetworks", "() → int"),
  fn("WiFi.RSSI", "() → long"),
  fn("WiFi.softAP", "(ssid, password)"),
  fn("Wire.begin", "(sda, scl)"),
  fn("Wire.beginTransmission", "(address)"),
  fn("Wire.endTransmission", "() → uint8_t"),
  fn("Wire.requestFrom", "(address, count)"),
  fn("Wire.write", "(value)"),
  fn("Wire.read", "() → int"),
  fn("SPI.begin", "()"),
  fn("ESP.restart", "()"),
  fn("ESP.getFreeHeap", "() → uint32_t"),
  fn("xTaskCreate", "(task, name, stackSize, param, priority, handle)"),
  fn("vTaskDelay", "(ticks)"),
  fn("pdMS_TO_TICKS", "(ms)"),
  k("HIGH"),
  k("LOW"),
  k("INPUT"),
  k("OUTPUT"),
  k("INPUT_PULLUP"),
  k("INPUT_PULLDOWN"),
  k("LED_BUILTIN"),
  k("RISING"),
  k("FALLING"),
  k("CHANGE"),
  k("WL_CONNECTED"),
  k("WIFI_STA"),
  k("WIFI_AP"),
  k("PI"),
  { label: "String", type: "class" },
  { label: "IPAddress", type: "class" },
  { label: "WiFiClient", type: "class" },
  { label: "WebServer", type: "class" },
  { label: "Preferences", type: "class" },
  ...["void", "int", "float", "double", "char", "bool", "byte", "long", "short", "unsigned", "const", "static", "volatile", "return", "if", "else", "for", "while", "do", "switch", "case", "break", "continue", "uint8_t", "uint16_t", "uint32_t", "int8_t", "int16_t", "int32_t", "size_t"].map(
    (label): Entry => ({ label, type: "keyword" }),
  ),
];

/** Teilt "(a, f(b), c)" an den äußersten Kommas. "..." entfällt. */
function params(detail: string): string[] | null {
  const m = /^\((.*?)\)(?:\s*→.*)?$/.exec(detail);
  if (!m) return null;
  const out: string[] = [];
  let depth = 0;
  let cur = "";
  for (const ch of m[1]) {
    if (ch === "(") depth++;
    if (ch === ")") depth--;
    if (ch === "," && depth === 0) {
      out.push(cur.trim());
      cur = "";
    } else cur += ch;
  }
  if (cur.trim()) out.push(cur.trim());
  return out.filter((p) => p !== "...");
}

/** Funktionen fügen ihre Parameter als Tab-Stops ein. Tab springt weiter, zuletzt hinter den Aufruf. */
function applyFor(e: Entry): Completion["apply"] {
  if (e.type !== "function" || !e.detail) return undefined;
  const list = params(e.detail);
  if (!list) return undefined;
  if (!list.length) return `${e.label}()`;
  const fields = list.map((p, i) => `\${${i + 1}:${p.replace(/[{}]/g, "")}}`);
  return snippet(`${e.label}(${fields.join(", ")})\${0}`);
}

const OPTIONS: Completion[] = ENTRIES.map((e) => ({
  label: e.label,
  type: e.type,
  detail: e.detail,
  info: e.info,
  apply: applyFor(e),
  boost: e.type === "keyword" ? -2 : e.label.includes(".") ? -1 : 0,
}));

/**
 * Grundvervollständigung ohne Sprachserver: Arduino- und ESP32-Grundfunktionen plus Namen aus dem Dokument.
 * Sobald clangd bereit ist, liefert er genauere Vorschläge für die Bibliotheken des Projekts.
 */
export function arduinoCompletions(context: CompletionContext): CompletionResult | null {
  const word = context.matchBefore(/[A-Za-z_][\w]*(?:\.\w*)?/);
  if (!word || (word.from === word.to && !context.explicit)) return null;
  const local = new Map<string, Completion>();
  const text = context.state.doc.toString();
  for (const m of text.matchAll(/\b[A-Za-z_]\w{2,}\b/g)) {
    const name = m[0];
    if (!local.has(name) && name !== word.text && !OPTIONS.some((o) => o.label === name)) {
      local.set(name, { label: name, type: "variable", boost: -3 });
    }
  }
  return { from: word.from, options: [...OPTIONS, ...local.values()], validFor: /^[\w.]*$/ };
}
