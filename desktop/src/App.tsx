import { useCallback, useEffect, useState } from "react";
import { BackupsPage } from "./components/BackupsPage";
import { Connections } from "./components/Connections";
import { Console } from "./components/Console";
import { Dashboard } from "./components/Dashboard";
import { PacksPage } from "./components/PacksPage";
import { Players } from "./components/Players";
import { Setup } from "./components/Setup";
import { SettingsPage } from "./components/SettingsPage";
import { UpdatePage } from "./components/UpdatePage";
import { Worlds } from "./components/Worlds";
import { api, type UpdateCheck } from "./lib/api";
import { PAGE_ORDER, PAGES, type Page } from "./lib/pages";
import { useServer } from "./lib/useServer";

function App() {
  const { snapshot, error, run, adopt, selectFolder, install, subscribeLogs } = useServer();
  const [page, setPage] = useState<Page>("server");
  const [changingServer, setChangingServer] = useState(false);
  const [update, setUpdate] = useState<UpdateCheck | null>(null);
  const [updateError, setUpdateError] = useState<string | null>(null);

  // Checked quietly: being offline shouldn't put an error banner on every page.
  const checkUpdate = useCallback(async () => {
    setUpdate(null);
    try {
      setUpdate(await api.checkUpdate());
      setUpdateError(null);
    } catch (e) {
      setUpdateError(String(e));
    }
  }, []);

  const serverPath = snapshot?.serverPath;
  const version = snapshot?.version;
  useEffect(() => {
    if (serverPath) checkUpdate();
  }, [serverPath, version, checkUpdate]);

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

  const updateAvailable = update?.available === true;

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
        {PAGE_ORDER.map((id) => {
          const { label, icon: Icon } = PAGES[id];
          return (
          <button
            key={id}
            onClick={() => setPage(id)}
            aria-current={page === id ? "page" : undefined}
            className={`flex items-center gap-3 border-l-3 px-4 py-2.5 text-left text-sm ${
              page === id
                ? "border-grass bg-panel text-grass-light"
                : "border-transparent text-muted hover:text-ink"
            }`}
          >
            <Icon className="size-4 shrink-0" aria-hidden />
            {label}
            {id === "update" && updateAvailable && (
              <span className="size-2 rounded-full bg-amber" aria-label="Actualización disponible" />
            )}
          </button>
          );
        })}
      </nav>

      <main className="relative min-w-0 flex-1">
        {error && (
          <p role="alert" className="absolute inset-x-6 top-3 z-10 rounded-lg bg-danger px-4 py-2 text-sm">
            {error}
          </p>
        )}
        <div className={page === "server" ? "h-full overflow-y-auto" : "hidden"}>
          {updateAvailable && update.current && (
            <button
              onClick={() => setPage("update")}
              className="mx-6 mt-6 block w-[calc(100%-3rem)] rounded-xl border border-amber/50 bg-amber/10 p-3 text-left text-sm text-amber"
            >
              Hay una versión nueva del servidor: {update.latest}. Pulsa aquí para actualizar sin perder nada.
            </button>
          )}
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
        {page === "connections" && (
          <div className="h-full overflow-y-auto">
            <Connections run={run} />
          </div>
        )}
        {page === "worlds" && (
          <div className="h-full overflow-y-auto">
            <Worlds state={snapshot.server.state} run={run} adopt={adopt} />
          </div>
        )}
        {page === "packs" && (
          <div className="h-full overflow-y-auto">
            <PacksPage state={snapshot.server.state} run={run} />
          </div>
        )}
        {page === "backups" && (
          <div className="h-full overflow-y-auto">
            <BackupsPage state={snapshot.server.state} run={run} adopt={adopt} />
          </div>
        )}
        {page === "settings" && (
          <div className="h-full overflow-y-auto">
            <SettingsPage state={snapshot.server.state} run={run} adopt={adopt} />
          </div>
        )}
        {page === "update" && (
          <div className="h-full overflow-y-auto">
            <UpdatePage
              state={snapshot.server.state}
              check={update}
              checkError={updateError}
              onRecheck={checkUpdate}
              run={run}
              adopt={adopt}
            />
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
