import type { ReactNode } from "react";
import {
  Archive,
  Globe,
  Map,
  Package,
  RefreshCw,
  Server,
  Settings,
  SquareTerminal,
  Users,
  type LucideIcon,
} from "lucide-react";

export type Page =
  | "server"
  | "console"
  | "players"
  | "connections"
  | "worlds"
  | "packs"
  | "backups"
  | "settings"
  | "update";

interface PageInfo {
  label: string;
  icon: LucideIcon;
  /** One line under the title saying what the page is for. */
  description: string;
}

export const PAGES: Record<Page, PageInfo> = {
  server: {
    label: "Servidor",
    icon: Server,
    description: "Enciende, apaga o reinicia tu servidor y mira cómo está funcionando.",
  },
  console: {
    label: "Consola",
    icon: SquareTerminal,
    description: "Mira lo que pasa en el servidor y escribe comandos de Minecraft.",
  },
  players: {
    label: "Jugadores",
    icon: Users,
    description: "Mira quién está conectado y decide quién puede entrar.",
  },
  connections: {
    label: "Conexiones",
    icon: Globe,
    description: "Consigue una dirección para que tus amigos entren desde cualquier lugar.",
  },
  worlds: {
    label: "Mundos",
    icon: Map,
    description: "Importa, exporta y cambia el mundo que usa el servidor.",
  },
  packs: {
    label: "Packs y addons",
    icon: Package,
    description: "Agrega texturas y addons a tu mundo.",
  },
  backups: {
    label: "Copias de seguridad",
    icon: Archive,
    description: "Guarda copias de tu mundo y recupéralas si algo sale mal.",
  },
  settings: {
    label: "Configuración",
    icon: Settings,
    description: "Cambia el nombre, el modo de juego, la dificultad y otras opciones.",
  },
  update: {
    label: "Actualización",
    icon: RefreshCw,
    description: "Actualiza el servidor a la última versión sin perder nada.",
  },
};

export const PAGE_ORDER: Page[] = [
  "server",
  "console",
  "players",
  "connections",
  "worlds",
  "packs",
  "backups",
  "settings",
  "update",
];

interface HeaderProps {
  page: Page;
  /** Replaces the page name, as the dashboard does with the server's name. */
  title?: string;
  children?: ReactNode;
}

/** Icon, title and what-you-can-do line, with the page's buttons on the right. */
export function PageHeader({ page, title, children }: HeaderProps) {
  const { icon: Icon, label, description } = PAGES[page];
  return (
    <header className="flex flex-wrap items-start gap-4">
      <span className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-grass/20 text-grass-light">
        <Icon className="size-6" aria-hidden />
      </span>
      <div className="mr-auto min-w-0">
        <h1 className="truncate text-2xl font-semibold">{title ?? label}</h1>
        <p className="mt-1 text-sm text-muted">{description}</p>
      </div>
      {children && <div className="flex flex-wrap items-center gap-2">{children}</div>}
    </header>
  );
}
