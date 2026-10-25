/**
 * Findet die GPIOs, die ein Sketch benutzt. Das ist eine Textsuche, keine Analyse:
 * `#define`-Namen, die nach Pin klingen und im Code vorkommen, plus Aufrufe wie `pinMode(4, ...)`.
 */
const PIN_FUNCTIONS = /\b(?:pinMode|digitalWrite|digitalRead|analogRead|analogWrite|ledcAttach|touchRead|tone|noTone|pulseIn|digitalPinToInterrupt)\s*\(\s*([A-Za-z_]\w*|\d{1,2})\b/g;
const PIN_NAME = /PIN|LED|BTN|BUTTON|SDA|SCL|SCK|MISO|MOSI|CS\b|_CS|TX|RX|RELAY|BUZZER|SENSOR|TRIG|ECHO|SERVO|MOTOR|IRQ|INT\b/i;

const NOT_A_PIN = /^(NUM|COUNT|MAX|MIN|N)_|_(COUNT|NUM|MAX|MIN|SIZE|LEN|LENGTH)$/i;

function short(name: string): string {
  return name.replace(/_?PIN$/i, "").slice(0, 8).toUpperCase() || name.slice(0, 8).toUpperCase();
}

export function usedPins(text: string): Record<number, string> {
  const names = new Map<string, number>();
  for (const m of text.matchAll(/^\s*#define\s+([A-Za-z_]\w*)\s+(\d{1,2})\b/gm)) names.set(m[1], Number(m[2]));
  for (const m of text.matchAll(/\b(?:const\s+)?(?:u?int(?:8|16|32)?_t|int|byte)\s+([A-Za-z_]\w*)\s*=\s*(\d{1,2})\s*;/g)) names.set(m[1], Number(m[2]));

  const used: Record<number, string> = {};
  const mention = (name: string) => new RegExp(`\\b${name}\\b`, "g");
  for (const [name, pin] of names) {
    if (!PIN_NAME.test(name) || NOT_A_PIN.test(name)) continue;
    // Das Makro muss außer in seiner Definition noch vorkommen.
    if ((text.match(mention(name)) ?? []).length >= 2) used[pin] = short(name);
  }
  for (const m of text.matchAll(PIN_FUNCTIONS)) {
    const arg = m[1];
    const pin = /^\d+$/.test(arg) ? Number(arg) : names.get(arg);
    if (pin == null) continue;
    used[pin] ??= /^\d+$/.test(arg) ? "belegt" : short(arg);
  }
  return used;
}
