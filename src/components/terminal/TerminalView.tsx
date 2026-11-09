import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import {
  getScrollback,
  historyEnabled,
  listHistory,
  readHistory,
  onSessionStatus,
  onTerminalOutput,
  resizeTerminal,
  writeTerminal,
  type TerminalChunk,
} from "@/lib/api";
import { useSessionStore } from "@/stores/useSessionStore";
import { TERMINAL_BACKGROUND, terminalOptions } from "./terminalOptions";

interface TerminalViewProps {
  sessionId: string;
  /** Saved profile this session belongs to; its last session is shown first. */
  profileId?: string;
  /** Whether this is the visible tab. Hidden terminals stay mounted. */
  active: boolean;
}

export function TerminalView({ sessionId, profileId, active }: TerminalViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const updateTabStatus = useSessionStore((s) => s.updateTabStatus);

  useEffect(() => {
    if (!containerRef.current) return;
    const container = containerRef.current;

    const terminal = new Terminal({ ...terminalOptions, cursorBlink: true });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(new WebLinksAddon());
    terminal.open(container);
    terminalRef.current = terminal;

    const fitAndResize = () => {
      // Hidden tabs have no size; fit them when they are shown again.
      if (container.clientWidth === 0 || container.clientHeight === 0) return;
      fitAddon.fit();
      resizeTerminal(sessionId, terminal.cols, terminal.rows).catch(() => {});
    };
    fitAndResize();

    const resizeObserver = new ResizeObserver(fitAndResize);
    resizeObserver.observe(container);

    const dataDisposable = terminal.onData((data) => {
      writeTerminal(sessionId, data).catch(() => {});
    });

    // Output may arrive before this view exists (the login banner) or while
    // the listener is being set up. Queue live chunks until the backend's
    // scrollback is drawn, then only write chunks newer than it.
    let disposed = false;
    let drawnUpTo: number | null = null;
    const queued: TerminalChunk[] = [];
    const writeChunk = (chunk: TerminalChunk) => {
      if (drawnUpTo === null) {
        queued.push(chunk);
      } else if (chunk.seq > drawnUpTo) {
        terminal.write(chunk.data);
        drawnUpTo = chunk.seq;
      }
    };

    const unlistenOutput = onTerminalOutput(sessionId, writeChunk);
    unlistenOutput
      .then(() => drawPreviousSession(terminal, profileId, () => disposed))
      .then(() => getScrollback(sessionId))
      .then(
        (snapshot) => {
          if (disposed) return;
          terminal.write(snapshot.data);
          drawnUpTo = snapshot.seq;
        },
        () => {
          drawnUpTo = 0;
        }
      )
      .finally(() => {
        if (disposed) return;
        for (const chunk of queued.splice(0)) writeChunk(chunk);
      });

    const unlistenStatus = onSessionStatus(sessionId, (status, error) => {
      updateTabStatus(
        sessionId,
        status as "connecting" | "connected" | "disconnected" | "error",
        error
      );
      if (status === "disconnected") {
        terminal.writeln("\r\n\x1b[38;5;8m[Session disconnected]\x1b[0m");
      }
    });

    return () => {
      disposed = true;
      dataDisposable.dispose();
      unlistenOutput.then((fn) => fn());
      unlistenStatus.then((fn) => fn());
      resizeObserver.disconnect();
      terminal.dispose();
      terminalRef.current = null;
    };
  }, [sessionId, profileId, updateTabStatus]);

  useEffect(() => {
    if (active) terminalRef.current?.focus();
  }, [active]);

  return (
    <div
      ref={containerRef}
      className="h-full w-full p-1"
      style={{ background: TERMINAL_BACKGROUND }}
    />
  );
}

/**
 * Write the output of the profile's most recent ended session above the new
 * one, so reconnecting shows where you left off. Never fails.
 */
async function drawPreviousSession(
  terminal: Terminal,
  profileId: string | undefined,
  isDisposed: () => boolean
): Promise<void> {
  if (!profileId) return;
  try {
    if (!(await historyEnabled())) return;
    const [last] = await listHistory(profileId);
    if (!last) return;
    const text = await readHistory(profileId, last.id);
    if (isDisposed()) return;
    const ended = new Date(last.endedAt).toLocaleString();
    await writeAsync(terminal, text);
    if (isDisposed()) return;

    // If the old session ended inside a full-screen program (vim, htop),
    // switch back to the normal screen.
    if (terminal.buffer.active.type === "alternate") {
      await writeAsync(terminal, "\x1b[?1049l");
    }
    // The old output may have moved the cursor up (cleared screen, progress
    // bars); move it below the last written line so nothing is overwritten.
    const buf = terminal.buffer.active;
    let lastLine = buf.length - 1;
    while (lastLine > 0 && !buf.getLine(lastLine)?.translateToString(true).trim()) {
      lastLine--;
    }
    const down = lastLine - (buf.baseY + buf.cursorY);
    const moveDown = down > 0 ? `\x1b[${down}B` : "";

    // Reset colours, show the cursor and mark where the new session starts.
    await writeAsync(
      terminal,
      `\x1b[0m\x1b[?25h${moveDown}\r\n` +
        `\x1b[38;5;8m──── previous session ended ${ended} ────\x1b[0m\r\n`
    );
  } catch {
    // History is a convenience; the live session works without it.
  }
}

function writeAsync(terminal: Terminal, data: string): Promise<void> {
  return new Promise((resolve) => terminal.write(data, resolve));
}
