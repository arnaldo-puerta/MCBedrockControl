import { useCallback, useEffect, useState } from "react";
import { ask, open } from "@tauri-apps/plugin-dialog";
import { api, type PackInfo, type Packs, type ServerState } from "../lib/api";
import { PRIMARY_BUTTON, SMALL_BUTTON } from "../lib/format";
import { PageHeader } from "../lib/pages";

function PackRow({ pack, busy, onToggle, onRemove }: {
  pack: PackInfo;
  busy: boolean;
  onToggle: (enabled: boolean) => void;
  onRemove: () => void;
}) {
  return (
    <li className="flex flex-wrap items-center gap-3 rounded-xl border border-line bg-panel p-4">
      <input
        type="checkbox"
        checked={pack.enabled}
        disabled={busy}
        onChange={(event) => onToggle(event.target.checked)}
        aria-label={`Activar ${pack.name}`}
        className="size-4 accent-grass"
      />
      <div className="mr-auto min-w-0">
        <p className="truncate font-semibold">{pack.name}</p>
        <p className="truncate text-xs text-muted">
          Versión {pack.version.join(".")}
          {pack.description && ` · ${pack.description}`}
        </p>
        {pack.needsExperiments && (
          <p className="text-xs text-amber">
            Usa funciones experimentales: actívalas en el mundo desde Minecraft o puede que no funcione.
          </p>
        )}
      </div>
      <button onClick={onRemove} disabled={busy} className={SMALL_BUTTON}>
        Quitar
      </button>
    </li>
  );
}

interface Props {
  state: ServerState;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
}

export function PacksPage({ state, run }: Props) {
  const [data, setData] = useState<Packs | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [changed, setChanged] = useState(false);
  const running = state === "running" || state === "starting";

  const refresh = useCallback(() => run(async () => setData(await api.packs())), [run]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  async function task(label: string, action: () => Promise<Packs>) {
    setBusy(label);
    const ok = await run(async () => setData(await action()));
    setBusy(null);
    if (ok) setChanged(true);
  }

  async function importFile() {
    const paths = await open({
      title: "Agregar packs o addons",
      multiple: true,
      filters: [{ name: "Packs y addons de Minecraft", extensions: ["mcpack", "mcaddon", "zip"] }],
    });
    if (!paths) return;
    for (const path of Array.isArray(paths) ? paths : [paths]) {
      await task("Instalando…", () => api.importPacks(path));
    }
  }

  async function remove(pack: PackInfo) {
    const confirmed = await ask(`Se quitará "${pack.name}" del servidor.`, {
      title: "Quitar pack",
      kind: "warning",
      okLabel: "Quitar",
      cancelLabel: "Cancelar",
    });
    if (confirmed) await task("Quitando…", () => api.removePack(pack));
  }

  const resource = data?.packs.filter((p) => p.kind === "resource") ?? [];
  const behavior = data?.packs.filter((p) => p.kind === "behavior") ?? [];

  const section = (title: string, hint: string, packs: PackInfo[]) => (
    <div className="space-y-2">
      <div>
        <h2 className="font-semibold">{title}</h2>
        <p className="text-xs text-muted">{hint}</p>
      </div>
      {packs.length === 0 ? (
        <p className="rounded-xl border border-line bg-panel p-4 text-sm text-muted">Ninguno todavía.</p>
      ) : (
        <ul className="space-y-2">
          {packs.map((pack) => (
            <PackRow
              key={`${pack.kind}-${pack.folder}`}
              pack={pack}
              busy={!!busy}
              onToggle={(enabled) => task("Guardando…", () => api.setPackEnabled(pack, enabled))}
              onRemove={() => remove(pack)}
            />
          ))}
        </ul>
      )}
    </div>
  );

  return (
    <section className="space-y-5 p-6">
      <PageHeader page="packs">
        <button onClick={importFile} disabled={!!busy} className={PRIMARY_BUTTON}>
          Agregar packs
        </button>
      </PageHeader>
      <p className="text-xs text-muted">
        Acepta archivos .mcpack, .mcaddon y .zip. Se instalan y se activan en tu mundo solos.
      </p>

      {busy && <p role="status" className="text-sm text-amber">{busy}</p>}
      {changed && running && (
        <p className="rounded-lg bg-amber/10 p-3 text-sm text-amber">
          Reinicia el servidor para que tus amigos vean los cambios.
        </p>
      )}
      {data && !data.hasWorld && (
        <p className="rounded-lg bg-amber/10 p-3 text-sm text-amber">
          Tu servidor aún no tiene un mundo. Inícialo una vez y luego activa aquí tus packs.
        </p>
      )}

      {data && (
        <>
          {section("Texturas", "Cambian cómo se ve el juego. Tus amigos los descargan al entrar.", resource)}
          {section("Addons", "Cambian cómo funciona el juego: criaturas, objetos, reglas.", behavior)}

          <label className="flex items-start gap-3 rounded-xl border border-line bg-panel p-4">
            <input
              type="checkbox"
              checked={data.texturepackRequired}
              disabled={!!busy}
              onChange={(event) => {
                const required = event.target.checked;
                task("Guardando…", () => api.setTexturepackRequired(required));
              }}
              className="mt-0.5 size-4 accent-grass"
            />
            <span>
              <span className="block text-sm font-semibold">Obligar a usar las texturas del servidor</span>
              <span className="block text-xs text-muted">
                Tus amigos tendrán que aceptar la descarga para poder entrar.
              </span>
            </span>
          </label>
        </>
      )}
    </section>
  );
}
