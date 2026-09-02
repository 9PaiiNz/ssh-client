# SSH Client

A modern, clean cross-platform SSH client built with Tauri 2 and React. Terminal-first design inspired by tools like Tabby, but with a minimal UI that stays out of your way.

## Features

- **Connection profiles** — Save hosts with groups, password or SSH key authentication
- **Tabbed terminal sessions** — Multiple concurrent SSH sessions with xterm.js
- **SFTP file browser** — Browse, upload, and download files via a slide-out drawer
- **Port forwarding** — Local and remote TCP port forwarding per session
- **Cross-platform** — Windows and Linux support

## Tech Stack

| Layer | Technologies |
|-------|-------------|
| Frontend | React 19, TypeScript, Tailwind CSS, shadcn/ui, xterm.js, Zustand |
| Backend | Rust, Tauri 2, russh, russh-sftp, tokio |
| Tooling | Vite, GitHub Actions |

## Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) (stable toolchain)
- Platform dependencies for [Tauri](https://v2.tauri.app/start/prerequisites/):
  - **Windows:** Microsoft C++ Build Tools (Visual Studio Build Tools with the "Desktop development with C++" workload)
  - **Linux:** `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`, `patchelf`, `build-essential`

## Getting Started

```bash
# Install frontend dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Project Structure

```
src/                    React frontend
  components/           UI components (layout, terminal, connections, sftp)
  stores/               Zustand state management
  lib/                  Types and Tauri API wrappers
src-tauri/              Rust backend
  src/ssh/              SSH session management (russh)
  src/profiles/         Connection profile persistence
```

## License

MIT
