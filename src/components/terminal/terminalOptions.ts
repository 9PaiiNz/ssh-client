import type { ITerminalOptions } from "@xterm/xterm";

export const TERMINAL_BACKGROUND = "#1a1a1e";

/** Shared look of live terminals and the history viewer. */
export const terminalOptions: ITerminalOptions = {
  fontSize: 14,
  fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
  // Enough lines to hold the ~1 MB of output the backend keeps per session.
  scrollback: 10000,
  theme: {
    background: TERMINAL_BACKGROUND,
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
};
