import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import {
  onSessionStatus,
  onTerminalOutput,
  resizeTerminal,
  writeTerminal,
} from "@/lib/api";
import { useSessionStore } from "@/stores/useSessionStore";

interface TerminalViewProps {
  sessionId: string;
}

export function TerminalView({ sessionId }: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const updateTabStatus = useSessionStore((s) => s.updateTabStatus);

  useEffect(() => {
    if (!containerRef.current) return;

    const terminal = new Terminal({
      cursorBlink: true,
      fontSize: 14,
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
      theme: {
        background: "#1a1a1e",
        foreground: "#e8e8ed",
        cursor: "#2dd4bf",
        selectionBackground: "#2dd4bf33",
        black: "#1a1a1e",
        red: "#f87171",
        green: "#2dd4bf",
        yellow: "#fbbf24",
        blue: "#60a5fa",
        magenta: "#c084fc",
        cyan: "#2dd4bf",
        white: "#e8e8ed",
      },
    });

    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(new WebLinksAddon());
    terminal.open(containerRef.current);
    fitAddon.fit();
    terminalRef.current = terminal;
    fitAddonRef.current = fitAddon;

    const resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
      resizeTerminal(sessionId, terminal.cols, terminal.rows).catch(() => {});
    });
    resizeObserver.observe(containerRef.current);

    const dataDisposable = terminal.onData((data) => {
      writeTerminal(sessionId, data).catch(() => {});
    });

    let unlistenOutput: (() => void) | undefined;
    let unlistenStatus: (() => void) | undefined;

    onTerminalOutput(sessionId, (data) => {
      terminal.write(data);
    }).then((fn) => {
      unlistenOutput = fn;
    });

    onSessionStatus(sessionId, (status, error) => {
      updateTabStatus(
        sessionId,
        status as "connecting" | "connected" | "disconnected" | "error",
        error
      );
      if (status === "disconnected") {
        terminal.writeln("\r\n\x1b[38;5;8m[Session disconnected]\x1b[0m");
      }
    }).then((fn) => {
      unlistenStatus = fn;
    });

    resizeTerminal(sessionId, terminal.cols, terminal.rows).catch(() => {});

    return () => {
      dataDisposable.dispose();
      unlistenOutput?.();
      unlistenStatus?.();
      resizeObserver.disconnect();
      terminal.dispose();
      terminalRef.current = null;
      fitAddonRef.current = null;
    };
  }, [sessionId, updateTabStatus]);

  return (
    <div
      ref={containerRef}
      className="h-full w-full p-1"
      style={{ background: "#1a1a1e" }}
    />
  );
}
