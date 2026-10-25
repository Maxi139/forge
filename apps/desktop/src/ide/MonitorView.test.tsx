import { SerialMonitor } from "@forge/ui";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

describe("SerialMonitor", () => {
  it("blättert mit ↑/↓ durch den Sendeverlauf und sendet CR allein", () => {
    const onSend = vi.fn();
    render(<SerialMonitor lines={[]} onSend={onSend} />);
    const input = screen.getByLabelText("Nachricht senden") as HTMLInputElement;
    fireEvent.change(screen.getByLabelText("Zeilenende"), { target: { value: "cr" } });
    for (const cmd of ["eins", "zwei"]) {
      fireEvent.change(input, { target: { value: cmd } });
      fireEvent.keyDown(input, { key: "Enter" });
    }
    expect(onSend).toHaveBeenLastCalledWith("zwei", "cr");
    fireEvent.change(input, { target: { value: "ent" } });
    fireEvent.keyDown(input, { key: "ArrowUp" });
    expect(input.value).toBe("zwei");
    fireEvent.keyDown(input, { key: "ArrowUp" });
    expect(input.value).toBe("eins");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(input.value).toBe("ent");
  });

  it("filtert nach Text und nur Fehler", () => {
    const lines = [
      { id: 1, kind: "rx" as const, text: "alles gut" },
      { id: 2, kind: "err" as const, text: "E (1) kaputt" },
    ];
    render(<SerialMonitor lines={lines} />);
    fireEvent.click(screen.getByLabelText("Filter und Suche"));
    fireEvent.change(screen.getByLabelText("Ausgabe filtern"), { target: { value: "gut" } });
    expect(screen.queryByText("E (1) kaputt")).toBeNull();
    expect(screen.getByText("alles gut")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Ausgabe filtern"), { target: { value: "" } });
    fireEvent.click(screen.getByText("Nur Fehler"));
    expect(screen.queryByText("alles gut")).toBeNull();
    expect(screen.getByText("E (1) kaputt")).toBeInTheDocument();
  });

  it("übernimmt eine frei eingegebene Baudrate und löst den Reset aus", () => {
    const onBaud = vi.fn();
    const onReset = vi.fn();
    render(<SerialMonitor lines={[]} baud={115200} onBaud={onBaud} onReset={onReset} />);
    const baud = screen.getByLabelText("Baudrate");
    fireEvent.change(baud, { target: { value: "76800" } });
    fireEvent.keyDown(baud, { key: "Enter" });
    expect(onBaud).toHaveBeenCalledWith(76800);
    fireEvent.change(baud, { target: { value: "12" } });
    fireEvent.blur(baud);
    expect(onBaud).toHaveBeenCalledTimes(1);
    expect((baud as HTMLInputElement).value).toBe("115200");
    fireEvent.click(screen.getByLabelText("Board neu starten"));
    expect(onReset).toHaveBeenCalled();
  });

  it("löst einen Backtrace auf und springt zur Stelle", async () => {
    const frame = { addr: "0x400d1234", function: "loop()", file: "/p/a.ino", line: 12, openable: true };
    const onDecode = vi.fn().mockResolvedValue([frame]);
    const onOpenFrame = vi.fn();
    const lines = [{ id: 1, kind: "rx" as const, text: "Backtrace: 0x400d1234:0x3ffb1f50" }, { id: 2, kind: "rx" as const, text: "normal" }];
    render(<SerialMonitor lines={lines} canDecode={(l) => l.text.startsWith("Backtrace")} onDecode={onDecode} onOpenFrame={onOpenFrame} />);
    expect(screen.getAllByText("Auflösen")).toHaveLength(1);
    fireEvent.click(screen.getByText("Auflösen"));
    const loc = await screen.findByText("a.ino:12");
    fireEvent.click(loc);
    expect(onOpenFrame).toHaveBeenCalledWith(frame);
    expect(screen.getByText("loop()")).toBeInTheDocument();
  });

  it("zeigt Fehler beim Auflösen", async () => {
    const onDecode = vi.fn().mockRejectedValue(new Error("Es gibt noch keinen Build."));
    render(<SerialMonitor lines={[{ id: 1, text: "Backtrace: 0x1" }]} canDecode={() => true} onDecode={onDecode} />);
    fireEvent.click(screen.getByText("Auflösen"));
    await waitFor(() => expect(screen.getByText("Es gibt noch keinen Build.")).toBeInTheDocument());
  });
});
