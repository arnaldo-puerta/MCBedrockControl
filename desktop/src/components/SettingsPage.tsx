import { useEffect, useState, type ReactNode } from "react";
import { Gamepad2, Gauge, Save, Tag, Users, Wrench, type LucideIcon } from "lucide-react";
import {
  api,
  type AppSnapshot,
  type Difficulty,
  type GameMode,
  type Permission,
  type ServerSettings,
  type ServerState,
} from "../lib/api";
import { PRIMARY_BUTTON, SMALL_BUTTON } from "../lib/format";
import { PageHeader } from "../lib/pages";

const GAMEMODES: [GameMode, string][] = [
  ["survival", "Supervivencia"],
  ["creative", "Creativo"],
  ["adventure", "Aventura"],
];
const DIFFICULTIES: [Difficulty, string][] = [
  ["peaceful", "Pacífico"],
  ["easy", "Fácil"],
  ["normal", "Normal"],
  ["hard", "Difícil"],
];
const PERMISSIONS: [Permission, string][] = [
  ["visitor", "Visitante: solo mira"],
  ["member", "Miembro: juega normal"],
  ["operator", "Operador: puede usar comandos"],
];

const INPUT = "w-full rounded-lg border border-line bg-base px-3 py-2 text-sm select-text disabled:opacity-50";

function Section({ icon: Icon, title, children }: { icon: LucideIcon; title: string; children: ReactNode }) {
  return (
    <div className="space-y-4 rounded-xl border border-line bg-panel p-5">
      <h2 className="flex items-center gap-2 font-semibold">
        <Icon className="size-4 text-grass-light" aria-hidden />
        {title}
      </h2>
      {children}
    </div>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="block space-y-1.5">
      <span className="block text-sm font-medium">{label}</span>
      {children}
      {hint && <span className="block text-xs text-muted">{hint}</span>}
    </label>
  );
}

function Toggle({ label, hint, checked, onChange }: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-start gap-3">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="mt-0.5 size-4 accent-grass"
      />
      <span>
        <span className="block text-sm font-medium">{label}</span>
        {hint && <span className="block text-xs text-muted">{hint}</span>}
      </span>
    </label>
  );
}

function Select<T extends string>({ value, options, onChange }: {
  value: T;
  options: [T, string][];
  onChange: (value: T) => void;
}) {
  return (
    <select value={value} onChange={(event) => onChange(event.target.value as T)} className={INPUT}>
      {options.map(([id, label]) => (
        <option key={id} value={id}>{label}</option>
      ))}
    </select>
  );
}

function NumberInput({ value, min, max, onChange }: {
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
}) {
  return (
    <input
      type="number"
      min={min}
      max={max}
      value={value}
      onChange={(event) => onChange(Number(event.target.value))}
      className={INPUT}
    />
  );
}

interface Props {
  state: ServerState;
  run: (action: () => Promise<unknown>) => Promise<boolean>;
  adopt: (snapshot: AppSnapshot) => void;
}

export function SettingsPage({ state, run, adopt }: Props) {
  const [saved, setSaved] = useState<ServerSettings | null>(null);
  const [form, setForm] = useState<ServerSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [needsRestart, setNeedsRestart] = useState(false);
  const running = state === "running" || state === "starting";

  useEffect(() => {
    run(async () => {
      const settings = await api.settings();
      setSaved(settings);
      setForm(settings);
    });
  }, [run]);

  if (!form || !saved) {
    return (
      <section className="space-y-4 p-6">
        <PageHeader page="settings" />
        <p role="status" className="text-sm text-muted">Cargando…</p>
      </section>
    );
  }

  const set = <K extends keyof ServerSettings>(key: K, value: ServerSettings[K]) =>
    setForm((current) => current && { ...current, [key]: value });
  const changed = JSON.stringify(form) !== JSON.stringify(saved);
  const renamingWorld = form.worldName !== saved.worldName;

  async function save(restart: boolean) {
    setBusy(true);
    const ok = await run(async () => {
      adopt(await api.saveSettings(form!));
      const fresh = await api.settings();
      setSaved(fresh);
      setForm(fresh);
      if (restart) await api.restart();
    });
    setBusy(false);
    if (ok) setNeedsRestart(running && !restart);
  }

  return (
    <section className="space-y-4 p-6 pb-24">
      <PageHeader page="settings" />

      {needsRestart && (
        <div className="flex flex-wrap items-center gap-3 rounded-xl border border-amber/50 bg-amber/10 p-3 text-sm text-amber">
          <span className="mr-auto">Guardado. Reinicia el servidor para aplicar los cambios.</span>
          <button
            onClick={async () => {
              if (await run(api.restart)) setNeedsRestart(false);
            }}
            className={SMALL_BUTTON}
          >
            Reiniciar ahora
          </button>
        </div>
      )}

      <Section icon={Tag} title="Nombres">
        <Field label="Nombre del servidor" hint="Es lo que ven tus amigos en la lista de servidores.">
          <input
            value={form.serverName}
            maxLength={64}
            onChange={(event) => set("serverName", event.target.value)}
            className={INPUT}
          />
        </Field>
        {form.worldName !== null && (
          <Field
            label="Nombre del mundo"
            hint={
              running
                ? "Detén el servidor para cambiar el nombre del mundo."
                : "Antes de cambiarlo se guarda una copia de seguridad del mundo."
            }
          >
            <input
              value={form.worldName}
              maxLength={64}
              disabled={running}
              onChange={(event) => set("worldName", event.target.value)}
              className={INPUT}
            />
          </Field>
        )}
      </Section>

      <Section icon={Gamepad2} title="Modo de juego">
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Modo" hint="Con el que entran los jugadores nuevos.">
            <Select value={form.gamemode} options={GAMEMODES} onChange={(v) => set("gamemode", v)} />
          </Field>
          <Field label="Dificultad">
            <Select value={form.difficulty} options={DIFFICULTIES} onChange={(v) => set("difficulty", v)} />
          </Field>
        </div>
        <Toggle
          label="Usar este modo para todos"
          hint="Cada vez que alguien entre, se le pondrá este modo aunque antes tuviera otro."
          checked={form.forceGamemode}
          onChange={(v) => set("forceGamemode", v)}
        />
        <Toggle
          label="Permitir trucos"
          hint="Permite usar comandos como /gamemode o /tp dentro del juego."
          checked={form.allowCheats}
          onChange={(v) => set("allowCheats", v)}
        />
      </Section>

      <Section icon={Users} title="Jugadores">
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Máximo de jugadores">
            <NumberInput value={form.maxPlayers} min={1} max={100} onChange={(v) => set("maxPlayers", v)} />
          </Field>
          <Field label="Expulsar inactivos tras (minutos)" hint="0 para no expulsar nunca.">
            <NumberInput
              value={form.idleTimeoutMinutes}
              min={0}
              max={1440}
              onChange={(v) => set("idleTimeoutMinutes", v)}
            />
          </Field>
        </div>
        <Field label="Permiso de los jugadores nuevos">
          <Select value={form.defaultPermission} options={PERMISSIONS} onChange={(v) => set("defaultPermission", v)} />
        </Field>
      </Section>

      <Section icon={Gauge} title="Rendimiento">
        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Distancia de visión (chunks)" hint="Menos es más rápido en equipos modestos.">
            <NumberInput value={form.viewDistance} min={5} max={96} onChange={(v) => set("viewDistance", v)} />
          </Field>
          <Field label="Distancia de simulación (chunks)" hint="Hasta dónde funcionan granjas y redstone.">
            <NumberInput value={form.tickDistance} min={4} max={12} onChange={(v) => set("tickDistance", v)} />
          </Field>
        </div>
      </Section>

      <Section icon={Wrench} title="Avanzado">
        <Field label="Puerto" hint="Si lo cambias, cambia también el túnel de Playit para que apunte al nuevo puerto.">
          <NumberInput value={form.port} min={1} max={65535} onChange={(v) => set("port", v)} />
        </Field>
        <Toggle
          label="Comprobar cuentas de Xbox"
          hint="Recomendado. Si lo desactivas, cualquiera podría entrar con el nombre de otra persona."
          checked={form.onlineMode}
          onChange={(v) => set("onlineMode", v)}
        />
      </Section>

      {changed && (
        <div className="sticky bottom-4 flex flex-wrap items-center gap-2 rounded-xl border border-grass bg-panel p-3 shadow-lg">
          <span className="mr-auto text-sm">
            Tienes cambios sin guardar{renamingWorld ? ", incluido el nombre del mundo" : ""}.
          </span>
          <button onClick={() => setForm(saved)} disabled={busy} className={SMALL_BUTTON}>
            Deshacer
          </button>
          {running && (
            <button onClick={() => save(true)} disabled={busy} className={SMALL_BUTTON}>
              Guardar y reiniciar
            </button>
          )}
          <button onClick={() => save(false)} disabled={busy} className={`${PRIMARY_BUTTON} flex items-center gap-2`}>
            <Save className="size-4" aria-hidden />
            {busy ? "Guardando…" : "Guardar"}
          </button>
        </div>
      )}
    </section>
  );
}
