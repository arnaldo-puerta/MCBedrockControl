import { useState } from "react";
import { Console } from "./components/Console";
import { Dashboard } from "./components/Dashboard";
import { Players } from "./components/Players";
import { Setup } from "./components/Setup";
import { api } from "./lib/api";
import { useServer } from "./lib/useServer";

type Page = "server" | "console" | "players";

const PAGES: { id: Page; label: string }[] = [
  { id: "server", label: "Servidor" },
  { id: "console", label: "Consola" },
  { id: "players", label: "Jugadores" },
];

function App() {
  const { snapshot, error, run, selectFolder, install, subscribeLogs } = useServer();
  const [page, setPage] = useState<Page>("server");
  const [changingServer, setChangingServer] = useState(false);

  if (!snapshot) {
    return error ? <p role="alert" className="p-6 text-danger">{error}</p> : null;
  }
  if (!snapshot.serverPath || changingServer) {
    return (
      <Setup
        error={error}
        onSelect={selectFolder}
        onInstall={install}
        onDone={() => setChangingServer(false)}
        onBack={snapshot.serverPath ? () => setChangingServer(false) : undefined}
      />
    );
  }

  return (
    <div className="flex h-full">
      <nav className="flex w-52 shrink-0 flex-col border-r border-line bg-sidebar">
        <div className="flex items-center gap-3 p-4">
          <img src="/logo.png" alt="" className="size-9" />
          <div className="leading-tight">
            <p className="text-sm font-bold">MC Bedrock</p>
            <p className="text-xs text-grass-light">Control</p>
          </div>
        </div>
        {PAGES.map(({ id, label }) => (
          <button
            key={id}
            onClick={() => setPage(id)}
            aria-current={page === id ? "page" : undefined}
            className={`border-l-3 px-4 py-3 text-left text-sm ${
              page === id
                ? "border-grass bg-panel text-grass-light"
                : "border-transparent text-muted hover:text-ink"
            }`}
          >
            {label}
          </button>
        ))}
      </nav>

      <main className="relative min-w-0 flex-1">
        {error && (
          <p role="alert" className="absolute inset-x-6 top-3 z-10 rounded-lg bg-danger px-4 py-2 text-sm">
            {error}
          </p>
        )}
        <div className={page === "server" ? "h-full overflow-y-auto" : "hidden"}>
          <Dashboard
            snapshot={snapshot}
            onStart={() => run(api.start)}
            onStop={() => run(api.stop)}
            onRestart={() => run(api.restart)}
            onChangeServer={() => setChangingServer(true)}
          />
        </div>
        {page === "players" && (
          <div className="h-full overflow-y-auto">
            <Players state={snapshot.server.state} players={snapshot.server.players} run={run} />
          </div>
        )}
        {/* Stays mounted while hidden so the terminal keeps its scrollback. */}
        <div className={page === "console" ? "h-full" : "hidden"}>
          <Console
            state={snapshot.server.state}
            visible={page === "console"}
            subscribeLogs={subscribeLogs}
            onSend={(command) => run(() => api.sendCommand(command))}
          />
        </div>
      </main>
    </div>
  );
}

export default App;
