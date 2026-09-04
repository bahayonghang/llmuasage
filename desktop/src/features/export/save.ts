import { save } from "@tauri-apps/plugin-dialog";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { analyticsCsvFileName } from "./csv";

export async function saveAnalyticsCsv(
  csv: string,
  now = new Date(),
): Promise<string | null> {
  const path = await save({
    defaultPath: analyticsCsvFileName(now),
    filters: [{ name: "CSV", extensions: ["csv"] }],
  });
  if (!path) {
    return null;
  }
  await writeTextFile(path, csv);
  return path;
}
