import { useEffect, useState, type FormEvent } from "react";
import { api, type AllowList, type Player, type ServerState } from "../lib/api";

const SMALL_BUTTON = "rounded-lg bg-line px-3 py-1.5 text-sm hover:brightness-125 disabled:opacity-40";

interface Props {
  state: ServerState;
  players: Player[];
  run: (action: () => Promise<unknown>) => Promise<boolean>;
}

export function Players({ state, players, run }: Props) {
  const [list, setList] = useState<AllowList | null>(null);
  const [name, setName] = useState("");
  const running = state === "running";
  const busy = state === "starting" || state === "stopping";

  // The server fills in a player's xuid when they join, so reload on changes.
  useEffect(() => {
    run(async () => setList(await api.allowList()));
  }, [run, state, players]);

  async function add(event: FormEvent) {
    event.preventDefault();
    const added = await run(async () => setList(await api.addAllowed(name)));
    if (added) setName("");
  }

  const isAllowed = (player: string) =>
    list?.entries.some((entry) => entry.name.toLowerCase() === player.toLowerCase());

  return (
    <section className="space-y-4 p-6">
      <header>
        <h1 className="text-2xl font-semibold">Jugadores</h1>
        <p className="mt-1 text-sm text-muted">
          Decide quién puede entrar. Los cambios se aplican al momento.
        </p>
      </header>

      <div className="rounded-xl border border-line bg-panel p-4">
        <h2 className="text-sm font-semibold">Jugadores conectados</h2>
        {players.length === 0 ? (
          <p className="mt-2 text-sm text-muted">No hay nadie conectado.</p>
        ) : (
          <ul className="mt-2 divide-y divide-line">
            {players.map((player) => (
              <li key={player.name} className="flex items-center gap-2 py-2 text-sm">
                <span className="mr-auto min-w-0 truncate">{player.name}</span>
                {list?.enabled === false || isAllowed(player.name) ? null : (
                  <button
                    onClick={() => run(async () => setList(await api.addAllowed(player.name)))}
                    className={SMALL_BUTTON}
                  >
                    Permitir
                  </button>
                )}
                <button
                  onClick={() => run(() => api.kick(player.name))}
                  disabled={!running}
                  className={SMALL_BUTTON}
                >
                  Expulsar
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      {list && (
        <div className="space-y-3 rounded-xl border border-line bg-panel p-4">
          <label className="flex items-center gap-3">
            <span className="mr-auto text-sm font-semibold">Lista de permitidos</span>
            <span className="text-sm text-muted">{list.enabled ? "Activada" : "Desactivada"}</span>
            <input
              type="checkbox"
              checked={list.enabled}
              disabled={busy}
              onChange={(event) => {
                const enabled = event.target.checked;
                run(async () => setList(await api.setAllowListEnabled(enabled)));
              }}
              className="size-4 accent-grass"
            />
          </label>

          {!list.enabled && (
            <p className="rounded-lg bg-amber/10 p-3 text-sm text-amber">
              Cualquiera que tenga la dirección de tu servidor puede entrar.
            </p>
          )}
          {list.enabled && list.entries.length === 0 && (
            <p className="rounded-lg bg-amber/10 p-3 text-sm text-amber">
              La lista está vacía: nadie puede entrar. Agrega tu Gamertag.
            </p>
          )}

          <form onSubmit={add} className="flex gap-2">
            <input
              value={name}
              onChange={(event) => setName(event.target.value)}
              disabled={busy}
              maxLength={32}
              aria-label="Gamertag"
              placeholder="Gamertag del jugador"
              className="min-w-0 flex-1 rounded-lg border border-line bg-base px-3 py-2 text-sm select-text disabled:opacity-50"
            />
            <button
              type="submit"
              disabled={busy || !name.trim()}
              className="rounded-lg bg-grass px-4 py-2 text-sm font-semibold hover:brightness-110 disabled:opacity-40"
            >
              Agregar
            </button>
          </form>

          {list.entries.length > 0 && (
            <ul className="divide-y divide-line">
              {list.entries.map((entry) => (
                <li key={entry.name} className="flex items-center gap-2 py-2 text-sm">
                  <span className="mr-auto min-w-0 truncate">{entry.name}</span>
                  {!entry.xuid && <span className="text-xs text-muted">Aún no ha entrado</span>}
                  <button
                    onClick={() => run(async () => setList(await api.removeAllowed(entry.name)))}
                    disabled={busy}
                    className={SMALL_BUTTON}
                  >
                    Quitar
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </section>
  );
}
