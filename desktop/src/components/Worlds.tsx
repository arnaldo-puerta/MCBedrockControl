import { useEffect, useState } from "react";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { api, type AppSnapshot, type ServerState, type WorldInfo } from "../lib/api";
import { formatBytes, PRIMARY_BUTTON, SMALL_BUTTON } from "../lib/format";
import { PageHeader } from "../lib/pages";

interface Props {
  state: ServerState;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  adopt: (snapshot: AppSnapshot) => void;
}

export function Worlds({ state, run, adopt }: Props) {
  const [worlds, setWorlds] = useState<WorldInfo[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const stopped = state === "stopped" || state === "crashed";

  useEffect(() => {
    run(async () => setWorlds(await api.worlds()));
  }, [run, state]);

  async function task(label: string, action: () => Promise<unknown>) {
    setBusy(label);
    await run(action);
    setBusy(null);
  }

  async function importWorld() {
    const path = await open({
      title: "Importar mundo",
      filters: [{ name: "Mundos de Minecraft", extensions: ["mcworld", "zip"] }],
    });
    if (path) await task("Importando mundo…", async () => setWorlds(await api.importWorld(path)));
  }

  async function exportWorld(world: WorldInfo) {
    const destination = await save({
      title: "Exportar mundo",
      defaultPath: `${world.name}.mcworld`,
      filters: [{ name: "Mundo de Minecraft", extensions: ["mcworld"] }],
    });
    if (destination) await task("Exportando mundo…", () => api.exportWorld(world.folder, destination));
  }

  async function deleteWorld(world: WorldInfo) {
    const confirmed = await ask(
      `Se borrará "${world.name}". Antes se guardará una copia de seguridad automática.`,
      { title: "Borrar mundo", kind: "warning", okLabel: "Borrar", cancelLabel: "Cancelar" },
    );
    if (confirmed) await task("Guardando copia y borrando…", async () => setWorlds(await api.deleteWorld(world.folder)));
  }

  async function activate(world: WorldInfo) {
    await task("Cambiando de mundo…", async () => {
      adopt(await api.setActiveWorld(world.folder));
      setWorlds(await api.worlds());
    });
  }

  return (
    <section className="space-y-4 p-6">
      <PageHeader page="worlds">
        <button onClick={importWorld} disabled={!!busy} className={PRIMARY_BUTTON}>
          Importar .mcworld
        </button>
      </PageHeader>
      <p className="text-xs text-muted">
        El servidor carga el mundo activo. Importar nunca reemplaza un mundo existente.
      </p>

      {busy && <p role="status" className="text-sm text-amber">{busy}</p>}
      {!stopped && (
        <p className="text-sm text-muted">Detén el servidor para cambiar el mundo activo.</p>
      )}

      {worlds && worlds.length === 0 && (
        <p className="rounded-xl border border-line bg-panel p-4 text-sm text-muted">
          Aún no hay mundos. Inicia el servidor una vez para crear uno, o importa un .mcworld.
        </p>
      )}

      <ul className="space-y-2">
        {worlds?.map((world) => (
          <li key={world.folder} className="flex flex-wrap items-center gap-2 rounded-xl border border-line bg-panel p-4">
            <div className="mr-auto min-w-0">
              <p className="truncate font-semibold">
                {world.name}
                {world.active && (
                  <span className="ml-2 rounded-full bg-grass/30 px-2 py-0.5 text-xs text-grass-light">Activo</span>
                )}
              </p>
              <p className="truncate text-xs text-muted">
                {formatBytes(world.sizeBytes)}
                {world.name !== world.folder && ` · carpeta ${world.folder}`}
              </p>
            </div>
            {!world.active && (
              <button onClick={() => activate(world)} disabled={!stopped || !!busy} className={SMALL_BUTTON}>
                Activar
              </button>
            )}
            <button onClick={() => exportWorld(world)} disabled={!!busy} className={SMALL_BUTTON}>
              Exportar
            </button>
            {!world.active && (
              <button onClick={() => deleteWorld(world)} disabled={!!busy} className={SMALL_BUTTON}>
                Borrar
              </button>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
