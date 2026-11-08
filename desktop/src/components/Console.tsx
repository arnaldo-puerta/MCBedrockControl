import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import type { LogLevel, LogLine, ServerState } from "../lib/api";
import { PageHeader } from "../lib/pages";

const ANSI: Record<LogLevel, string> = {
  info: "\x1b[38;2;123;201;111m",
  warn: "\x1b[38;2;212;168;67m",
  error: "\x1b[38;2;201;79;79m",
  plain: "\x1b[38;2;208;208;208m",
};

function render(lines: LogLine[]) {
  return lines.map((line) => `${ANSI[line.level]}${line.text}\x1b[0m\r\n`).join("");
}

interface Props {
  state: ServerState;
  visible: boolean;
  subscribeLogs: (listener: (lines: LogLine[]) => void) => () => void;
  onSend: (command: string) => void;
}

export function Console({ state, visible, subscribeLogs, onSend }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const fit = useRef<FitAddon | null>(null);
  const [command, setCommand] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [cursor, setCursor] = useState<number | null>(null);
  const running = state === "running";

  useEffect(() => {
    const terminal = new Terminal({
      convertEol: false,
      disableStdin: true,
      cursorStyle: "bar",
      cursorInactiveStyle: "none",
      fontFamily: "Cascadia Mono, Consolas, monospace",
      fontSize: 13,
      scrollback: 5000,
      theme: { background: "#1e1e1e", foreground: "#d0d0d0", cursor: "#1e1e1e" },
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(host.current!);
    fit.current = fitAddon;

    const unsubscribe = subscribeLogs((lines) => terminal.write(render(lines)));
    const observer = new ResizeObserver(() => {
      if (host.current?.offsetWidth) fitAddon.fit();
    });
    observer.observe(host.current!);

    return () => {
      observer.disconnect();
      unsubscribe();
      terminal.dispose();
      fit.current = null;
    };
  }, [subscribeLogs]);

  useEffect(() => {
    if (visible) fit.current?.fit();
  }, [visible]);

  function send() {
    const text = command.trim();
    if (!text) return;
    onSend(text);
    setHistory((items) => [...items, text]);
    setCursor(null);
    setCommand("");
  }

  function browseHistory(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    const last = history.length - 1;
    const next =
      event.key === "ArrowUp"
        ? Math.max((cursor ?? history.length) - 1, 0)
        : cursor === null || cursor >= last
          ? null
          : cursor + 1;
    if (history.length === 0) return;
    setCursor(next);
    setCommand(next === null ? "" : history[next]);
  }

  return (
    <section className="flex h-full flex-col gap-4 p-6">
      <PageHeader page="console" />
      <div className="min-h-0 flex-1 rounded-xl border border-line bg-panel p-3 select-text">
        <div ref={host} className="h-full" />
      </div>
      <form
        className="flex gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          send();
        }}
      >
        <input
          value={command}
          onChange={(event) => setCommand(event.target.value)}
          onKeyDown={browseHistory}
          disabled={!running}
          aria-label="Comando"
          placeholder={running ? "Escribe un comando, por ejemplo: list" : "Inicia el servidor para enviar comandos"}
          className="min-w-0 flex-1 rounded-lg border border-line bg-panel px-3 py-2.5 text-sm select-text disabled:opacity-50"
        />
        <button
          type="submit"
          disabled={!running || !command.trim()}
          className="rounded-lg bg-grass px-4 py-2.5 text-sm font-semibold hover:brightness-110 disabled:opacity-40"
        >
          Enviar
        </button>
      </form>
    </section>
  );
}
