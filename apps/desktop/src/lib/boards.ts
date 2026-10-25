export interface BoardOptionEntry {
  fqbn: string;
  name: string;
  chip: string;
}

/** Fest eingebaute Auswahl, bis die Boardliste aus arduino-cli kommt. */
export const FALLBACK_BOARDS: BoardOptionEntry[] = [
  { fqbn: "esp32:esp32:esp32s3", name: "ESP32-S3 Dev Module", chip: "ESP32-S3" },
  { fqbn: "esp32:esp32:esp32", name: "ESP32 Dev Module", chip: "ESP32" },
  { fqbn: "esp32:esp32:esp32s2", name: "ESP32-S2 Dev Module", chip: "ESP32-S2" },
  { fqbn: "esp32:esp32:esp32c3", name: "ESP32-C3 Dev Module", chip: "ESP32-C3" },
  { fqbn: "esp32:esp32:esp32c6", name: "ESP32-C6 Dev Module", chip: "ESP32-C6" },
  { fqbn: "esp32:esp32:esp32h2", name: "ESP32-H2 Dev Module", chip: "ESP32-H2" },
  { fqbn: "esp32:esp32:esp32p4", name: "ESP32-P4 Dev Module", chip: "ESP32-P4" },
  { fqbn: "esp32:esp32:XIAO_ESP32S3", name: "XIAO ESP32S3", chip: "ESP32-S3" },
];

export function boardName(fqbn: string): string {
  return FALLBACK_BOARDS.find((b) => b.fqbn === fqbn)?.name ?? fqbn.split(":").pop() ?? fqbn;
}

export function chipOf(fqbn: string): string {
  return FALLBACK_BOARDS.find((b) => b.fqbn === fqbn)?.chip ?? "ESP32";
}


/** Chipfamilie aus dem Namen, den esptool meldet: `ESP32-D0WD-V3` wird `ESP32`, `ESP32-S3` bleibt. */
export function chipFamily(chip: string): string {
  const m = /^ESP32-(S2|S3|C2|C3|C5|C6|H2|P4)/i.exec(chip.trim());
  return m ? `ESP32-${m[1].toUpperCase()}` : "ESP32";
}

/** Familie, die ein FQBN meint. `null`, wenn der Name keinen Chip verrät. */
export function familyOfFqbn(fqbn: string): string | null {
  const board = (fqbn.split(":")[2] ?? "").toLowerCase();
  const m = /esp32(s2|s3|c2|c3|c5|c6|h2|p4)/.exec(board);
  if (m) return `ESP32-${m[1].toUpperCase()}`;
  if (board === "esp32" || board.startsWith("esp32_") || board === "esp32da" || board === "esp32wrover" || board === "esp32doit-devkit-v1") return "ESP32";
  return null;
}

/** Das Standardboard für einen erkannten Chip. */
export function boardForChip(chip: string): BoardOptionEntry {
  const family = chipFamily(chip);
  return FALLBACK_BOARDS.find((b) => b.chip === family && !b.fqbn.includes("XIAO")) ?? FALLBACK_BOARDS[1];
}


