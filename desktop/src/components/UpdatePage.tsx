import { useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import { api, type AppSnapshot, type InstallProgress, type ServerState, type UpdateCheck } from "../lib/api";
import { PRIMARY_BUTTON, SMALL_BUTTON } from "../lib/format";
import { PageHeader } from "../lib/pages";

function progressLabel({ phase, done, total }: InstallProgress) {
  const mb = (bytes: number) => (bytes / (1024 * 1024)).toFixed(0);
  if (phase === "extract") return `Instalando archivos: ${done} de ${total}`;
  return total ? `Descargando: ${mb(done)} de ${mb(total)} MB` : `Descargando: ${mb(done)} MB`;
}

interface Props {
  state: ServerState;
  check: UpdateCheck | null;
  checkError: string | null;
  onRecheck: () => void;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  adopt: (snapshot: AppSnapshot) => void;
}

export function UpdatePage({ state, check, checkError, onRecheck, run, adopt }: Props) {
  const [updating, setUpdating] = useState(false);
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const stopped = state === "stopped" || state === "crashed";

  useEffect(() => {
    const unlisten = api.onInstallProgress(setProgress);
    return () => {
      unlisten.then((stop) => stop());
    };
  }, []);

  async function update() {
    const confirmed = await ask(
      `Se instalará la versión ${check?.latest}. Antes se guardará una copia de tu mundo. ` +
        "Tus mundos, ajustes, lista de permitidos y packs se conservan.",
      { title: "Actualizar servidor", okLabel: "Actualizar", cancelLabel: "Cancelar" },
    );
    if (!confirmed) return;
    setUpdating(true);
    setProgress(null);
    const ok = await run(async () => adopt(await api.updateServer()));
    setUpdating(false);
    if (ok) {
      setDone(check?.latest ?? null);
      onRecheck();
    }
  }

  const percent = progress?.total ? (progress.done / progress.total) * 100 : 0;

  return (
    <section className="space-y-4 p-6">
      <PageHeader page="update">
        <button onClick={onRecheck} disabled={updating} className={SMALL_BUTTON}>
          Buscar de nuevo
        </button>
      </PageHeader>
      <p className="text-xs text-muted">
        Mantén el servidor al día para que tus amigos con el Minecraft más nuevo puedan entrar.
      </p>

      {done && (
        <p role="status" className="rounded-xl border border-grass bg-grass/10 p-4 text-sm">
          Servidor actualizado a la versión {done}. Tus mundos y packs siguen ahí.
        </p>
      )}

      <div className="space-y-3 rounded-xl border border-line bg-panel p-5">
        {checkError ? (
          <p className="text-sm text-amber">No se pudo consultar la última versión: {checkError}</p>
        ) : !check ? (
          <p role="status" className="text-sm text-muted">Buscando la última versión…</p>
        ) : (
          <>
            <div className="flex flex-wrap gap-6">
              <div>
                <p className="text-xs text-muted">Tu versión</p>
                <p className="font-mono text-lg font-semibold">{check.current ?? "Desconocida"}</p>
              </div>
              <div>
                <p className="text-xs text-muted">Última versión</p>
                <p className="font-mono text-lg font-semibold">{check.latest}</p>
              </div>
            </div>
            {!check.available ? (
              <p className="text-sm text-grass-light">Tu servidor está al día.</p>
            ) : (
              <>
                <p className="text-sm">
                  {check.current
                    ? "Hay una versión nueva disponible."
                    : "No se sabe qué versión tienes. Puedes instalar la última para asegurarte."}
                </p>
                {updating ? (
                  <div className="space-y-2">
                    <div role="progressbar" aria-valuenow={Math.round(percent)} className="h-2 overflow-hidden rounded-full bg-line">
                      <div className="h-full bg-grass-light" style={{ width: `${percent}%` }} />
                    </div>
                    <p role="status" className="text-sm text-muted">
                      {progress ? progressLabel(progress) : "Guardando copia de tu mundo…"}
                    </p>
                  </div>
                ) : (
                  <>
                    <button onClick={update} disabled={!stopped} className={PRIMARY_BUTTON}>
                      Actualizar a {check.latest}
                    </button>
                    {!stopped && <p className="text-sm text-muted">Detén el servidor para actualizarlo.</p>}
                  </>
                )}
              </>
            )}
          </>
        )}
      </div>

      <div className="space-y-1 text-sm text-muted">
        <p className="font-semibold text-ink">Qué se conserva al actualizar</p>
        <p>
          Tus mundos, tus ajustes de server.properties, la lista de permitidos, los permisos y tus packs de texturas
          y addons. Las opciones nuevas que traiga Minecraft se añaden con sus valores por defecto. Si algo falla, el
          servidor queda como estaba.
        </p>
      </div>
    </section>
  );
}
