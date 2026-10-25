import { useEffect, useState } from "react";

/**
 * Die KI arbeitet: Ein Glutpuls läuft von der Aufgabe durch den Chip zum Code. Das ist das Bild für „Daten fließen“,
 * ruhig und ohne Text im Weg. Bei reduzierter Bewegung steht die Platine still.
 */
export function AiScene({ label }: { label: string }) {
  return (
    <svg className="ai-scene" viewBox="0 0 360 72" role="img" aria-label={label}>
      <path className="t" d="M18 36H124" />
      <path className="t" d="M236 36H342" />
      <path className="t" d="M70 36V20H100" />
      <path className="t" d="M260 36V52H290" />
      <circle className="pad" cx="14" cy="36" r="5" />
      <circle className="pad" cx="346" cy="36" r="5" />
      <circle className="dot" cx="100" cy="20" r="2.5" />
      <circle className="dot" cx="290" cy="52" r="2.5" />
      {[148, 168, 188, 208].map((x) => (
        <g key={x}>
          <path className="pin" d={`M${x} 14v-6M${x} 58v6`} />
        </g>
      ))}
      <rect className="chip" x="124" y="14" width="112" height="44" rx="6" />
      <text className="chip-label" x="180" y="40" textAnchor="middle">
        KI
      </text>
      <path className="lit lit-in" pathLength={120} d="M18 36H124" />
      <path className="lit lit-out" pathLength={120} d="M236 36H342" />
    </svg>
  );
}

/** Klein, für Zeilen und Buttons: Pad, Bahn, Pad mit einem Puls dazwischen. */
export function AiPulse({ label }: { label?: string }) {
  return (
    <svg className="ai-pulse" viewBox="0 0 44 12" role={label ? "img" : undefined} aria-label={label} aria-hidden={label ? undefined : true}>
      <path className="t" d="M5 6H39" />
      <circle className="pad" cx="4" cy="6" r="3" />
      <circle className="pad" cx="40" cy="6" r="3" />
      <path className="lit" pathLength={120} d="M5 6H39" />
    </svg>
  );
}

/** Vergangene Sekunden seit dem Einblenden, als „8.4 s“. */
export function Elapsed() {
  const [ms, setMs] = useState(0);
  useEffect(() => {
    const start = performance.now();
    const id = setInterval(() => setMs(performance.now() - start), 100);
    return () => clearInterval(id);
  }, []);
  return <span className="ai-elapsed">{`${(ms / 1000).toFixed(1)} s`}</span>;
}
