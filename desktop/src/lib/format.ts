export function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

/** Backups are named with `2026-11-07 18-56-47`, which is filename-safe. */
export function formatBackupDate(created: string) {
  const [date, time = ""] = created.split(" ");
  return `${date} ${time.split("-").join(":")}`;
}

export const SMALL_BUTTON =
  "rounded-lg bg-line px-3 py-1.5 text-sm hover:brightness-125 disabled:opacity-40";
export const PRIMARY_BUTTON =
  "rounded-lg bg-grass px-4 py-2 text-sm font-semibold hover:brightness-110 disabled:opacity-40";
